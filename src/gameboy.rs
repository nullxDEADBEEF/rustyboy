use std::{convert::TryInto, fs};
use std::path::Path;

use macroquad::prelude::{clear_background, is_key_pressed, next_frame, KeyCode, GREEN};

use crate::cpu::Cpu;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub struct Gameboy {
    pub cpu: Cpu,
}

impl Gameboy {
    pub fn new() -> Self {
        Self { cpu: Cpu::new() }
    }

    pub fn load_rom(&mut self, path: &Path) {
        self.cpu.mmu.working_ram = fs::read(path).unwrap().try_into().unwrap();
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
