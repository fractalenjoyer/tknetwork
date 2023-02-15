use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
include!(concat!(env!("OUT_DIR"), "/module.rs"));

use serde::{Deserialize, Serialize};
use serde_json;

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::mpsc::{Sender, Receiver, channel};
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

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    event: String,
    data: String,
}

struct ThreadMessage {
    event: String,
    peer: Py<Peer>,
    data: String,
}

#[pyclass]
struct Peer {
    #[pyo3(get)]
    address: String,
    events: HashMap<String, Py<Event>>,
    write: RefCell<TcpStream>,
    tx: Sender<ThreadMessage>,
}

#[pymethods]
impl Peer {
    fn emit(&self, event: String, data: String) -> PyResult<()> {
        let mut message = serde_json::to_string(&Message { event, data }).unwrap();
        message.push(char::from(0x4));
        self.write.borrow_mut().write_all(message.as_bytes())?;
        Ok(())
    }
}

impl Peer {
    fn new(
        py: Python,
        address: String,
        socket: TcpStream,
        tx: Sender<ThreadMessage>,
    ) -> PyResult<Py<Self>> {
        let peer = Py::new(
            py,
            Peer {
                address,
                events: HashMap::new(),
                write: RefCell::new(socket.try_clone()?),
                tx: tx.clone(),
            },
        )?;

        let peer_clone: Py<Peer> = peer.clone_ref(py);
        thread::spawn(move || Self::listen(peer_clone, socket).unwrap());

        tx.send(ThreadMessage {
            event: "new_peer".to_string(),
            peer: peer.clone_ref(py),
            data: "".to_string(),
        }).unwrap();

        Ok(peer)
    }

    fn trigger(
        &self,
        py: Python,
        name: &str,
        args: &PyTuple,
        kwargs: Option<&PyDict>,
    ) -> PyResult<()> {
        if let Some(event) = self.events.get(name) {
            event.borrow(py).call(py, args, kwargs)?;
        }
        Ok(())
    }

    fn listen(peer: Py<Peer>, socket: TcpStream) -> Result<(), ()> {
        let reader = BufReader::new(socket);

        for line in reader.split(0x4) {
            if let Ok(buffer) = line {
                Self::decode_message(&peer, &buffer)
            } else {
                Python::with_gil(|py| {
                    peer.borrow(py).tx.send(
                        ThreadMessage {
                            event: "peer_disconnect".to_string(),
                            peer: peer.clone_ref(py),
                            data: "".to_string(),
                        }
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
    tx: Option<Sender<ThreadMessage>>,
    events: HashMap<String, Py<Event>>,
    peers: Vec<Py<Peer>>,
}

#[pymethods]
impl Network {
    #[new]
    fn new(py: Python, ip: String, port: u16) -> PyResult<Self> {
        Ok(Self {
            ip,
            port,
            tx: None,
            events: HashMap::new(),
            peers: Vec::new(),
        })
    }

    fn connect(&mut self, ip: &str, port: u16) -> PyResult<()> {
        let socket = UdpSocket::bind("0.0.0.0:7337")?;
        socket.send_to(&[0], (ip, port))?;
        Ok(())
    }

    fn emit(&mut self, py: Python, event: String, data: String) -> PyResult<()> {
        self.peers
            .retain(|peer| peer.borrow(py).emit(event.clone(), data.clone()).is_ok());
        Ok(())
    }

    fn serve(mut slf: PyRefMut<'_, Self>, py: Python) -> PyResult<()> {
        let (tx, rx) = channel();
        slf.tx = Some(tx.clone());
        let slf: Py<Network> = slf.into();
        {
            let slf = slf.clone_ref(py);
            thread::spawn(move || Self::listen(slf, rx).unwrap());
        };
        {
            let slf = slf.clone_ref(py);
            thread::spawn(move || Self::tcp_server(slf, slf.borrow(py).ip.clone(), slf.borrow(py).port));
        };
        {
            let slf = slf.clone_ref(py);
            thread::spawn(move || Self::udp_server(slf, slf.borrow(py).ip.clone(), slf.borrow(py).port));
        }

        Ok(())
    }
}

impl Network {
    fn listen(slf: Py<Self>, rx: Receiver<ThreadMessage>) -> PyResult<()> {
        Ok(())
    }
    fn tcp_connect(&mut self, py: Python, ip: String, port: u16) -> PyResult<()> {
        let socket = TcpStream::connect((ip.clone(), port))?;
        let peer = Peer::new(
            py,
            format!("{}:{}", ip, port),
            socket,
            self.tx.as_ref().unwrap().clone(),
        )?;
        self.peers.push(peer);
        Ok(())
    }

    fn tcp_server(slf: Py<Self>, ip: String, port: u16) {
        let listener = TcpListener::bind((ip, port)).unwrap();

        for stream in listener.incoming() {
            let socket = match stream {
                Ok(socket) => socket,
                Err(_) => continue,
            };

            Python::with_gil(|py| {
                let mut slf = slf.borrow_mut(py);

                if let Ok(peer) = Peer::new(
                    py,
                    socket.peer_addr().unwrap().to_string(),
                    socket,
                    slf.tx.as_ref().unwrap().clone(),
                ) {
                    slf.peers.push(peer);
                } else {
                    println!("Error: Failed to create peer");
                }
            })
        }
    }

    fn udp_server(slf: Py<Self>, ip: String, port: u16) {
        let socket = UdpSocket::bind((ip, port)).unwrap();

        loop {
            let mut buffer = [0; 1024];
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
                slf.emit(py, "connection_request".to_string(), address.clone())
                    .unwrap();
                slf.tcp_connect(py, address, port).unwrap();
            });
        }
    }
}
