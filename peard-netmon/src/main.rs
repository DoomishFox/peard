use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;

struct Sender {
    id: u8,
    sck_addr: SocketAddr,
}
impl Sender {
    fn new(addr: SocketAddr) -> Sender {
        Sender {
            id: 0,
            sck_addr: SocketAddr::new(addr.ip(), 17000),
        }
    }
}

fn main() {
    println!("initializing socket...");
    let socket =
        UdpSocket::bind("0.0.0.0:17001").expect("Couldnt bind socket to broadcast address!");
    socket
        .set_read_timeout(Some(Duration::new(5, 0)))
        .expect("Failed to set socket recv timeout!");

    println!("listening for broadcast on port 17001");

    loop {
        if let Some(sender) = discovery_recv(&socket) {
            println!("received broadcast: {:?}", sender.id);
            println!("sending tcp callback...");
            discovery_ack(sender);
        }
    }
}

fn discovery_recv(socket: &UdpSocket) -> Option<Sender> {
    let mut recv_buffer = [0u8; 5];
    if let Ok((n, addr)) = socket.recv_from(&mut recv_buffer) {
        println!("udp: recv {} bytes from {:?}", n, addr);
        return Some(Sender::new(addr));
    }
    return None;
}

fn discovery_ack(sender: Sender) {
    match TcpStream::connect(sender.sck_addr) {
        Ok(mut stream) => {
            println!("Connected to sender: {}", sender.id);
            let mut data = [0u8; 5];
            let id = dumb_rand();
            println!("simulating device {}", id);
            data[0] = 1;
            data[1] = id as u8;
            data[2] = (id >> 8) as u8;
            data[3] = (id >> 16) as u8;
            data[4] = (id >> 24) as u8;
            stream.write(&data).expect("Failed to write to stream!");
        }
        Err(e) => {
            println!("Failed to connect: {}", e);
        }
    }
}

fn dumb_rand() -> u32 {
    let num1 = vec![2, 3];
    let address1 = &num1 as *const Vec<i32>;
    let number1 = address1 as u32;
    number1
}
