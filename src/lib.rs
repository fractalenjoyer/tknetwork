use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
include!(concat!(env!("OUT_DIR"), "/module.rs"));

use serde::{Deserialize, Serialize};

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

#[pyclass]
struct Event {
    callback: Option<Py<PyAny>>,
}

#[pymethods]
impl Event {
    #[new]
    const fn new() -> Self {
        Self { callback: None }
    }

    fn __call__(&mut self, py: Python, func: Py<PyAny>) -> Py<PyAny> {
        self.callback = Some(func.clone_ref(py));
        func
    }

    fn call(&self, py: Python, args: &PyTuple, kwargs: Option<&PyDict>) -> PyResult<()> {
        if let Some(callback) = &self.callback {
            callback.call(py, args, kwargs)?;
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    event: String,
    data: String,
}

struct ThreadMessage {
    event: String,
    peer: Option<Py<Peer>>,
    data: String,
}

#[pyclass]
struct Peer {
    #[pyo3(get)]
    name: String,
    events: HashMap<String, Py<Event>>,
    socket: RefCell<TcpStream>,
    tx: Sender<ThreadMessage>,
}

#[pymethods]
impl Peer {
    fn on(&mut self, py: Python, name: String) -> PyResult<Py<Event>> {
        let event = Py::new(py, Event { callback: None })?;
        self.events.insert(name, event.clone_ref(py));
        Ok(event)
    }

    fn emit(&self, event: String, data: String) -> PyResult<()> {
        let mut buffer = serde_json::to_vec(&Message { event, data }).unwrap();
        buffer.push(0x4);
        self.socket.borrow_mut().write_all(&buffer)?;
        Ok(())
    }
}

impl Peer {
    fn new(
        py: Python,
        address: String,
        socket: TcpStream,
        tx: &Sender<ThreadMessage>,
    ) -> PyResult<Py<Self>> {
        let peer = Py::new(
            py,
            Self {
                name: address,
                events: HashMap::new(),
                socket: RefCell::new(socket.try_clone()?),
                tx: tx.clone(),
            },
        )?;

        let peer_clone: Py<Self> = peer.clone_ref(py);
        thread::spawn(move || Self::listen(&peer_clone, socket));

        tx.send(ThreadMessage {
            event: "connect".to_string(),
            peer: Some(peer.clone_ref(py)),
            data: String::new(),
        })
        .unwrap();

        Ok(peer)
    }

    fn trigger(&self, py: Python, name: &str, data: String) -> PyResult<()> {
        let args = PyTuple::new(py, [&data]);
        if let Some(event) = self.events.get(name) {
            event.borrow(py).call(py, args, None)?;
        }
        self.tx
            .send(ThreadMessage {
                event: name.to_string(),
                peer: None,
                data,
            })
            .unwrap();
        Ok(())
    }

    fn listen(peer: &Py<Self>, socket: TcpStream) {
        let reader = BufReader::new(socket);

        for line in reader.split(0x4) {
            if let Ok(buffer) = line {
                Self::decode_message(peer, &buffer);
            } else {
                Python::with_gil(|py| {
                    peer.borrow(py).tx.send(ThreadMessage {
                        event: "disconnect".to_string(),
                        peer: Some(peer.clone_ref(py)),
                        data: String::new(),
                    })
                })
                .unwrap();
                break;
            }
        }
    }

    fn decode_message(peer: &Py<Self>, buffer: &[u8]) {
        let message: Message = if let Ok(message) = serde_json::from_slice(buffer) {
            message
        } else {
            println!("Error: Malformed packet");
            return;
        };

        Python::with_gil(|py| {
            if let Err(e) = peer.borrow(py).trigger(py, &message.event, message.data) {
                println!("Error: {e}");
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
    peers: RefCell<Vec<Py<Peer>>>,
}

#[pymethods]
impl Network {
    #[new]
    fn new(ip: String, port: u16) -> Self {
        Self {
            ip,
            port,
            tx: None,
            events: HashMap::new(),
            peers: RefCell::new(Vec::new()),
        }
    }

    fn connect(&self, ip: &str, port: u16) -> PyResult<()> {
        let socket = UdpSocket::bind("0.0.0.0:7337")?;
        socket.send_to(&self.port.to_be_bytes(), (ip, port))?;
        Ok(())
    }

    fn on(&mut self, py: Python, name: String) -> PyResult<Py<Event>> {
        let event = Py::new(py, Event { callback: None })?;
        self.events.insert(name, event.clone_ref(py));
        Ok(event)
    }

    fn emit(&self, py: Python, event: &str, data: &str) {
        self.peers.borrow_mut().retain(|peer| {
            peer.borrow(py)
                .emit(event.to_string(), data.to_string())
                .is_ok()
        });
    }

    #[pyo3(signature = (tcp = true, udp = true))]
    fn serve(mut slf: PyRefMut<'_, Self>, py: Python, tcp: bool, udp: bool) {
        let (tx, rx) = channel();
        let ip = slf.ip.clone();
        let port = slf.port;
        slf.tx = Some(tx);
        let network: Py<Self> = slf.into();

        {
            let slf = network.clone_ref(py);
            thread::Builder::new()
                .name("listen".to_string())
                .spawn(move || Self::listen(&slf, rx))
                .unwrap();
        };
        if tcp {
            let slf = network.clone_ref(py);
            let ip = ip.clone();
            thread::Builder::new()
                .name("tcp_server".to_string())
                .spawn(move || Self::tcp_server(&slf, ip, port))
                .unwrap();
        };
        if udp {
            let slf = network.clone_ref(py);
            thread::Builder::new()
                .name("udp_server".to_string())
                .spawn(move || Self::udp_server(&slf, ip, port))
                .unwrap();
        };
    }
}

impl Network {
    fn listen(slf: &Py<Self>, rx: Receiver<ThreadMessage>) {
        for message in rx {
            Python::with_gil(|py| {
                let slf = slf.borrow(py);
                match message.event.as_str() {
                    "connection_request" => {
                        if let Some((ip, port)) = message.data.split_once(':') {
                            if let Ok(port) = port.parse::<u16>() {
                                slf.tcp_connect(py, ip, port).unwrap();
                            }
                        }
                    }
                    "connect" | "disconnect" => {
                        if let Some(event) = slf.events.get(&message.event) {
                            let args = PyTuple::new(py, &[message.peer]);
                            event.borrow(py).call(py, args, None).unwrap();
                        }
                    }
                    _ => {
                        if let Some(event) = slf.events.get(&message.event) {
                            let args = PyTuple::new(py, &[message.data]);
                            event.borrow(py).call(py, args, None).unwrap();
                        }
                    }
                }
            });
        }
    }

    fn tcp_connect(&self, py: Python, ip: &str, port: u16) -> PyResult<()> {
        let socket = TcpStream::connect((ip, port))?;
        let peer = Peer::new(
            py,
            format!("{ip}:{port}"),
            socket,
            &self.tx.clone().unwrap(),
        )?;
        self.peers.borrow_mut().push(peer);
        Ok(())
    }

    fn tcp_server(slf: &Py<Self>, ip: String, port: u16) {
        let listener = TcpListener::bind((ip, port)).unwrap();

        for stream in listener.incoming() {
            let socket = match stream {
                Ok(socket) => socket,
                Err(_) => continue,
            };

            Python::with_gil(|py| {
                let slf = slf.borrow(py);

                Peer::new(
                    py,
                    socket.peer_addr().unwrap().to_string(),
                    socket,
                    &slf.tx.clone().unwrap(),
                )
                .map_or_else(
                    |_| println!("Error: Failed to create peer"),
                    |peer| slf.peers.borrow_mut().push(peer),
                );
            });
        }
    }

    fn udp_server(slf: &Py<Self>, ip: String, port: u16) {
        let socket = UdpSocket::bind((ip, port)).unwrap();

        loop {
            let mut buffer = [0; 2];
            let (_bytes_read, address) = match socket.recv_from(&mut buffer) {
                Ok((bytes_read, address)) => (bytes_read, address),
                Err(e) => {
                    println!("Error: {e}");
                    continue;
                }
            };

            Python::with_gil(|py| {
                let slf = slf.borrow(py);
                let address = format!("{}", address.ip());
                let port = u16::from_be_bytes(buffer);
                slf.emit(py, "connection_request", &format!("{address}:{port}"));
                slf.tcp_connect(py, address.as_str(), port).unwrap();
            });
        }
    }
}
