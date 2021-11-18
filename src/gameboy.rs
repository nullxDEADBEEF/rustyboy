use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use macroquad::prelude::{clear_background, is_key_pressed, next_frame, KeyCode, GREEN};

use crate::cpu::Cpu;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub struct Gameboy {
    pub cpu: Cpu,
}

#[allow(clippy::unused_io_amount)]
impl Gameboy {
    pub fn new() -> Self {
        Self { cpu: Cpu::new() }
    }

    pub fn load_rom(&mut self, path: &Path) -> Result<()> {
        let file = File::open(path)?;
        let mut buf_reader = BufReader::new(file);
        // load file data into the working ram
        buf_reader.read(&mut self.cpu.mmu.working_ram)?;
        Ok(())
    }

    pub async fn run(&mut self) {
        loop {
            clear_background(GREEN);

            if is_key_pressed(KeyCode::Escape) {
                break;
            }

            self.cpu.decode_execute();

            next_frame().await
        }
    }
}
