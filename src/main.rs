mod cpu;
mod register;
mod gameboy;
mod mmu;
mod util;

use std::{env, path::Path};

use gameboy::Gameboy;
use util::window_conf;

#[macroquad::main(window_conf)]
async fn main() {
    let args: Vec<String> = env::args().collect();

    let mut gameboy = Gameboy::new();
    gameboy.load_rom(Path::new(&args[1]));
    gameboy.run().await;
}
