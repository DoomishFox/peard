#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use std::collections::HashMap;
use std::env;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::mpsc::{self, channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, RwLock};
use std::thread::{self, Builder, JoinHandle};
use std::time::Duration;

enum PeardFlags {
    None = 0,
    DebugMode = 1,
}

struct PeardConfig {
    flags: u8,
    interface_addr: IpAddr,
    interface_port: u16,
    discover_recv_timeout: u64,
    device_id: u32,
}

struct Device {
    id: u32,
    ip_addr: [u8; 4],
    // compressed ECC public key is apparently 256 + 1
    // this should fit inside a 64 byte integer just fine
    pbl_key: u64,
}
impl Device {
    fn new(id: u32, addr: [u8; 4]) -> Device {
        Device {
            id: id,
            ip_addr: addr,
            pbl_key: 0,
        }
    }
}

type DeviceMap = HashMap<u32, Device>;
type DeviceList = Vec<Device>;

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut peard_flags: u8 = 0;
    if args.len() > 1 {
        for i in 1..args.len() {
            peard_flags += match args[i].as_str() {
                "-d" => PeardFlags::DebugMode as u8,
                _ => 0,
            };
        }
    }

    let config = PeardConfig {
        flags: peard_flags,
        interface_addr: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        interface_port: 17000,
        discover_recv_timeout: 5,
        device_id: 1234,
    };

    println!("[peard] initializing daemon...");
    // initalize doscovery broadcast socket
    let discover_socket = initalize_discover_socket(&config);
    // initialize callback receive socket
    let tcp_listener = initialize_tcp_listener(&config);

    // initialize concurrency-safe queue for tcplistener
    let (dack_tx, dack_rx): (Sender<bool>, Receiver<bool>) = mpsc::channel();
    let devices = Arc::new(RwLock::new(DeviceList::new()));
    // create a safe (cloned arc) version of devices list
    // to give to the listener thread
    let safe_devices = Arc::clone(&devices);
    let safe_debug_printing = (config.flags & (PeardFlags::DebugMode as u8)) == 1;
    let tcp_thread_handle = thread::Builder::new()
        .name(String::from("dack_t"))
        .spawn(move || tcp_listen(dack_rx, tcp_listener, safe_devices, safe_debug_printing))
        .expect("[peard] tcp listener thread failed!");

    println!(
        "[peard] daemon registered to interface {:?}",
        discover_socket.local_addr()
    );

    if config.flags & PeardFlags::DebugMode as u8 == PeardFlags::DebugMode as u8 {
        println!(
            "[peard] debug mode enabled, broadcasts send to {} for debugging with peard-netmon!",
            config.interface_port + ((config.flags & (PeardFlags::DebugMode as u8)) as u16)
        );
    }

    println!("[peard] discovering devices...");
    discover_ask(&discover_socket, &config);

    // wait for two seconds and rebroadcast discovery message
    thread::sleep(Duration::new(2, 0));
    discover_ask(&discover_socket, &config);

    // wait for two more seconds to ensure all responses have arrived
    thread::sleep(Duration::new(2, 0));

    println!("[peard] discovery complete!");
    let devices_reader = devices.read().unwrap();
    for device in devices_reader.iter() {
        println!("device: {:?}", device.id);
    }
    drop(devices_reader);

    // gracefully terminate the tcp listener thread
    let _ = dack_tx.send(true);
}

fn initalize_discover_socket(config: &PeardConfig) -> UdpSocket {
    let socket = UdpSocket::bind(SocketAddr::new(
        config.interface_addr,
        config.interface_port,
    ))
    .expect("[peard] failed to bind discover socket!");
    socket
        .set_broadcast(true)
        .expect("[peard] failed to enable multicast on discover socket!");
    socket
        .set_read_timeout(Some(Duration::new(config.discover_recv_timeout, 0)))
        .expect("[peard] failed to set discover socket receive timeout!");
    socket
}

fn initialize_tcp_listener(config: &PeardConfig) -> TcpListener {
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

fn discover_ask(socket: &UdpSocket, config: &PeardConfig) {
    println!("[upd] sending broadcast message");
    let data: [u8; 10] = [0; 10];
    socket
        .send_to(
            &data,
            SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)),
                config.interface_port + ((config.flags & (PeardFlags::DebugMode as u8)) as u16),
            ),
        )
        .expect("[peard] unable to send broadcast message!");
}

fn tcp_listen(
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
                            "[tcp] terminating DACK listener on thread {}",
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

fn udp_listen(rx: Receiver<bool>, socket: &UdpSocket, config: &PeardConfig, debug_printing: bool) {
    println!(
        "[peard] starting DISC listener on thread {}",
        thread::current().name().unwrap()
    );
    loop {
        let mut recv_buffer: [u8; 5] = [0; 5];
        if let Ok((n, mut addr)) = socket.recv_from(&mut recv_buffer) {
            if debug_printing {println!("[udp] recv {:?}", addr);}
            addr.set_port(1700);
            match TcpStream::connect(addr) {
                Ok(mut stream) => {
                    let mut data = [0u8; 10];
                    let id = config.device_id;
                    if debug_printing {println!("[peard] responding to DISC from {}", addr);}
                    data[0] = 1;
                    data[1] = id as u8;
                    data[2] = (id >> 8) as u8;
                    data[3] = (id >> 16) as u8;
                    data[4] = (id >> 24) as u8;
                    stream.write(&data).expect("[tcp] failed to write to DACK connection!");
                    stream.shutdown(Shutdown::Both).unwrap();
                }
                Err(e) => {
                    println!("[tcp] failed to connect on DACK: {}", e);
                }
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
}
