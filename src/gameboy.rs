use std::path::Path;

use minifb::{Key, Window, WindowOptions};

use crate::cpu::Cpu;

const WIDTH: usize = 800;
const HEIGHT: usize = 600;

pub struct Gameboy {
    pub cpu: Cpu,
}

impl Gameboy {
    pub fn new(rom_file: &Path) -> Self {
        Self {
            cpu: Cpu::new(rom_file),
        }
    }

    pub fn run(&mut self) {
        //let mut window = Window::new("Rustyboy", WIDTH, HEIGHT, WindowOptions::default())
        //    .unwrap_or_else(|e| panic!("{}", e));
        //let buffer = vec![125; WIDTH * HEIGHT];

        //window.limit_update_rate(Some(std::time::Duration::from_micros(16600)));

        //while window.is_open() && !window.is_key_down(Key::Escape) {
        //    window.update_with_buffer(&buffer, WIDTH, HEIGHT).unwrap();

        let mut instruction_count = 0;
        let max_instruction = 6_714_723;

        while instruction_count < max_instruction {
            self.cpu.run_cycle();
            instruction_count += 1;
        }

        //loop {
        //    self.cpu.run_cycle();
        //}
    }
}
