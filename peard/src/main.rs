mod pear;
mod tcp;
mod udp;

use ipipe::Pipe;
use std::env;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::thread::{self};
use std::time::Duration;

use pear::{DeviceList, PeardConfig, PeardFlags, PipeHeader};

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
        interface_port: 17001,
        discover_recv_timeout: 5,
        device_id: 1235,
    };

    println!("[peard] initializing daemon...");
    // initalize doscovery udp socket
    let udp_socket = udp::initalize_discover_socket(&config);
    // initialize callback tcp socket
    let tcp_listener = tcp::initialize_callback_listener(&config);

    println!(
        "[peard] daemon registered to interface {:?}",
        udp_socket.local_addr()
    );

    // initialize channel for disc listener
    let (disc_tx, disc_rx): (Sender<u8>, Receiver<u8>) = mpsc::channel();
    // initialize channel for dack listener
    let (dack_tx, dack_rx): (Sender<bool>, Receiver<bool>) = mpsc::channel();

    let udpsafe_debug_printing = (config.flags & (PeardFlags::DebugMode as u8)) == 1;
    let udp_thread_handle = thread::Builder::new()
        .name(String::from("disc_t"))
        .spawn(move || udp::udp_listen_t(disc_rx, udp_socket, &config, udpsafe_debug_printing))
        .expect("[peard] udp thread failed!");

    // initialize concurrency-safe queue for dack listener
    let devices = Arc::new(RwLock::new(DeviceList::new()));
    // create a safe (cloned arc) version of devices list to give to the listener thread
    let safe_devices = Arc::clone(&devices);

    let tcpsafe_debug_printing = (config.flags & (PeardFlags::DebugMode as u8)) == 1;
    // create DACK listener thread
    let tcp_thread_handle = thread::Builder::new()
        .name(String::from("dack_t"))
        .spawn(move || {
            tcp::tcp_listen_t(dack_rx, tcp_listener, safe_devices, tcpsafe_debug_printing)
        })
        .expect("[peard] tcp listener thread failed!");

    // daemon initialization complete
    println!("[peard] daemon initialization success!");

    if config.flags & PeardFlags::DebugMode as u8 == PeardFlags::DebugMode as u8 {
        println!(
            "[peard] debug mode enabled, broadcasts send to {} for debugging with peard-netmon!",
            config.interface_port + ((config.flags & (PeardFlags::DebugMode as u8)) as u16)
        );
    }

    // initialize named pipe
    let mut pipe = Pipe::with_name("peard-server").expect("[peard] failed to create pipe!");
    println!("[peard] pipe created at {}", pipe.path().display());

    let mut pipe_buffer = [0u8; 8];
    match pipe.read_exact(&mut pipe_buffer) {
        Ok(()) => {
            if config.flags & (PeardFlags::DebugMode as u8) == PeardFlags::DebugMode as u8 {
                println!("[pipe] recv");
            }
            let header = PipeHeader::parse(pipe_buffer);
            if header.r_flag == 0 {
                // if return flag is not set
                match header.d_type {
                    0 => {
                        // request current device list
                        // let devices_reader = devices.read().unwrap();
                        // for device in devices_reader.iter() {
                        //     println!("device: {:?}", device.id);
                        // }
                        // drop(devices_reader);
                    }
                    2 => {
                        // request refreshed device list
                        println!("[peard] discovering devices...");
                        udp::discover(&disc_tx);
                        // wait for two seconds and rebroadcast discovery message
                        thread::sleep(Duration::new(2, 0));
                        udp::discover(&disc_tx);
                        // wait for two more seconds to ensure all responses have arrived
                        thread::sleep(Duration::new(2, 0));
                        // println!("[peard] discovery complete!");
                        // let devices_reader = devices.read().unwrap();
                        // for device in devices_reader.iter() {
                        //     println!("device: {:?}", device.id);
                        // }
                        // drop(devices_reader);
                    }
                    10 => {
                        // send data payload to target device
                        let devices_reader = devices.read().unwrap();
                        if let Some(target_device) = devices_reader
                            .iter()
                            .find(|&device| device.id == header.target_id)
                        {
                            match TcpStream::connect(SocketAddr::new(
                                IpAddr::from(target_device.ip_addr),
                                config.interface_port,
                            )) {
                                Ok(mut stream) => {
                                    let mut data = [0u8; 5];
                                    let id = config.device_id;
                                    println!("[tcp] SEND to {:?}", stream.peer_addr());
                                    // set up message header
                                    data[0] = 10;
                                    data[1..4].clone_from_slice(&id.to_be_bytes());
                                    // write header to tcp stream
                                    stream
                                        .write(&data)
                                        .expect("[tcp] failed to write to SEND connection!");
                                    let cursor_pos = 0;
                                    while cursor_pos < header.p_len {}

                                    if config.flags & (PeardFlags::DebugMode as u8)
                                        == PeardFlags::DebugMode as u8
                                    {
                                        println!("[tcp] term {:?}", stream.peer_addr());
                                    }
                                    stream.shutdown(Shutdown::Both).unwrap();
                                }
                                Err(e) => {
                                    println!(
                                        "[peard] failed to connect to device {}: {}",
                                        header.target_id, e
                                    );
                                }
                            }
                        } else {
                            println!("[peard] WARN: device {} not in registry!", header.target_id);
                        }
                    }
                    _ => {}
                }
            }
        }
        Err(e) => {
            println!("[peard] pipe error: {}", e);
        }
    }

    // gracefully terminate the tcp listener thread
    let _ = dack_tx.send(true);
    //gracefully terminate the udp listener thread
    let _ = disc_tx.send(0);
    tcp_thread_handle.join().unwrap();
    udp_thread_handle.join().unwrap();
}
