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

    pub fn read_byte(&self, addr: u8) -> u8 {
        self.working_ram[addr as usize]
    }

    // read 16-bit from addr
    pub fn read_word(&self, addr: u16) -> u16 {
        unimplemented!()
    }

    pub fn write_byte(&self, addr: u8, value: u8) {
        unimplemented!()
    }

    // write 16-bit value to addr
    pub fn write_word(&self, addr: u16, value: u16) {
        unimplemented!()
    }
}
