const MAX_FLAG_VALUE: u8 = 0xF0;

#[derive(Clone, Copy)]
pub enum Flags {
    Zero = 1 << 7,      // set if last operation produced 0, used by conditional jumps
    Negative = 1 << 6,  // set if last operation was subtraction
    HalfCarry = 1 << 5, // set if lower half of the byte overflowed in last operation
    Carry = 1 << 4,     // set if last operation produced result over 255 or under 0
}

pub struct Register {
    // 8-bit registers
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    // 16-bit registers
    pub sp: u16,
    pub pc: u16,
}

impl Register {
    pub fn new() -> Self {
        Self {
            a: 0x01,
            f: 0xB0,
            b: 0x00,
            c: 0x13,
            d: 0x00,
            e: 0xD8,
            h: 0x01,
            l: 0x4D,
            sp: 0xFFFE,
            // when the gameboy powers up, pc is set to 0x0100
            // and instruction found at that location in the ROM is run.
            pc: 0x0100,
        }
    }

    pub fn get_bc(&self) -> u16 {
        (self.b as u16) << 8 | self.c as u16
    }

    pub fn get_hl(&self) -> u16 {
        (self.h as u16) << 8 | self.l as u16
    }

    pub fn get_de(&self) -> u16 {
        (self.d as u16) << 8 | self.e as u16
    }

    pub fn get_af(&self) -> u16 {
        (self.a as u16) << 8 | (self.f & MAX_FLAG_VALUE) as u16
    }

    pub fn set_bc(&mut self, value: u16) {
        self.b = (value >> 8) as u8;
        self.c = (value & 0xFF) as u8;
    }

    pub fn set_hl(&mut self, value: u16) {
        self.h = (value >> 8) as u8;
        self.l = (value & 0xFF) as u8;
    }
    pub fn set_de(&mut self, value: u16) {
        self.d = (value >> 8) as u8;
        self.e = (value & 0xFF) as u8;
    }
    pub fn set_af(&mut self, value: u16) {
        self.a = (value >> 8) as u8;
        self.f = (value & MAX_FLAG_VALUE as u16) as u8;
    }
}

impl Default for Register {
    fn default() -> Self {
        Self::new()
    }
}
