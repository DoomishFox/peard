mod pear;
mod tcp;
mod udp;

use ipipe::Pipe;
use std::env;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr};
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
        interface_port: 17000,
        discover_recv_timeout: 5,
        device_id: 1234,
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
            let header = PipeHeader::new(pipe_buffer);
            match header.d_type {
                0 => {
                    // request current device list
                    let devices_reader = devices.read().unwrap();
                    for device in devices_reader.iter() {
                        println!("device: {:?}", device.id);
                    }
                }
                _ => {}
            }
        }
        Err(e) => {
            println!("[peard] pipe error: {}", e);
        }
    }

    // for line in std::io::BufReader::new(pipe).lines() {
    //     read_pipe(&line.unwrap().as_str());
    // }

    println!("[peard] discovering devices...");
    //discover_ask(&udp_socket, &config);
    udp::discover(&disc_tx);

    // wait for two seconds and rebroadcast discovery message
    thread::sleep(Duration::new(2, 0));
    //discover_ask(&udp_socket, &config);
    udp::discover(&disc_tx);

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
    //gracefully terminate the udp listener thread
    let _ = disc_tx.send(0);
    tcp_thread_handle.join().unwrap();
    udp_thread_handle.join().unwrap();
}

fn read_pipe(line: &str) {
    println!("incoming from pipe: {}", line);
    match line {
        "" => {}
        _ => {}
    }
}
