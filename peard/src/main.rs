#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use std::collections::HashMap;
use std::env;
use std::io::{self, Read};
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
                "debug" => PeardFlags::DebugMode as u8,
                _ => 0,
            };
        }
    }

    let config = PeardConfig {
        flags: peard_flags,
        interface_addr: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        interface_port: 17000,
        discover_recv_timeout: 5,
    };

    println!("[peard] initializing daemon...");
    // initalize doscovery broadcast socket
    let discover_socket = initalize_discover_socket(&config);
    // initialize callback receive socket
    let callback_socket = initalize_calback_socket(&config);

    // initialize concurrency-safe queue for tcplistener
    let (tx, rx): (Sender<bool>, Receiver<bool>) = mpsc::channel();
    let devices = Arc::new(RwLock::new(DeviceList::new()));
    // create a safe (cloned arc) version of devices list
    // to give to the listener thread
    let safe_devices = Arc::clone(&devices);
    let tcp_thread_handle = thread::Builder::new()
        .name(String::from("pear_listener"))
        .spawn(move || listen_on_thread(rx, callback_socket, safe_devices))
        .expect("[peard] tcp listener thread failed!");

    println!("[peard] daemon initalized successfully!");
    println!(
        "[peard] registered to interface {:?}",
        discover_socket.local_addr().unwrap()
    );
    if config.flags & PeardFlags::DebugMode as u8 == PeardFlags::DebugMode as u8 {
        println!(
            "[DEBUG] debug mode enabled, broadcasts send to {} for debugging with peard-netmon!",
            config.interface_port + ((config.flags & (PeardFlags::DebugMode as u8)) as u16)
        );
    }

    println!("Searching for devices...");
    discover_ask(&discover_socket, &config);



    thread::sleep(Duration::new(2, 0));
    println!("wait complete, reading device list:");
    let devices_reader = devices.read().unwrap();
    for device in devices_reader.iter() {
        println!("device: {:?}", device.id);
    }
    drop(devices_reader);

    // gracefully terminate the tcp listener thread
    let _ = tx.send(true);
}

fn initalize_discover_socket(config: &PeardConfig) -> UdpSocket {
    let discover_socket = UdpSocket::bind(SocketAddr::new(
        config.interface_addr,
        config.interface_port,
    ))
    .expect("[peard] Failed to bind discover socket!");
    discover_socket
        .set_broadcast(true)
        .expect("[peard] Failed to enable multicast on discover socket!");
    discover_socket
        .set_read_timeout(Some(Duration::new(config.discover_recv_timeout, 0)))
        .expect("[peard] Failed to set discover socket receive timeout!");
    discover_socket
}

fn initalize_calback_socket(config: &PeardConfig) -> TcpListener {
    let callback_socket = TcpListener::bind(SocketAddr::new(
        config.interface_addr,
        config.interface_port,
    ))
    .expect("[peard] Failed to create TCP listener!");
    callback_socket
        .set_nonblocking(true)
        .expect("[peard] Failed to set TCP listener to non-blocking!");
    callback_socket
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

fn listen_on_thread(rx: Receiver<bool>, listener: TcpListener, map: Arc<RwLock<DeviceList>>) {
    println!(
        "[peard] starting tcp listener on thread {}",
        thread::current().name().unwrap()
    );
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let _ = stream.set_nonblocking(false);
                println!("[tcp] recv {:?}", stream.peer_addr());
                if let Ok(peer) = stream.peer_addr() {
                    if let Some(addr) = match peer.ip() {
                        IpAddr::V4(ip) => Some(ip.octets()),
                        _ => None,
                    } {
                        let mut buffer = [0u8; 10];
                        stream.read_exact(&mut buffer).unwrap();
                        println!("[tcp] [payload] raw: {:?}", buffer);

                        let id: u32 = 0
                            + (buffer[0] as u32)
                            + ((buffer[1] as u32) << 8)
                            + ((buffer[2] as u32) << 16)
                            + ((buffer[3] as u32) << 24);
                        println!("[tcp] [payload] device_id: {}", id);
                        // register new device if it doesnt exist
                        let devices_reader = map.read().unwrap();
                        if let None = devices_reader.iter().find(|&device| device.id == id) {
                            drop(devices_reader);
                            let mut devices_writer = map.write().unwrap();
                            devices_writer.push(Device::new(id, addr));
                            println!("[tcp] [payload] registered device {}", id);
                            drop(devices_writer);
                        }
                    }
                }
                println!("[tcp] term {:?}", stream.peer_addr());
                stream.shutdown(Shutdown::Both).unwrap();
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Decide if we should exit
                match rx.try_recv() {
                    Ok(_) | Err(TryRecvError::Disconnected) => {
                        println!(
                            "[tcp] terminating tcp listener on thread {}",
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
    //listener.close
}