use std::path::Path;

use minifb::{Key, Window, WindowOptions};

use crate::cpu::Cpu;

const WIDTH: usize = 160;
const HEIGHT: usize = 144;

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
        let window_options= WindowOptions {
            scale: minifb::Scale::X2,
            resize: true,
            ..WindowOptions::default()
        };
        let mut window = Window::new("Rustyboy", WIDTH, HEIGHT, window_options)
            .unwrap_or_else(|e| panic!("{}", e));

        while window.is_open() && !window.is_key_down(Key::Escape) {
            let cycles_per_frame = 17556; // ~4.19 MHz / 60 FPS
            let mut cycles_run = 0;
            while cycles_run < cycles_per_frame {
                cycles_run += self.cpu.run_cycle() as u32;
            }
            window.update_with_buffer(&self.cpu.bus.frame_buffer, WIDTH, HEIGHT).unwrap();
        }
    }
}
