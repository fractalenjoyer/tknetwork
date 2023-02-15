use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
include!(concat!(env!("OUT_DIR"), "/module.rs"));

use serde::{Deserialize, Serialize};
use serde_json;

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::thread;

#[pyclass]
struct Event {
    callback: Option<Py<PyAny>>,
}

#[pymethods]
impl Event {
    #[new]
    fn new() -> Self {
        Self { callback: None }
    }

    fn __call__(&mut self, py: Python, func: Py<PyAny>) -> PyResult<Py<PyAny>> {
        self.callback = Some(func.clone_ref(py));
        Ok(func)
    }

    fn call(&self, py: Python, args: &PyTuple, kwargs: Option<&PyDict>) -> PyResult<()> {
        if let Some(callback) = &self.callback {
            callback.call(py, args, kwargs)?;
        }
        Ok(())
    }

    fn __repr__(&self, py: Python) -> PyResult<String> {
        if let Some(callback) = &self.callback {
            Ok(format!("Event({})", callback.getattr(py, "__name__")?))
        } else {
            Ok("Event(None)".to_string())
        }
    }
}

#[pyclass]
struct EventManager {
    events: HashMap<String, Py<Event>>,
}

#[pymethods]
impl EventManager {
    #[new]
    fn new() -> Self {
        Self {
            events: HashMap::new(),
        }
    }

    fn bind(&mut self, py: Python, name: String) -> PyResult<Py<Event>> {
        let event = Py::new(py, Event::new())?;
        self.events.insert(name, event.clone_ref(py));
        Ok(event)
    }

    #[pyo3(signature = (name, *args, **kwargs))]
    fn trigger(
        &self,
        py: Python,
        name: &str,
        args: &PyTuple,
        kwargs: Option<&PyDict>,
    ) -> PyResult<bool> {
        if let Some(event) = self.events.get(name) {
            event.borrow(py).call(py, args, kwargs)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn __repr__(&self, py: Python) -> PyResult<String> {
        let mut repr = "EventManager(".to_string();
        for (name, event) in &self.events {
            repr.push_str(&format!("{}: {}, ", name, event.borrow(py).__repr__(py)?));
        }
        repr.push(')');
        Ok(repr)
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    event: String,
    data: String,
}

#[pyclass]
struct Peer {
    #[pyo3(get)]
    address: String,
    #[pyo3(get)]
    events: Py<EventManager>,
    global_events: Py<EventManager>,
    read: TcpStream,
    write: RefCell<TcpStream>,
}

#[pymethods]
impl Peer {
    fn trigger(
        &self,
        py: Python,
        name: &str,
        args: &PyTuple,
        kwargs: Option<&PyDict>,
    ) -> PyResult<()> {
        if self.events.borrow(py).trigger(py, name, args, kwargs)? {
            Ok(())
        } else {
            self.global_events
                .borrow(py)
                .trigger(py, name, args, kwargs)?;
            Ok(())
        }
    }

    fn emit(&self, event: String, data: String) -> PyResult<()> {
        let mut message = serde_json::to_string(&Message { event, data }).unwrap();
        message.push(char::from(0x4));
        self.write.borrow_mut().write_all(message.as_bytes())?;
        Ok(())
    }

    fn __repr__(&self, py: Python) -> PyResult<String> {
        Ok(format!(
            "Peer({}, {})",
            self.address,
            self.events.borrow(py).__repr__(py)?
        ))
    }
}

impl Peer {
    fn new(
        py: Python,
        address: String,
        global_events: Py<EventManager>,
        socket: TcpStream,
    ) -> PyResult<Py<Self>> {
        let peer = Py::new(
            py,
            Peer {
                address,
                events: Py::new(py, EventManager::new()).unwrap(),
                global_events: global_events.clone_ref(py),
                read: socket.try_clone().unwrap(),
                write: RefCell::new(socket),
            },
        )?;

        let peer_clone: Py<Peer> = peer.clone_ref(py);
        let socket = peer.borrow(py).read.try_clone()?;
        thread::spawn(move || Self::listen(peer_clone, socket).unwrap());

        global_events.borrow(py).trigger(
            py,
            "new_peer",
            PyTuple::new(py, &[peer.clone_ref(py)]),
            None,
        )?;

        Ok(peer)
    }

    fn listen(peer: Py<Peer>, socket: TcpStream) -> Result<(), ()> {
        let reader = BufReader::new(socket);

        for line in reader.split(0x4) {
            if let Ok(buffer) = line {
                Self::decode_message(&peer, &buffer)
            } else {
                Python::with_gil(|py| {
                    peer.borrow(py).trigger(
                        py,
                        &"peer_disconnect",
                        &PyTuple::new(py, &[peer.clone_ref(py)]),
                        None,
                    )
                })
                .unwrap();
                break
            }
        }
        Ok(())
    }

    fn decode_message(peer: &Py<Peer>, buffer: &[u8]) {
        let message: Message = match serde_json::from_slice(&buffer) {
            Ok(message) => message,
            Err(_) => {
                println!("Error: Malformed packet");
                return;
            }
        };

        Python::with_gil(|py| {
            let args = PyTuple::new(py, &[message.data]);
            if let Err(e) = peer.borrow(py).trigger(py, &message.event, args, None) {
                println!("Error: {}", e);
            }
        });
    }
}

#[pyclass]
struct Network {
    ip: String,
    port: u16,
    #[pyo3(get)]
    events: Py<EventManager>,
    peers: Vec<Py<Peer>>,
}

#[pymethods]
impl Network {
    #[new]
    fn new(py: Python, ip: String, port: u16) -> PyResult<Self> {
        Ok(Self {
            ip,
            port,
            events: Py::new(py, EventManager::new())?,
            peers: Vec::new(),
        })
    }

    fn connect(&mut self, ip: &str, port: u16) -> PyResult<()> {
        let socket = UdpSocket::bind("0.0.0.0:7337")?;
        socket.send_to(&[0], (ip, port))?;
        Ok(())
    }

    fn tcp_connect(&mut self, py: Python, ip: String, port: u16) -> PyResult<()> {
        let socket = TcpStream::connect((ip.clone(), port))?;
        let peer = Peer::new(
            py,
            format!("{}:{}", ip, port),
            self.events.clone_ref(py),
            socket,
        )?;
        self.peers.push(peer);
        Ok(())
    }

    fn emit(&mut self, py: Python, event: String, data: String) -> PyResult<()> {
        self.peers
            .retain(|peer| peer.borrow(py).emit(event.clone(), data.clone()).is_ok());
        Ok(())
    }

    fn tcp_server(slf: PyRefMut<'_, Self>) -> PyResult<()> {
        let listener = TcpListener::bind((slf.ip.clone(), slf.port))?;
        let slf: Py<Self> = slf.into();

        thread::spawn(move || {
            for stream in listener.incoming() {
                let socket = match stream {
                    Ok(stream) => stream,
                    Err(e) => {
                        println!("Error: {}", e);
                        continue;
                    }
                };

                Python::with_gil(|py| {
                    let mut slf = slf.borrow_mut(py);
                    if let Ok(peer) = Peer::new(
                        py,
                        socket.peer_addr().unwrap().to_string(),
                        slf.events.clone_ref(py),
                        socket,
                    ) {
                        slf.peers.push(peer);
                    } else {
                        println!("Error: Failed to create peer");
                    }
                });
            }
        });

        Ok(())
    }

    fn udp_server(slf: PyRef<'_, Self>) -> PyResult<()> {
        let socket = UdpSocket::bind((slf.ip.clone(), slf.port))?;
        let slf: Py<Self> = slf.into();
        thread::spawn(move || {
            let mut buffer = [0; 1024];
            loop {
                let (_bytes_read, address) = match socket.recv_from(&mut buffer) {
                    Ok((bytes_read, address)) => (bytes_read, address),
                    Err(e) => {
                        println!("Error: {}", e);
                        continue;
                    }
                };

                Python::with_gil(|py| {
                    let mut slf = slf.borrow_mut(py);
                    let address = format!("{}", address.ip());
                    let port = slf.port;
                    slf.emit(py, "connect".to_string(), address.clone())
                        .unwrap();
                    slf.tcp_connect(py, address, port).unwrap();
                });
            }
        });

        Ok(())
    }

    fn __repr__(&self, py: Python) -> PyResult<String> {
        let mut repr = format!(
            "Network(IP: {}, Port: {}, Events: {}",
            self.ip,
            self.port,
            self.events.borrow(py).__repr__(py)?
        );
        repr.push_str("Peers: [");
        for peer in &self.peers {
            repr.push_str(&format!("{}, ", peer.borrow(py).__repr__(py)?));
        }
        repr.push_str("])");
        Ok(repr)
    }
}
