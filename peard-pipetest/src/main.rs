use ipipe::Pipe;
use std::io::{Read, Write};

fn main() {
    println!("testing pipe!");
    let mut pipe = Pipe::with_name("peard-server").unwrap();
    
    let mut pipe_buffer = [0u8; 8];
    pipe_buffer[0] = 2;
    pipe.write(&pipe_buffer).expect("failed to write to pipe!");
    //writeln!(&mut pipe, "This is only a test.").unwrap();
}
