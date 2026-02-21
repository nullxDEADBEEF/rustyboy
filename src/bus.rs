// memory management unit

use std::path::Path;

use crate::{apu::Apu, cartridge::Cartridge, ppu::Ppu, serial::Serial, timer::Timer};

// NOTE: "word" in this context means 16-bit

const ROM_START: u16 = 0x0000;
const ROM_END: u16 = 0x7FFF;
const VRAM_START: u16 = 0x8000;
const VRAM_END: u16 = 0x9FFF;
const WRAM_START: u16 = 0xC000;
const WRAM_END: u16 = 0xDFFF;
const SPRITE_OAM_START: u16 = 0xFE00;
const SPRITE_OAM_END: u16 = 0xFE9F;
const JOYPAD: u16 = 0xFF00;
const SERIAL_START: u16 = 0xFF01;
const SERIAL_END: u16 = 0xFF02;
const TIMER_START: u16 = 0xFF04;
const TIMER_END: u16 = 0xFF07;
const INTERRUPT_FLAG: u16 = 0xFF0F;
const SOUND_START: u16 = 0xFF10;
const SOUND_END: u16 = 0xFF26;
const WAVE_RAM_START: u16 = 0xFF30;
const WAVE_RAM_END: u16 = 0xFF3F;
const HRAM_START: u16 = 0xFF80;
const HRAM_END: u16 = 0xFFFE;
const INTERRUPT_ENABLE: u16 = 0xFFFF;

const WRAM_SIZE: u16 = 0x1FFF;
const HRAM_SIZE: u16 = 0x7F;

// can be read from or written to by the CPU
pub struct Bus {
    pub timer: Timer,
    pub ppu: Ppu,
    pub apu: Apu,
    rom: Cartridge,
    serial: Serial,
    // internal ram
    working_ram: Vec<u8>,
    high_ram: Vec<u8>,
    ie: u8,
    pub if_: u8, // Interrupt Flag reference for STAT and VBlank interrupts
}

impl Bus {
    pub fn new(rom_file: &Path, sample_rate: u32) -> Self {
        let mut bus = Self {
            timer: Timer::new(),
            serial: Serial::new(),
            rom: Cartridge::new(),
            ppu: Ppu::new(),
            apu: Apu::new(sample_rate),
            working_ram: vec![0xFF; WRAM_SIZE as usize + 1],
            high_ram: vec![0xFF; HRAM_SIZE as usize + 1],
            ie: 0x00,
            if_: 0x00,
        };

        bus.rom.load(rom_file).unwrap();
        println!("{}", bus.rom);

        bus
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            // from cartridge, usually fixed bank
            ROM_START..=ROM_END => self.rom.read_byte(addr),
            VRAM_START..=VRAM_END => self.ppu.read_byte(addr),
            0xA000..=0xBFFF => self.rom.read_byte(addr),
            WRAM_START..=WRAM_END => self.working_ram[addr as usize & WRAM_SIZE as usize],
            0xE000..=0xFDFF => {
                // Echo RAM - mirrors 0xC000-0xDDFF
                let mirrored_addr = addr - 0x2000;
                self.working_ram[mirrored_addr as usize & WRAM_SIZE as usize]
            }
            // sprite attribute table
            SPRITE_OAM_START..=SPRITE_OAM_END => self.ppu.read_byte(addr),
            // prohibited area
            0xFEA0..=0xFEFF => 0,
            // I/O registers
            JOYPAD => 0xFF, // TODO: implement joypad input
            SERIAL_START..=SERIAL_END => self.serial.read_byte(addr),
            TIMER_START..=TIMER_END => self.timer.read_byte(addr),
            INTERRUPT_FLAG => self.if_ & 0x1F,
            SOUND_START..=SOUND_END => self.apu.read_byte(addr),
            WAVE_RAM_START..=WAVE_RAM_END => self.apu.read_byte(addr),
            // high ram (HRAM)
            HRAM_START..=HRAM_END => self.high_ram[addr as usize & HRAM_SIZE as usize],
            INTERRUPT_ENABLE => self.ie & 0x1F,
            0xFF40..=0xFF4B => self.ppu.read_byte(addr),
            _ => {
                // For now, return 0 for unhandled registers
                // This prevents the debug spam and allows the test to continue
                0
            }
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        match addr {
            // from cartridge, usually fixed bank
            ROM_START..=ROM_END => self.rom.write_byte(addr, value),
            VRAM_START..=VRAM_END => self.ppu.write_byte(addr, value),
            0xA000..=0xBFFF => self.rom.write_byte(addr, value),
            WRAM_START..=WRAM_END => {
                self.working_ram[addr as usize & WRAM_SIZE as usize] = value;
                // Also update echo RAM
                let echo_addr = addr + 0x2000;
                if echo_addr <= 0xFDFF {
                    self.working_ram[echo_addr as usize & WRAM_SIZE as usize] = value;
                }
            }
            0xE000..=0xFDFF => {
                let mirrored_addr = addr - 0x2000;
                self.working_ram[mirrored_addr as usize & WRAM_SIZE as usize] = value;
                // Also update main RAM
                self.working_ram[addr as usize & WRAM_SIZE as usize] = value;
            }
            // sprite attribute table
            SPRITE_OAM_START..=SPRITE_OAM_END => {
                self.ppu.write_byte(addr, value);
            }
            // prohibited area
            0xFEA0..=0xFEFF => {}
            // I/O registers
            JOYPAD => {}
            SERIAL_START..=SERIAL_END => self.serial.write_byte(addr, value),
            TIMER_START..=TIMER_END => self.timer.write_byte(addr, value),
            INTERRUPT_FLAG => self.if_ = value & 0x1F,
            SOUND_START..=SOUND_END => self.apu.write_byte(addr, value),
            WAVE_RAM_START..=WAVE_RAM_END => self.apu.write_byte(addr, value),
            0xFF40..=0xFF43 | 0xFF45 => self.ppu.write_byte(addr, value),
            // DMA Transfer and Start Address
            0xFF46 => {
                let source_addr = (value as u16) << 8;
                for i in 0..160 {
                    let byte = self.read_byte(source_addr + i);
                    self.ppu.oam[i as usize] = byte;
                }
            }
            0xFF47..=0xFF4B => self.ppu.write_byte(addr, value),
            // high ram (HRAM)
            HRAM_START..=HRAM_END => self.high_ram[addr as usize & HRAM_SIZE as usize] = value,
            // interrupt enable register (IE)
            INTERRUPT_ENABLE => {
                self.ie = value & 0x1F;
            }
            _ => {}
        }
    }

    pub fn read_word(&mut self, addr: u16) -> u16 {
        (self.read_byte(addr) as u16) | ((self.read_byte(addr + 1) as u16) << 8)
    }

    pub fn write_word(&mut self, addr: u16, value: u16) {
        self.write_byte(addr, (value & 0xFF) as u8);
        self.write_byte(addr + 1, (value >> 8) as u8);
    }

    pub fn serial_mut(&mut self) -> &mut Serial {
        &mut self.serial
    }
}
