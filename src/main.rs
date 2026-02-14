mod bus;
mod cartridge;
mod cpu;
mod gameboy;
mod register;
mod serial;
mod timer;
mod ppu;

use std::{env, path::Path};

use crate::gameboy::Gameboy;

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.len() {
        2 => {
            let mut gameboy = Gameboy::new(Path::new(&args[1]));
            gameboy.run();
        }
        _ => eprintln!("Usage: cargo run <ROM>"),
    }
}
