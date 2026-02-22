use std::{path::Path, sync::mpsc::SyncSender};

use minifb::{Key, Window, WindowOptions};

use crate::cpu::Cpu;

const WIDTH: usize = 160;
const HEIGHT: usize = 144;

pub struct Gameboy {
    pub cpu: Cpu,
    sample_sender: SyncSender<Vec<i16>>,
}

impl Gameboy {
    pub fn new(rom_file: &Path, sample_rate: u32, sample_sender: SyncSender<Vec<i16>>) -> Self {
        Self {
            cpu: Cpu::new(rom_file, sample_rate),
            sample_sender,
        }
    }

    pub fn run(&mut self) {
        let window_options = WindowOptions {
            scale: minifb::Scale::X8,
            resize: true,
            ..WindowOptions::default()
        };
        let mut window = Window::new("Rustyboy", WIDTH, HEIGHT, window_options)
            .unwrap_or_else(|e| panic!("{}", e));

        let mut frame_count = 0u32;
        let mut last_result = 0xFFu8;
        while window.is_open() && !window.is_key_down(Key::Escape) {
            let cycles_per_frame = 17556; // ~4.19 MHz / 60 FPS
            let mut cycles_run = 0;
            while cycles_run < cycles_per_frame {
                cycles_run += self.cpu.run_cycle() as u32;
            }
            let samples = self.cpu.bus.apu.end_frame();
            let _ = self.sample_sender.try_send(samples);

            frame_count += 1;
            // Debug: check Blargg test result at 0xA000
            let result = self.cpu.bus.read_byte(0xA000);
            if result != last_result {
                last_result = result;
                let mut text = String::new();
                for i in 0..200 {
                    let ch = self.cpu.bus.read_byte(0xA004 + i);
                    if ch == 0 { break; }
                    text.push(ch as char);
                }
                eprintln!("Frame {}: Blargg code={:#04X} text={}", frame_count, result, text);
            }

            window
                .update_with_buffer(&self.cpu.bus.ppu.frame_buffer, WIDTH, HEIGHT)
                .unwrap();
        }
    }
}
