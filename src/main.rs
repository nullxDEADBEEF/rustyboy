mod apu;
mod bus;
mod cartridge;
mod cpu;
mod gameboy;
mod ppu;
mod register;
mod serial;
mod timer;

use std::{env, path::Path};

use crate::gameboy::Gameboy;

use cpal::{Device, traits::{DeviceTrait, HostTrait, StreamTrait}};

fn main() {
    let args: Vec<String> = env::args().collect();

    let (sender, receiver) = std::sync::mpsc::sync_channel::<Vec<i16>>(4);

    let audio_device = get_audio_device();

    let audio_config = if let Ok(config) = audio_device.default_output_config() {
        config.config()
    } else {
        panic!("Could not get audio output config");
    };

    let _stream = audio_device.build_output_stream(
        &audio_config,
        move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let samples = receiver.try_recv().unwrap_or_default();
            let mut sample_iter = samples.iter();

            for out in output.iter_mut() {
                if let Some(&sample) = sample_iter.next() {
                    *out = sample as f32 / 32768.0;
                } else {
                    *out = 0.0;
                }
            }
        },
        |err| eprintln!("Audio stream error: {}", err),
        None, 
    ).expect("Failed to build audio stream");
    
    _stream.play().expect("Failed to play audio stream");

    match args.len() {
        2 => {
            let mut gameboy = Gameboy::new(Path::new(&args[1]), audio_config.sample_rate, sender);
            gameboy.run();
        }
        _ => eprintln!("Usage: cargo run <ROM>"),
    }
}

fn get_audio_device() -> Device {
    let default_audio_host = cpal::default_host();

    if let Some(audio_output_device) = default_audio_host.default_output_device()
    {
        audio_output_device
    } else {
        panic!("No output device found")
    }
}
