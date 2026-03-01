mod apu;
mod bus;
mod cartridge;
mod cpu;
mod gameboy;
mod ppu;
mod register;
mod serial;
mod timer;

use std::env;
use std::path::Path;

use crate::gameboy::Gameboy;

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device,
};
use ringbuf::{traits::*, HeapRb};

fn main() {
    let args: Vec<String> = env::args().collect();

    let audio_device = get_audio_device();

    let audio_config = if let Ok(config) = audio_device.default_output_config() {
        let channels = config.channels();
        let sample_rate = config.sample_rate();
        (channels, sample_rate, config.config())
    } else {
        panic!("Could not get audio output config");
    };

    let (_channels, sample_rate, stream_config) = audio_config;
    let sample_rate_u32: u32 = sample_rate.into();

    // Lock-free SPSC ring buffer sized for ~100ms of stereo audio.
    // At 48kHz stereo: 48000 * 2 * 0.1 = 9600 samples.
    // Use a power-of-two-friendly size for good ring buffer performance.
    let ring_buf_size = (sample_rate_u32 as usize) * 2 / 5; // ~200ms worth, generous headroom
    let rb = HeapRb::<f32>::new(ring_buf_size);
    let (producer, mut consumer) = rb.split();

    let _stream = audio_device
        .build_output_stream(
            &stream_config,
            move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let filled = consumer.pop_slice(output);
                // Fill any remaining output with silence to avoid noise on underrun
                for sample in &mut output[filled..] {
                    *sample = 0.0;
                }
            },
            |err| eprintln!("Audio stream error: {}", err),
            None,
        )
        .expect("Failed to build audio stream");

    _stream.play().expect("Failed to play audio stream");

    match args.len() {
        2 => {
            let mut gameboy = Gameboy::new(Path::new(&args[1]), sample_rate_u32, producer);
            gameboy.run();
        }
        _ => eprintln!("Usage: cargo run <ROM>"),
    }
}

fn get_audio_device() -> Device {
    let default_audio_host = cpal::default_host();

    if let Some(audio_output_device) = default_audio_host.default_output_device() {
        audio_output_device
    } else {
        panic!("No output device found")
    }
}
