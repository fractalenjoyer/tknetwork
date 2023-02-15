use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
include!(concat!(env!("OUT_DIR"), "/module.rs"));

use serde::{Deserialize, Serialize};
use serde_json;

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::{mpsc::{channel, Receiver, Sender}, Arc, Mutex};
use std::thread;

#[pyclass]
struct Event {
    callback: Option<Py<PyAny>>,
}

#[pymethods]
impl Event {
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

struct ThreadMessage {
    event: String,
    peer: Option<Py<Peer>>,
    data: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Message {
    event: String,
    data: String,
}

#[pyclass]
struct Peer {
    #[pyo3(get)]
    name: String,
    stream: RefCell<TcpStream>,
    sender: Sender<ThreadMessage>,
    events: HashMap<String, Py<Event>>,
}

impl Peer {
    fn new(
        py: Python,
        name: String,
        stream: TcpStream,
        sender: Sender<ThreadMessage>,
    ) -> PyResult<Py<Self>> {
        let peer = Py::new(
            py,
            Self {
                name,
                stream: RefCell::new(stream.try_clone()?),
                sender: sender.clone(),
                events: HashMap::new(),
            },
        )?;

        let peer_clone = peer.clone_ref(py);
        thread::spawn(move || Self::listen(peer_clone, stream));

        sender
            .send(ThreadMessage {
                event: "connect".to_string(),
                peer: Some(peer.clone_ref(py)),
                data: None,
            })
            .unwrap();

        Ok(peer)
    }

    fn listen(peer: Py<Self>, socket: TcpStream) {
        let reader = BufReader::new(socket);

        for line in reader.split(0x4) {
            if let Ok(buffer) = line {
                Self::decode_message(&peer, &buffer)
            } else {
                Python::with_gil(|py| {
                    peer.borrow(py)
                        .sender
                        .send(ThreadMessage {
                            event: "disconnect".to_string(),
                            peer: Some(peer.clone_ref(py)),
                            data: None,
                        })
                        .unwrap();
                });
                break;
            }
        }
    }

    fn decode_message(peer: &Py<Peer>, buffer: &[u8]) {
        let message: Message = match serde_json::from_slice(buffer) {
            Ok(message) => message,
            Err(_) => {
                println!("Error: Malformed packet");
                return;
            }
        };

        Python::with_gil(|py| {
            if let Err(e) = Peer::trigger(peer, py, &message) {
                e.print(py);
            }
        });
    }

    fn trigger(peer: &Py<Self>, py: Python, message: &Message) -> PyResult<()> {
        let peer = peer.borrow(py);
        if let Some(event) = peer.events.get(&message.event) {
            event
                .borrow(py)
                .call(py, PyTuple::new(py, &[&message.data]), None)?;
        };

        peer.sender
            .send(ThreadMessage {
                event: message.event.clone(),
                peer: None,
                data: Some(message.data.clone()),
            })
            .unwrap();

        Ok(())
    }
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
        self.stream.borrow_mut().write_all(&buffer)?;
        Ok(())
    }
}

#[pyclass]
struct Network {
    address: String,
    port: u16,
    peers: Vec<Py<Peer>>,
    events: HashMap<String, Py<Event>>,

    receiver: Arc<Mutex<Receiver<ThreadMessage>>>,
    sender: Sender<ThreadMessage>,
}

impl Network {
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
                    slf.sender.clone(),
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

    fn tcp_connect(&mut self, py: Python, ip: String, port: u16) -> PyResult<()> {
        let socket = TcpStream::connect((ip.clone(), port))?;

        if let Ok(peer) = Peer::new(py, format!("{}:{}", ip, port), socket, self.sender.clone()) {
            self.peers.push(peer);
        };
        Ok(())
    }

    fn listen(slf: Py<Self>, rx: Arc<Mutex<Receiver<ThreadMessage>>>) {
        for message in rx.lock().unwrap().iter() {
            Python::with_gil(|py| {
                let mut slf = slf.borrow_mut(py);
                match message.event.as_str() {
                    "connection_request" => {
                        let port = slf.port;
                        slf.tcp_connect(py, message.data.unwrap(), port).unwrap();
                    }
                    "connect" | "disconnect" => {
                        if let Some(event) = slf.events.get(&message.event) {
                            event
                                .borrow(py)
                                .call(py, PyTuple::new(py, &[&message.peer]), None)
                                .unwrap();
                        };
                    }
                    _ => {
                        if let Some(event) = slf.events.get(&message.event) {
                            event
                                .borrow(py)
                                .call(py, PyTuple::new(py, &[&message.data]), None)
                                .unwrap();
                        };
                    }
                }
            });
        }
    }
}

#[pymethods]
impl Network {
    #[new]
    fn new(address: String, port: u16) -> Self {
        let (sender, receiver) = channel();
        Self {
            address,
            port,
            peers: Vec::new(),
            events: HashMap::new(),
            receiver: Arc::new(Mutex::new(receiver)),
            sender,
        }
    }

    fn connect(&self, ip: &str, port: u16) -> PyResult<()> {
        let socket = UdpSocket::bind("0.0.0.0:7337")?;
        socket.send_to(self.port.to_string().as_bytes(), (ip, port))?;
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

    #[pyo3(signature = (debug = (false, false)))]
    fn serve(slf: PyRef<'_, Self>, py: Python, debug: (bool, bool)) {
        let ip = slf.address.clone();
        let port = slf.port;
        let rx = slf.receiver.clone();
        let network: Py<Self> = slf.into();

        {
            let slf = network.clone_ref(py);
            thread::spawn(move || Self::listen(slf, rx));
        };
        if !debug.0 {
            let slf = network.clone_ref(py);
            let ip = ip.clone();
            thread::spawn(move || Self::tcp_server(slf, ip, port));
        };
        if !debug.1 {
            let slf = network.clone_ref(py);
            let ip = ip.clone();
            thread::spawn(move || Self::udp_server(slf, ip, port));
        };
    }
}
