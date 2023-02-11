use std::io::{Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpStream, UdpSocket};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::Duration;
use std::thread;

use crate::pear::{PeardConfig, PeardFlags};

pub fn discover(tx: &Sender<u8>) {
    let _ = tx.send(1);
}

pub fn initalize_discover_socket(config: &PeardConfig) -> UdpSocket {
    let socket = UdpSocket::bind(SocketAddr::new(
        config.interface_addr,
        config.interface_port,
    ))
    .expect("[peard] failed to bind discover socket!");
    socket
        .set_broadcast(true)
        .expect("[peard] failed to enable multicast on discover socket!");
    socket
        .set_read_timeout(Some(Duration::from_micros(config.discover_recv_timeout)))
        .expect("[peard] failed to set discover socket receive timeout!");
    socket
}

pub fn udp_listen_t(rx: Receiver<u8>, socket: UdpSocket, config: &PeardConfig, debug_printing: bool) {
    println!(
        "[peard] starting DISC worker on thread {}",
        thread::current().name().unwrap()
    );
    loop {
        match rx.try_recv() {
            Ok(1) => {
                if debug_printing {
                    println!("[upd] sending broadcast message");
                }
                let data: [u8; 10] = [0xF0, 0x9F, 0x8D, 0x90, 0x00, 1,2,3,5, 0];
                socket
                    .send_to(
                        &data,
                        SocketAddr::new(
                            IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)),
                            config.interface_port
                                + ((config.flags & (PeardFlags::DebugMode as u8)) as u16),
                        ),
                    )
                    .expect("[peard] unable to send broadcast message!");
            }
            Ok(0) | Err(TryRecvError::Disconnected) => {
                println!(
                    "[peard] terminating DISC worker on thread {}",
                    thread::current().name().unwrap()
                );
                break;
            }
            Ok(_) | Err(TryRecvError::Empty) => {
                // check for incomping broadcast
                let mut recv_buffer: [u8; 5] = [0; 5];
                if let Ok((_n, mut addr)) = socket.recv_from(&mut recv_buffer) {
                    if debug_printing {
                        println!("[udp] recv {:?}", addr);
                    }
                    addr.set_port(config.interface_port);
                    match TcpStream::connect(addr) {
                        Ok(mut stream) => {
                            let mut data = [0u8; 5];
                            let id = config.device_id;
                            println!("[peard] responding to DISC from {}", addr);
                            data[0] = 1;
                            data[1] = id as u8;
                            data[2] = (id >> 8) as u8;
                            data[3] = (id >> 16) as u8;
                            data[4] = (id >> 24) as u8;
                            stream
                                .write(&data)
                                .expect("[tcp] failed to write to DACK connection!");
                            stream.shutdown(Shutdown::Both).unwrap();
                        }
                        Err(e) => {
                            println!("[tcp] failed to connect on DACK: {}", e);
                        }
                    }
                }
            }
        }
    }
}
