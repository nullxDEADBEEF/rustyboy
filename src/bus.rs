// memory management unit

use std::path::Path;

use crate::{cartridge::Cartridge, serial::Serial, timer::Timer};

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
const HRAM_START: u16 = 0xFF80;
const HRAM_END: u16 = 0xFFFE;
const INTERRUPT_ENABLE: u16 = 0xFFFF;

const WRAM_SIZE: u16 = 0x1FFF;
const VRAM_SIZE: u16 = 0x1FFF;
const HRAM_SIZE: u16 = 0x7F;

// can be read from or written to by the CPU
pub struct Bus {
    pub timer: Timer,
    rom: Cartridge,
    serial: Serial,
    // internal ram
    working_ram: Vec<u8>,
    // stores graphic tiles
    video_ram: Vec<u8>,
    // OAM stores data that tells the gameboy
    // which tiles to use to construct moving objects on the screen
    oam: Vec<u8>,
    high_ram: Vec<u8>,
    ly: u8,
    ly_cycles: u16,
    stat: u8,    // 0xFF41
    lyc: u8,     // 0xFF45
    ie: u8,      // 0xFFFF - Interrupt Enable
    pub if_: u8, // 0xFF0F - Interrupt Flag
}

impl Bus {
    pub fn new(rom_file: &Path) -> Self {
        let mut bus = Self {
            timer: Timer::new(),
            serial: Serial::new(),
            rom: Cartridge::new(),
            working_ram: vec![0xFF; WRAM_SIZE as usize + 1],
            video_ram: vec![0xFF; VRAM_SIZE as usize + 1],
            oam: vec![0xFF; 160], // 160 bytes for OAM
            high_ram: vec![0xFF; HRAM_SIZE as usize + 1],
            ly: 0,
            ly_cycles: 0,
            stat: 0x85,
            lyc: 0x00,
            ie: 0x00,
            if_: 0x00,
        };

        bus.rom.load(rom_file).unwrap();
        // TODO: uncomment at some point
        //println!("{}", bus.rom);

        // hardware registers - boot ROM values
        //bus.write_byte(0xFF00, 0xCF); // Joypad
        //bus.write_byte(0xFF01, 0x00); // Serial data
        //bus.write_byte(0xFF02, 0x7E); // Serial control
        //bus.write_byte(0xFF04, 0xAB); // DIV
        //bus.write_byte(0xFF05, 0x00); // TIMA
        //bus.write_byte(0xFF06, 0x00); // TMA
        //bus.write_byte(0xFF07, 0xF8); // TAC
        //bus.write_byte(0xFF0F, 0xE1); // IF - set some interrupts
        //bus.write_byte(0xFF10, 0x80); // Sound
        //bus.write_byte(0xFF11, 0xBF);
        //bus.write_byte(0xFF12, 0xF3);
        //bus.write_byte(0xFF14, 0xBF);
        //bus.write_byte(0xFF16, 0x3F);
        //bus.write_byte(0xFF17, 0x00);
        //bus.write_byte(0xFF19, 0xBF);
        //bus.write_byte(0xFF1A, 0x7F);
        //bus.write_byte(0xFF1B, 0xFF);
        //bus.write_byte(0xFF1C, 0x9F);
        //bus.write_byte(0xFF1E, 0xFF);
        //bus.write_byte(0xFF20, 0xFF);
        //bus.write_byte(0xFF21, 0x00);
        //bus.write_byte(0xFF22, 0x00);
        //bus.write_byte(0xFF23, 0xBF);
        //bus.write_byte(0xFF24, 0x77);
        //bus.write_byte(0xFF25, 0xF3);
        //bus.write_byte(0xFF26, 0xF1);
        //bus.write_byte(0xFF40, 0x91); // LCD Control
        //bus.write_byte(0xFF41, 0x85); // LCD Status
        //bus.write_byte(0xFF42, 0x00); // Scroll Y
        //bus.write_byte(0xFF43, 0x00); // Scroll X
        //bus.write_byte(0xFF45, 0x00); // LYC
        //bus.write_byte(0xFF47, 0xFC); // BG Palette
        //bus.write_byte(0xFF48, 0xFF); // Object Palette 0
        //bus.write_byte(0xFF49, 0xFF); // Object Palette 1
        //bus.write_byte(0xFF4A, 0x00); // Window Y
        //bus.write_byte(0xFF4B, 0x00); // Window X
        //bus.write_byte(0xFFFF, 0x00); // IE - no interrupts enabled initially

        bus
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            // from cartridge, usually fixed bank
            ROM_START..=ROM_END => self.rom.read_byte(addr),
            VRAM_START..=VRAM_END => self.video_ram[addr as usize & VRAM_SIZE as usize],
            0xA000..=0xBFFF => self.rom.read_byte(addr),
            WRAM_START..=WRAM_END => self.working_ram[addr as usize & WRAM_SIZE as usize],
            0xE000..=0xFDFF => {
                // Echo RAM - mirrors 0xC000-0xDDFF
                let mirrored_addr = addr - 0x2000;
                self.working_ram[mirrored_addr as usize & WRAM_SIZE as usize]
            }
            // sprite attribute table
            SPRITE_OAM_START..=SPRITE_OAM_END => {
                let oam_addr = addr - SPRITE_OAM_START;
                self.oam[oam_addr as usize]
            }
            // prohibited area
            0xFEA0..=0xFEFF => 0,
            // I/O registers
            JOYPAD => 0xFF, // TODO: implement joypad input
            SERIAL_START..=SERIAL_END => self.serial.read_byte(addr),
            TIMER_START..=TIMER_END => self.timer.read_byte(addr),
            INTERRUPT_FLAG => self.if_ & 0x1F,
            SOUND_START..=SOUND_END => 0,
            // high ram (HRAM)
            HRAM_START..=HRAM_END => self.high_ram[addr as usize & HRAM_SIZE as usize],
            INTERRUPT_ENABLE => self.ie & 0x1F,
            0xFF41 => {
                let mut stat = self.stat & 0xFC; // lower 2 bits are mode
                let mode = if self.ly >= 144 {
                    1 // v-blank
                } else if self.ly_cycles < 80 {
                    2 // OAM
                } else if self.ly_cycles < 204 {
                    3 // Transfer
                } else {
                    0 // h-blank
                };
                stat |= mode;
                if self.ly == self.lyc {
                    stat |= 0x04; // set coincidence flag
                }

                stat
            }
            0xFF44 => {
                0x90
                //println!("LY READ: {}", self.ly);
                // TODO: uncomment
                //self.ly
            }
            0xFF45 => self.lyc,
            // LCD Control Register
            0xFF40 => 0x91, // Default LCD enabled with background enabled
            // LCD Scroll Y
            0xFF42 => 0x00,
            // LCD Scroll X
            0xFF43 => 0x00,
            // DMA Transfer and Start Address
            0xFF46 => 0x00,
            // BG Palette Data
            0xFF47 => 0xFC,
            // Object Palette 0 Data
            0xFF48 => 0xFF,
            // Object Palette 1 Data
            0xFF49 => 0xFF,
            // Window Y Position
            0xFF4A => 0x00,
            // Window X Position minus 7
            0xFF4B => 0x00,
            // Unhandled I/O registers - return reasonable defaults
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
            VRAM_START..=VRAM_END => self.video_ram[addr as usize & VRAM_SIZE as usize] = value,
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
                let oam_addr = addr - SPRITE_OAM_START;
                self.oam[oam_addr as usize] = value;
            }
            // prohibited area
            0xFEA0..=0xFEFF => {}
            // I/O registers
            JOYPAD => {}
            SERIAL_START..=SERIAL_END => self.serial.write_byte(addr, value),
            TIMER_START..=TIMER_END => self.timer.write_byte(addr, value),
            INTERRUPT_FLAG => self.if_ = value & 0x1F,
            SOUND_START..=SOUND_END => {}
            0xFF41 => self.stat = value & 0x7C, // only bits 2-6 are writable
            0xFF45 => self.lyc = value,
            // LCD Control Register
            0xFF40 => {} // TODO: implement LCD control
            // LCD Scroll Y
            0xFF42 => {}
            // LCD Scroll X
            0xFF43 => {}
            // DMA Transfer and Start Address
            0xFF46 => {
                let source_addr = (value as u16) << 8;
                for i in 0..160 {
                    let byte = self.read_byte(source_addr + 1);
                    self.oam[i] = byte;
                }
            }
            // BG Palette Data
            0xFF47 => {}
            // Object Palette 0 Data
            0xFF48 => {}
            // Object Palette 1 Data
            0xFF49 => {}
            // Window Y Position
            0xFF4A => {}
            // Window X Position minus 7
            0xFF4B => {}
            // high ram (HRAM)
            HRAM_START..=HRAM_END => self.high_ram[addr as usize & HRAM_SIZE as usize] = value,
            // interrupt enable register (IE)
            INTERRUPT_ENABLE => {
                self.ie = value & 0x1F;
            }
            _ => {}
        }
    }

    pub fn read_word(&self, addr: u16) -> u16 {
        (self.read_byte(addr) as u16) | ((self.read_byte(addr + 1) as u16) << 8)
    }

    pub fn write_word(&mut self, addr: u16, value: u16) {
        self.write_byte(addr, (value & 0xFF) as u8);
        self.write_byte(addr + 1, (value >> 8) as u8);
    }

    pub fn update_ly(&mut self, cycles: u8) {
        self.ly_cycles += cycles as u16;
        while self.ly_cycles >= 456 {
            self.ly_cycles -= 456;
            self.ly = self.ly.wrapping_add(1);
            if self.ly > 153 {
                self.ly = 0;
            }

            if self.ly == 144 {
                self.if_ |= 0x01;
            }

            // STAT coincidence flag and interrupt
            if self.ly == self.lyc {
                self.stat |= 0x04;
                if self.stat & 0x40 != 0 {
                    self.if_ |= 0x02;
                }
            } else {
                self.stat &= !0x04;
            }
        }
    }

    pub fn serial_mut(&mut self) -> &mut Serial {
        &mut self.serial
    }
}
