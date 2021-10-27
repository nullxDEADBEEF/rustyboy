use crate::{mmu::Mmu, register::Flags, register::Register};

// memory interface can address up to 65536 bytes (16-bit bus)
// programs are accessed through the same address bus as normal memory
// instruction size can be between one and three bytes

// timings assume a CPU frequency of 4.19 MHz, called "T-states"
// because timings are divisble by 4 many specify timings and clock frequency divided by 4, called "M-cycles"

#[allow(dead_code)]
pub struct Cpu {
    reg: Register,
    pub mmu: Mmu,
    current_opcode: u8,
    // accumulated clock
    clock_m: u8,
    clock_t: u8,
    // clock for last instruction
    m: u8,
    t: u8,
    halted: bool,
}

impl Cpu {
    pub fn new() -> Self {
        Self {
            reg: Register::new(),
            mmu: Mmu::new(),
            current_opcode: 0,
            clock_m: 0,
            clock_t: 0,
            m: 0,
            t: 0,
            halted: false,
        }
    }

    // no operation, only advances the program counter by 1
    fn nop(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;
    }

    // load 2 bytes of data into register pair BC
    fn load_bc(&mut self) {
        self.reg.pc += 3;
        self.m = 3;
        self.t = 12;

        self.reg.get_bc();
    }

    // load data from register A to the register pair BC
    fn load_bc_a(&mut self) {
        self.m = 2;
        self.t = 8;

        // TODO: might be wrong...
        self.reg.b = self.reg.a;
        self.reg.c = self.reg.a;

        self.reg.pc += 1;
    }

    // increment register pair BC
    fn inc_bc(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        // TODO: double check correctness
        self.reg.b += 1;
        self.reg.c += 1;
    }

    // increment register B
    fn inc_b(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.b += 1;
        self.reg.f |= !(u8::from(Flags::Operation));
        // set half carry flag if we overflowed the lower 4-bits
        if self.reg.b & 0xF > 0x8 {
            self.reg.f |= u8::from(Flags::HalfCarry);
        }
        if self.reg.b == 0 {
            self.reg.f |= u8::from(Flags::Zero);
        }
    }

    // decrement register B
    fn dec_b(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.b -= 1;
        // set operation flag since we subtracted
        self.reg.f |= u8::from(Flags::Operation);

        if self.reg.b == 0 {
            self.reg.f |= u8::from(Flags::Zero);
        }

        // NOTE: borrow means if there was a carry/halfcarry from the preceeding operation
        // so the last 4-bytes overflowed
        if self.reg.b > 0xF {
            self.reg.f |= u8::from(Flags::HalfCarry);
        }
    }

    // load value into register B
    fn load_b(&mut self) {
        self.reg.pc += 2;
        self.m = 2;
        self.t = 8;

        self.reg.b = self.mmu.read_byte(self.reg.pc as u8);
    }

    // rotate register A left with carry
    fn rlca(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.f &= !(1 << u8::from(Flags::Zero));
        self.reg.f &= !(1 << u8::from(Flags::HalfCarry));
        self.reg.f &= !(1 << u8::from(Flags::Operation));

        self.reg.a <<= 1;
        self.reg.a = self.reg.a | self.reg.a >> 7;
    }

    // load something with stack pointer
    fn load_sp(&mut self) {
        self.reg.pc += 3;
        self.m = 5;
        self.t = 20;
    }

    pub fn decode_execute(&mut self) {
        self.current_opcode = self.mmu.read_byte(self.reg.pc as u8);
        match self.current_opcode {
            0x00 => self.nop(),
            0x01 => self.load_bc(),
            0x02 => self.load_bc_a(),
            0x03 => self.inc_bc(),
            0x04 => self.inc_b(),
            0x05 => self.dec_b(),
            0x06 => self.load_b(),
            0x07 => self.rlca(),
            0x08 => self.load_sp(),
            _ => println!("{:#X} is not a recognized opcode...", self.current_opcode),
        }
        self.dec_b();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nop() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 1;
        let expected_t_cycles = 4;
        let expected_pc = 0x0101;

        // Act
        cpu.nop();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
    }

    #[test]
    fn test_load_bc() {
        // Arrange
        let mut cpu = Cpu::new();
        cpu.reg.b = 1;
        cpu.reg.c = 2;
        let expected_m_cycles = 3;
        let expected_t_cycles = 12;
        let expected_pc = 0x0103;
        let expected_bc = 258;

        // Act
        cpu.load_bc();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_bc, cpu.reg.get_bc());
    }
}
