use std::net::IpAddr;

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
    pub name: [u8; 20],
    pub name_len: u8,
    pub ip_addr: [u8; 4],
    // compressed ECC public key is apparently 256 + 1
    // this should fit inside a 64 byte integer just fine
    pub pbl_key: u64,
}
impl Device {
    pub fn new(id: u32, addr: [u8; 4]) -> Device {
        Device {
            id: id,
            name: [0; 20],
            name_len: 0,
            ip_addr: addr,
            pbl_key: 0,
        }
    }
    pub fn to_pipe(&self) -> [u8; 25] {
        let mut buf = [0u8; 25];
        buf[..3].clone_from_slice(&self.id.to_ne_bytes());
        buf[4] = self.name_len;
        buf[5..].clone_from_slice(&self.name[..]);
        buf
    }
}

pub type DeviceList = Vec<Device>;

pub struct PipeHeader {
    pub d_type: u8,
    pub r_flag: u8,
    pub target_id: u32,
    pub p_len: u16,
}
impl PipeHeader {
    pub fn parse(buf: [u8; 8]) -> PipeHeader {
        PipeHeader {
            d_type: buf[0],
            r_flag: buf[1],
            target_id: u32::from_ne_bytes(buf[2..5].try_into().unwrap()),
            p_len: u16::from_ne_bytes(buf[6..7].try_into().unwrap()),
        }
    }
}
