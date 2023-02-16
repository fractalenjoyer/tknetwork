use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
include!(concat!(env!("OUT_DIR"), "/module.rs"));

use serde::{Deserialize, Serialize};
use serde_json;

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
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
enum Message {
    #[serde(skip)]
    Connect(Py<Peer>),
    #[serde(skip)]
    Disconnect(Py<Peer>),
    Request(String),
    Packet {
        event: String,
        data: String,
    },
}

use crate::Message::*;

#[pyclass]
struct Peer {
    #[pyo3(get)]
    name: String,
    events: HashMap<String, Py<Event>>,
    socket: RefCell<TcpStream>,
    tx: Sender<Message>,
}

#[pymethods]
impl Peer {
    fn on(&mut self, py: Python, name: String) -> PyResult<Py<Event>> {
        let event = Py::new(py, Event { callback: None })?;
        self.events.insert(name, event.clone_ref(py));
        Ok(event)
    }

    fn emit(&self, event: String, data: String) -> PyResult<()> {
        let mut buffer = serde_json::to_vec(&Packet { event, data }).unwrap();
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
        tx: Sender<Message>,
    ) -> PyResult<Py<Self>> {
        let peer = Py::new(
            py,
            Peer {
                name: address,
                events: HashMap::new(),
                socket: RefCell::new(socket.try_clone()?),
                tx: tx.clone(),
            },
        )?;

        let peer_clone: Py<Peer> = peer.clone_ref(py);
        thread::spawn(move || Self::listen(peer_clone, socket).unwrap());

        tx.send(Connect(peer.clone_ref(py))).unwrap();

        Ok(peer)
    }

    fn trigger(&self, py: Python, message: Message) -> PyResult<()> {
        self.tx.send(message.clone()).unwrap();
        let (name, data) = match message {
            Packet { event, data } => (event, data),
            _ => return Ok(()),
        };
        let args = PyTuple::new(py, &[&data]);
        if let Some(event) = self.events.get(&name) {
            event.borrow(py).call(py, args, None)?;
        }

        Ok(())
    }

    fn listen(peer: Py<Peer>, socket: TcpStream) -> Result<(), ()> {
        let reader = BufReader::new(socket);

        for line in reader.split(0x4) {
            if let Ok(buffer) = line {
                Self::decode_message(&peer, &buffer)
            } else {
                Python::with_gil(|py| peer.borrow(py).tx.send(Disconnect(peer.clone_ref(py))))
                    .unwrap();
                break;
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
            if let Err(e) = peer.borrow(py).trigger(py, message) {
                println!("Error: {}", e);
            }
        });
    }
}

#[pyclass]
struct Network {
    ip: String,
    port: u16,
    tx: Option<Sender<Message>>,
    events: HashMap<String, Py<Event>>,
    peers: Vec<Py<Peer>>,
}

#[pymethods]
impl Network {
    #[new]
    fn new(ip: String, port: u16) -> PyResult<Self> {
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

    fn on(&mut self, py: Python, name: String) -> PyResult<Py<Event>> {
        let event = Py::new(py, Event { callback: None })?;
        self.events.insert(name, event.clone_ref(py));
        Ok(event)
    }

    fn emit(&mut self, py: Python, event: String, data: String) -> PyResult<()> {
        self.peers
            .retain(|peer| peer.borrow(py).emit(event.clone(), data.clone()).is_ok());
        Ok(())
    }

    #[pyo3(signature = (tcp = true, udp = true))]
    fn serve(mut slf: PyRefMut<'_, Self>, py: Python, tcp: bool, udp: bool) -> PyResult<()> {
        let (tx, rx) = channel();
        let ip = slf.ip.clone();
        let port = slf.port;
        slf.tx = Some(tx.clone());
        let network: Py<Self> = slf.into();

        {
            let slf = network.clone_ref(py);
            thread::spawn(move || Self::listen(slf, rx));
        };
        if tcp {
            let slf = network.clone_ref(py);
            let ip = ip.clone();
            thread::spawn(move || Self::tcp_server(slf, ip, port));
        };
        if udp {
            let slf = network.clone_ref(py);
            let ip = ip.clone();
            thread::spawn(move || Self::udp_server(slf, ip, port));
        };

        Ok(())
    }
}

impl Network {
    fn listen(slf: Py<Self>, rx: Receiver<Message>) -> PyResult<()> {
        for message in rx {
            Python::with_gil(|py| {
                let mut slf = slf.borrow_mut(py);
                match message {
                    Request(address) => {
                        let port = slf.port;
                        slf.tcp_connect(py, address, port).unwrap();
                    }
                    Connect(peer) => {
                        if let Some(event) = slf.events.get("connect") {
                            let args = PyTuple::new(py, &[peer]);
                            event.borrow(py).call(py, args, None).unwrap();
                        }
                    }
                    Disconnect(peer) => {
                        if let Some(event) = slf.events.get("disconnect") {
                            let args = PyTuple::new(py, &[peer]);
                            event.borrow(py).call(py, args, None).unwrap();
                        }
                    }
                    Packet { event, data } => {
                        if let Some(event) = slf.events.get(&event) {
                            let args = PyTuple::new(py, &[&data]);
                            event.borrow(py).call(py, args, None).unwrap();
                        }
                    }
                }
            });
        }
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
                    slf.tx.clone().unwrap(),
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
