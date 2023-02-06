use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
include!(concat!(env!("OUT_DIR"), "/module.rs"));

use serde::{Deserialize, Serialize};
use serde_json;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
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
    ) -> PyResult<()> {
        if let Some(event) = self.events.get(name) {
            event.borrow(py).call(py, args, kwargs)?;
        }
        Ok(())
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
    events: Py<EventManager>,
    global_events: Py<EventManager>,
    read: Arc<Mutex<TcpStream>>,
    write: TcpStream,
}

#[pymethods]
impl Peer {
    fn connect(&self, py: Python, address: &str) -> PyResult<()> {
        self.global_events
            .borrow(py)
            .trigger(py, "connect", PyTuple::new(py, &[address]), None)?;
        Ok(())
    }

    fn listen(slf: PyRef<'_, Self>) -> PyResult<()> {
        let socket = slf.read.clone();
        let slf: Py<Self> = slf.into();

        thread::spawn(move || {
            let mut buffer = [0; 1024];
            let mut socket = socket.lock().unwrap();
            loop {
                let bytes_read = match socket.read(&mut buffer) {
                    Ok(bytes_read) => bytes_read,
                    Err(e) => {
                        println!("Error: {}", e);
                        return;
                    }
                };
                let message: Message = serde_json::from_slice(&buffer[..bytes_read]).unwrap();

                Python::with_gil(|py| {
                    let args = PyTuple::new(py, &[message.data]);
                    match slf.borrow(py).trigger(py, &message.event, args, None) {
                        Ok(_) => {}
                        Err(e) => {
                            println!("Error: {}", e);
                        }
                    }
                })
            }
        });

        Ok(())
    }

    fn trigger(
        &self,
        py: Python,
        name: &str,
        args: &PyTuple,
        kwargs: Option<&PyDict>,
    ) -> PyResult<()> {
        if let Some(event) = self.events.borrow(py).events.get(name) {
            event.borrow(py).call(py, args, kwargs)?;
        } else if let Some(event) = self.global_events.borrow(py).events.get(name) {
            event.borrow(py).call(py, args, kwargs)?;
        }
        Ok(())
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
        let socket = UdpSocket::bind("127.0.0.1:7337")?;
        socket.send_to(&[0], (ip, port))?;
        Ok(())
    }

    fn tcp_connect(&mut self, py: Python, ip: &str, port: u16) -> PyResult<()> {
        let read = TcpStream::connect((ip, port))?;
        let write = read.try_clone()?;
        let peer = Py::new(
            py,
            Peer {
                events: Py::new(py, EventManager::new())?,
                global_events: self.events.clone_ref(py),
                read: Arc::new(Mutex::new(read)),
                write,
            },
        )?;
        Peer::listen(peer.borrow(py))?;
        self.peers.push(peer.clone_ref(py));
        Ok(())
    }

    fn emit(&mut self, py: Python, event: String, data: String) -> PyResult<()> {
        let message = Message { event, data };
        let message = serde_json::to_string(&message).unwrap();
        self.peers.retain(|peer| {
            let mut socket = &peer.borrow(py).write;
            match socket.write(message.as_bytes()) {
                Ok(_) => true,
                Err(e) => {
                    peer.borrow(py)
                        .trigger(py, "disconnect", PyTuple::new(py, &["buh bye"]), None)
                        .unwrap();
                    println!("Error: {}", e);
                    false
                }
            }
        });
        Ok(())
    }

    fn tcp_server(slf: PyRefMut<'_, Self>) -> PyResult<()> {
        let listener = TcpListener::bind((slf.ip.clone(), slf.port))?;
        let slf: Py<Self> = slf.into();

        thread::spawn(move || {
            for stream in listener.incoming() {
                let read = match stream {
                    Ok(stream) => stream,
                    Err(e) => {
                        println!("Error: {}", e);
                        continue;
                    }
                };
                let write = match read.try_clone() {
                    Ok(stream) => stream,
                    Err(e) => {
                        println!("Error: {}", e);
                        continue;
                    }
                };
                Python::with_gil(|py| {
                    let peer = match Py::new(
                        py,
                        Peer {
                            events: Py::new(py, EventManager::new()).unwrap(),
                            global_events: slf.borrow(py).events.clone_ref(py),
                            read: Arc::new(Mutex::new(read)),
                            write,
                        },
                    ) {
                        Ok(peer) => peer,
                        Err(e) => {
                            println!("Error: {}", e);
                            return;
                        }
                    };
                    Peer::listen(peer.borrow(py)).unwrap();
                    slf.borrow_mut(py).peers.push(peer);
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
                    slf.tcp_connect(py, address.as_str(), port)
                        .unwrap();
                });
            }
        });

        Ok(())
    }
}
