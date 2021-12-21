use std::net::{IpAddr};

pub enum PeardFlags {
    DebugMode = 1,
}

#[derive(Copy, Clone)]
pub struct PeardConfig {
    pub flags: u8,
    pub interface_addr: IpAddr,
    pub interface_port: u16,
    pub discover_recv_timeout: u64,
    pub device_id: u32,
}

pub struct Device {
    pub id: u32,
    pub ip_addr: [u8; 4],
    // compressed ECC public key is apparently 256 + 1
    // this should fit inside a 64 byte integer just fine
    pub pbl_key: u64,
}
impl Device {
    pub fn new(id: u32, addr: [u8; 4]) -> Device {
        Device {
            id: id,
            ip_addr: addr,
            pbl_key: 0,
        }
    }
}

pub type DeviceList = Vec<Device>;