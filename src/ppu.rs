pub const VRAM_SIZE: u16 = 0x1FFF;

pub struct PPU {
    // stores graphic tiles
    pub video_ram: Vec<u8>,
    // OAM stores data that tells the gameboy
    // which tiles to use to construct moving objects on the screen
    pub oam: Vec<u8>,
    pub ly: u8,
    pub ly_cycles: u16,
    pub stat: u8,
    pub lyc: u8, 
    pub lcdc: u8,
}

impl PPU {
    pub fn new() -> Self {
        Self {
            video_ram: vec![0xFF; VRAM_SIZE as usize + 1],
            oam: vec![0xFF; 160], // 160 bytes for OAM
            ly: 0,
            ly_cycles: 0,
            lyc: 0x00,
            stat: 0x85,
            lcdc: 0x91,
        }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0xFF40 => self.lcdc,
            0xFF41 => {
                let mut stat = self.stat & 0xFC; // lower 2 bits are mode
                let mode = if self.ly >= 144 {
                    1 // v-blank
                } else if self.ly_cycles < 80 {
                    2 // OAM search
                } else if self.ly_cycles < 252 {
                    3 // pixel transfer
                } else {
                    0 // HBlank
                };
                stat |= mode;
                if self.ly == self.lyc {
                    stat |= 0x04; // set coincidence flag
                }

                stat
            },
            0xFF44 => self.ly,
            0xFF45 => self.lyc,
            _ => panic!("PPU read error at address: {addr}"),
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF40 => self.lcdc = value,
            0xFF41 => self.stat = value & 0x7C, // only bits 2-6 are writable
            0xFF45 => self.lyc = value,
            _ => panic!("PPU write error at address: {:#04X}", addr),
        }
    }

    pub fn update_ly(&mut self, cycles: u8) -> u8 {
        let mut bitmask: u8 = 0;

        self.ly_cycles += cycles as u16;
        while self.ly_cycles >= 456 {
            self.ly_cycles -= 456;
            self.ly = self.ly.wrapping_add(1);
            if self.ly > 153 {
                self.ly = 0;
            }

            if self.ly == 144 {
                bitmask |= 0x01;
            }

            // STAT coincidence flag and interrupt
            if self.ly == self.lyc {
                self.stat |= 0x04;
                if self.stat & 0x40 != 0 {
                    bitmask |= 0x02;
                }
            } else {
                self.stat &= !0x04;
            }
        }

        bitmask
    }
}