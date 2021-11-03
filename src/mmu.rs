// memory management unit

// NOTE: "word" in this context means 16-bit

const WORKING_RAM_BYTES: usize = 0x8000;
const VIDEO_RAM_BYTES: usize = 0x8000;
const ZERO_PAGE_RAM_BYTES: usize = 0x80;

pub struct Mmu {
    // can be read from or written to by the CPU
    pub working_ram: [u8; WORKING_RAM_BYTES],
    pub video_ram: [u8; VIDEO_RAM_BYTES],
    // most of the interaction between the program and the gameboy hardware happens
    // through this zero page ram.
    pub zero_page_ram: [u8; ZERO_PAGE_RAM_BYTES],
}

impl Mmu {
    pub fn new() -> Self {
        Self {
            working_ram: [0; WORKING_RAM_BYTES],
            video_ram: [0; VIDEO_RAM_BYTES],
            zero_page_ram: [0; ZERO_PAGE_RAM_BYTES],
        }
    }

    pub fn read_byte(&self, addr: usize) -> u8 {
        self.working_ram[addr]
    }

    pub fn read_word(&self, addr: usize) -> u16 {
        (self.working_ram[addr] as u16) << 8 | self.working_ram[addr + 1] as u16
    }
}
