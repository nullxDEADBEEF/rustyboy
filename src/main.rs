mod bus;
mod cartridge;
mod cpu;
mod gameboy;
mod register;
mod serial;
mod timer;

use std::{env, path::Path};

use crate::gameboy::Gameboy;

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.len() {
        2 => {
            let mut gameboy = Gameboy::new(Path::new(&args[1]));
            //for _ in 0..1000 {
            //    gameboy.cpu.run_cycle();
            //}
            gameboy.run();
        }
        _ => eprintln!("Usage: cargo run <ROM>"),
    }
}
