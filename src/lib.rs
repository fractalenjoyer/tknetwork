use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
include!(concat!(env!("OUT_DIR"), "/module.rs"));

use std::collections::HashMap;
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
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

    fn bind(&mut self, py: Python, name: &str) -> PyResult<Py<Event>> {
        let event = Py::new(py, Event { callback: None })?;
        self.events.insert(name.to_string(), event.clone_ref(py));
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

#[pyclass]
struct Peer {
    #[pyo3(get)]
    events: Py<EventManager>,
    global_events: Py<EventManager>,
    socket: TcpStream,
}

#[pymethods]
impl Peer {
    fn connect(&self, py: Python, address: &str) -> PyResult<()> {
        self.global_events
            .borrow(py)
            .trigger(py, "connect", PyTuple::new(py, &[address]), None)?;
        Ok(())
    }
}

#[pyclass]
struct Network {
    #[pyo3(get)]
    events: Py<EventManager>,
    peers: Arc<Mutex<Vec<Py<Peer>>>>,
    address: String,
}

#[pymethods]
impl Network {
    #[new]
    fn new(py: Python, address: &str) -> PyResult<Self> {
        Ok(Self {
            address: address.to_string(),
            events: Py::new(py, EventManager::new())?,
            peers: Arc::new(Mutex::new(Vec::new())),
        })
    }

    fn connect(&mut self, address: &str) -> PyResult<()> {
        let socket = UdpSocket::bind("127.0.0.1:7337")?;
        socket.send_to(&[0], address)?;
        Ok(())
    }

    #[pyo3(signature = (name, *args, **kwargs))]
    fn emit(
        &self,
        py: Python,
        name: &str,
        args: &PyTuple,
        kwargs: Option<&PyDict>,
    ) -> PyResult<()> {
        self.events.borrow(py).trigger(py, name, args, kwargs)?;
        Ok(())
    }

    fn tcp_server(&self, py: Python) -> PyResult<()> {
        let listener = TcpListener::bind(&self.address)?;
        let peers = self.peers.clone();
        let events = self.events.clone_ref(py);

        thread::spawn(move || {
            for stream in listener.incoming() {
                let connection = match stream {
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
                            global_events: events.clone_ref(py),
                            socket: connection,
                        },
                    ) {
                        Ok(peer) => peer,
                        Err(e) => {
                            println!("Error: {}", e);
                            return;
                        }
                    };
                    match peers.lock() {
                        Ok(mut peers) => peers.push(peer),
                        Err(e) => {
                            println!("Error: {}", e);
                            return;
                        }
                    };
                });
            }
        });

        Ok(())
    }

    #[getter]
    fn peers(&self) -> PyResult<Vec<Py<Peer>>> {
        Ok(self.peers.lock().unwrap().clone())
    }
}
