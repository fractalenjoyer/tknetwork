use std::{net::{ TcpListener, TcpStream, UdpSocket }, io::{ Read, Write }, thread::JoinHandle, str};
use std::thread;

#[pyfunction]
fn add_one(x: i32) -> PyResult<i32> {
    Ok(x + 1)
}

#[pyfunction]
fn start_server(address: String, callback: Py<PyAny>) -> PyResult<()> {
    
    let handle: JoinHandle<Result<(), PyErr>> = thread::spawn( move || {
        println!("Starting server");
        let listener = TcpListener::bind(address)?;
        for connection in listener.incoming() {
            let mut stream = match connection {
                Ok(stream) => stream,
                Err(e) => {
                    println!("Error: {}", e);
                    continue;
                }
            };
            let mut buffer = String::new();
            stream.read_to_string(&mut buffer)?;

            Python::with_gil(|py| {
                let args = PyTuple::new(py, &[buffer]);
                callback.call1(py, args).expect("Error calling callback");
            })

        }
        Ok(())
    });

    Ok(())  
}

#[pyfunction]
fn send_message(message: String, address: String) -> PyResult<()> {
    let mut stream = TcpStream::connect(address)?;
    stream.write(message.as_bytes())?;
    Ok(())
}

#[pyfunction]
fn udp_server(address: String, callback: Py<PyAny>) -> PyResult<()> {
    thread::spawn(move || {
        println!("Starting server");
        let socket = UdpSocket::bind(address).expect("Error binding socket");
        loop {
            let mut buf = [0; 30];
            match socket.recv_from(&mut buf) {
                Ok(e) => {
                    Python::with_gil(|py| {
                        let args = PyTuple::new(py, &[String::from(str::from_utf8(&buf[..e.0]).unwrap())]);
                        callback.call1(py, args).expect("Error calling callback");
                    })
                },
                Err(e) => {
                    println!("Error: {}", e);
                    continue;
                }
            }
        }
    });
    Ok(())
}

#[pyfunction]
fn udp_send(message: String, address: String) -> PyResult<()> {
    let socket = UdpSocket::bind("127.0.0.1:24387").expect("Error binding socket");
    socket.send_to(message.as_bytes(), address)?;
    Ok(())
}