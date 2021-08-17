pub enum Flags {
    Zero = 0x80,      // set if last operation produced 0, used by conditional jumps
    Operation = 0x40, // set if last operation was subtraction
    HalfCarry = 0x20, // set if lower half of the byte overflowed in last operation
    Carry = 0x10,     // set if last operation produced result over 255 or under 0
    None = 0x00,
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
}