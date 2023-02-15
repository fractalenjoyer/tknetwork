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
    name: String,
    stream: RefCell<TcpStream>,
    sender: Sender<ThreadMessage>,
    events: HashMap<String, Py<Event>>,
}

impl Peer {
    fn new(py: Python, name: String, stream: TcpStream, sender: Sender<ThreadMessage>) -> PyResult<Py<Self>> {
        let peer = Py::new(py, Self {
            name,
            stream: RefCell::new(stream.try_clone()?),
            sender: sender.clone(),
            events: HashMap::new(),
        })?;

        let peer_clone = peer.clone_ref(py);
        thread::spawn(move || Self::listen(peer_clone, stream));

        sender.send(ThreadMessage {
            event: "connect".to_string(),
            peer: Some(peer.clone_ref(py)),
            data: None,
        }).unwrap();

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

        peer.sender.send(ThreadMessage {
            event: message.event.clone(),
            peer: None,
            data: Some(message.data.clone()),
        }).unwrap();

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
    receiver: Receiver<ThreadMessage>,
    sender: Sender<ThreadMessage>,
    peers: Vec<Py<Peer>>,
}

#[pymethods]
impl Network {
    #[new]
    fn new(address: String, port: u16) -> Self {
        let (sender, receiver) = channel();
        Self {
            address,
            port,
            receiver,
            sender,
            peers: Vec::new(),
        }
    }

    fn connect(&self, ip: &str, port: u16) -> PyResult<()> {
        let socket = UdpSocket::bind("0.0.0.0:7337")?;
        socket.send_to(self.port.to_string().as_bytes(), (ip, port))?;
        Ok(())
    }

    fn emit(&mut self, py: Python, event: String, data: String) -> PyResult<()> {
        self.peers
            .retain(|peer| peer.borrow(py).emit(event.clone(), data.clone()).is_ok());
        Ok(())
    }

    #[pyo3(signature = (debug = (false, false)))]
    fn serve(slf: PyRef<'_, Self>, py: Python, debug: (bool, bool)) {
        let slf: Py<Self> = slf.into();

        if !debug.0 {
            thread::spawn(|| Self::tcp_server(slf.clone_ref(py)));
        } 
        if !debug.1 {
            thread::spawn(|| Self::udp_server(slf.clone_ref(py)));
        }
    }
}

impl Network {
}