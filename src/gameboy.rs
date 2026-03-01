use std::path::Path;

use minifb::{Key, Window, WindowOptions};
use ringbuf::{traits::*, HeapProd};

use crate::cpu::Cpu;

const WIDTH: usize = 160;
const HEIGHT: usize = 144;

pub struct Gameboy {
    pub cpu: Cpu,
    audio_producer: HeapProd<f32>,
}

impl Gameboy {
    pub fn new(rom_file: &Path, sample_rate: u32, audio_producer: HeapProd<f32>) -> Self {
        Self {
            cpu: Cpu::new(rom_file, sample_rate),
            audio_producer,
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

        // Disable minifb's built-in rate limiter — we sync to audio instead
        window.limit_update_rate(None);

        let buffer_capacity = self.audio_producer.capacity().get();
        // Wait when buffer is more than half full
        let high_water = buffer_capacity / 2;

        while window.is_open() && !window.is_key_down(Key::Escape) {
            self.check_joypad_state(&window);
            let cycles_per_frame = 17556; // ~4.19 MHz / 60 FPS
            let mut cycles_run = 0u32;
            while cycles_run < cycles_per_frame {
                cycles_run += self.cpu.run_cycle() as u32;
            }

            let samples = self.cpu.bus.apu.end_frame();
            self.audio_producer.push_slice(&samples);

            // Audio-driven sync: wait for the audio callback to drain the buffer
            // before producing more. This locks the emulator to the audio device's
            // real-time clock, preventing both overrun (crackling) and underrun (gaps).
            while self.audio_producer.occupied_len() > high_water {
                std::thread::yield_now();
            }

            window
                .update_with_buffer(&self.cpu.bus.ppu.frame_buffer, WIDTH, HEIGHT)
                .unwrap();
        }
    }

    fn check_joypad_state(&mut self, window: &Window) {
        let mut action = 0xF;
        let mut direction = 0xF;

        if window.is_key_down(Key::Z) {
            action &= !(1 << 0)
        }
        if window.is_key_down(Key::X) {
            action &= !(1 << 1)
        }
        if window.is_key_down(Key::Space) {
            action &= !(1 << 2)
        }
        if window.is_key_down(Key::Enter) {
            action &= !(1 << 3)
        }

        if window.is_key_down(Key::Right) {
            direction &= !(1 << 0)
        }
        if window.is_key_down(Key::Left) {
            direction &= !(1 << 1)
        }
        if window.is_key_down(Key::Up) {
            direction &= !(1 << 2)
        }
        if window.is_key_down(Key::Down) {
            direction &= !(1 << 3)
        }


        self.cpu.bus.joypad.action_state = action;
        self.cpu.bus.joypad.direction_state = direction;
    }
}
