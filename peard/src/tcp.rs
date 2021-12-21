use std::io::{self, Read};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, RwLock};
use std::thread::{self};
use std::time::Duration;

use crate::pear::{Device, DeviceList, PeardConfig};

pub fn initialize_callback_listener(config: &PeardConfig) -> TcpListener {
    let listener = TcpListener::bind(SocketAddr::new(
        config.interface_addr,
        config.interface_port,
    ))
    .expect("[peard] failed to create tcp listener!");
    listener
        .set_nonblocking(true)
        .expect("[peard] failed to set tcp listener to non-blocking!");
    listener
}

pub fn tcp_listen_t(
    rx: Receiver<bool>,
    listener: TcpListener,
    map: Arc<RwLock<DeviceList>>,
    debug_printing: bool,
) {
    println!(
        "[peard] starting DACK listener on thread {}",
        thread::current().name().unwrap()
    );
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let _ = stream.set_nonblocking(false);
                if debug_printing {
                    println!("[tcp] recv {:?}", stream.peer_addr());
                }
                if let Ok(peer) = stream.peer_addr() {
                    if let Some(addr) = match peer.ip() {
                        IpAddr::V4(ip) => Some(ip.octets()),
                        _ => None,
                    } {
                        let mut buffer = [0u8; 10];
                        stream.read_exact(&mut buffer).unwrap();
                        if debug_printing {
                            println!("[tcp] raw: {:?}", buffer);
                        }

                        let msg_type = buffer[0];
                        let id: u32 = 0
                            + (buffer[1] as u32)
                            + ((buffer[2] as u32) << 8)
                            + ((buffer[3] as u32) << 16)
                            + ((buffer[4] as u32) << 24);
                        if debug_printing {
                            println!("[tcp] [payload] device_id: {}", id);
                        }

                        match msg_type {
                            1 => {
                                // register new device if it doesnt exist
                                let mut devices_writer = map.write().unwrap();
                                if let None = devices_writer.iter().find(|&device| device.id == id)
                                {
                                    devices_writer.push(Device::new(id, addr));
                                    println!("[peard] registered new device {}", id);
                                    drop(devices_writer);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if debug_printing {
                    println!("[tcp] term {:?}", stream.peer_addr());
                }
                stream.shutdown(Shutdown::Both).unwrap();
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Decide if we should exit
                match rx.try_recv() {
                    Ok(_) | Err(TryRecvError::Disconnected) => {
                        println!(
                            "[peard] terminating DACK listener on thread {}",
                            thread::current().name().unwrap()
                        );
                        break;
                    }
                    Err(TryRecvError::Empty) => {}
                }
                // Decide if we should try to accept a connection again
                thread::sleep(Duration::from_micros(100));
                continue;
            }
            Err(e) => {
                println!("[tcp] [FAIL]: {}", e);
            }
        }
    }
}
