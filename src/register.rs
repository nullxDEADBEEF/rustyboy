#[derive(Clone)]
pub enum Flags {
    Zero,      // set if last operation produced 0, used by conditional jumps
    Operation, // set if last operation was subtraction
    HalfCarry, // set if lower half of the byte overflowed in last operation
    Carry,     // set if last operation produced result over 255 or under 0
    None,
}

impl From<Flags> for u8 {
    fn from(flags: Flags) -> u8 {
        match flags {
            Flags::Zero => 0x80,
            Flags::Operation => 0x40,
            Flags::HalfCarry => 0x20,
            Flags::Carry => 0x10,
            Flags::None => 0x00,
        }
    }
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
            a: 0,
            f: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
            sp: 0,
            // when the gameboy powers up, pc is set to 0x0100
            // and instruction found at that location in the ROM is run.
            pc: 0x0100,
        }
    }

    pub fn get_bc(&self) -> u16 {
        (self.b as u16) << 8 | self.c as u16
    }

    pub fn set_bc(&mut self, value: u16) {
        self.b = ((value & 0xFF00) >> 8) as u8;
        self.c = (value & 0xFF) as u8;
    }

    pub fn get_hl(&self) -> u16 {
        (self.h as u16) << 8 | self.l as u16
    }

    pub fn set_hl(&mut self, value: u16) {
        self.h = ((value & 0xFF00) >> 8) as u8;
        self.l = (value & 0xFF) as u8;
    }

    pub fn get_de(&self) -> u16 {
        (self.d as u16) << 8 | self.e as u16
    }

    pub fn set_de(&mut self, value: u16) {
        self.d = ((value & 0xFF00) >> 8) as u8;
        self.e = (value & 0xFF) as u8;
    }
}
