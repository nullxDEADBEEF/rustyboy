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

        self.reg.set_bc(self.mmu.read_word(self.reg.pc.into()));
    }

    // load data from register A to memory location specified by register pair BC
    fn load_bc_a(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.mmu.working_ram[self.reg.get_bc() as usize] = self.reg.a;
    }

    // increment register pair BC
    fn inc_bc(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg.set_bc(self.reg.get_bc() + 1);
    }

    // increment register B
    fn inc_b(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.b += 1;
        self.reg.f &= !u8::from(Flags::Operation);
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

        self.reg.b = self.reg.b.wrapping_sub(1);
        // set operation flag since we subtracted
        self.reg.f |= u8::from(Flags::Operation);

        if self.reg.b == 0 {
            self.reg.f |= u8::from(Flags::Zero);
        }

        // NOTE: borrow means if there was a carry/halfcarry from the preceeding operation
        // so the last 4-bits overflowed
        if self.reg.b & 0xF == 0xF {
            self.reg.f |= u8::from(Flags::HalfCarry);
        }
    }

    // load value into register B
    fn load_b(&mut self) {
        self.reg.pc += 2;
        self.m = 2;
        self.t = 8;

        self.reg.b = self.mmu.read_byte(self.reg.pc.into());
    }

    // rotate register A left
    fn rlca(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.f &= !u8::from(Flags::Zero);
        self.reg.f &= !u8::from(Flags::HalfCarry);
        self.reg.f &= !u8::from(Flags::Operation);

        self.reg.a = (self.reg.a << 1)
            | (if self.reg.a & u8::from(Flags::Zero) == 0x80 {
                1
            } else {
                0
            });
    }

    // load stack pointer at given address
    fn load_sp_at_addr(&mut self, addr: u16) {
        self.reg.pc += 3;
        self.m = 5;
        self.t = 20;

        // store lower byte of sp at addr
        self.mmu.working_ram[addr as usize] = self.reg.sp as u8;

        // store upper byte of sp at addr + 1
        self.mmu.working_ram[addr as usize + 1] = (self.reg.sp >> 8 & 0xFF) as u8;
    }

    // add register BC to HL
    fn add_hl_bc(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg
            .set_hl(self.reg.get_hl().wrapping_add(self.reg.get_bc()));
        self.reg.f &= !u8::from(Flags::Operation);

        if self.reg.get_hl() > 0x7FA6 {
            self.reg.f |= u8::from(Flags::Carry);
            println!("CARRY");
        } else if self.reg.get_hl() > 0x800 {
            self.reg.f |= u8::from(Flags::HalfCarry);
            println!("HALF CARRY");
        }
    }

    // load contents specified by register BC into register A
    fn ld_a_bc(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg.a = self.mmu.working_ram[self.reg.get_bc() as usize];
    }

    // decrement register pair BC by 1
    fn dec_bc(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg.set_bc(self.reg.get_bc() - 1);
    }

    // increment contents of register C by 1
    fn inc_c(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.c += 1;
        self.reg.f &= !u8::from(Flags::Operation);
        if self.reg.c == 0 {
            self.reg.f |= u8::from(Flags::Zero);
        } else if self.reg.c > 0x8 {
            self.reg.f |= u8::from(Flags::HalfCarry);
        }
    }

    // decrement contents of register C by 1
    fn dec_c(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.c = self.reg.c.wrapping_sub(1);
        self.reg.f |= u8::from(Flags::Operation);
        if self.reg.c == 0 {
            self.reg.f |= u8::from(Flags::Zero);
        } else if self.reg.c & 0xF == 0 {
            self.reg.f |= u8::from(Flags::HalfCarry);
        }
    }

    // load immediate operand into register C
    fn ld_c(&mut self) {
        self.reg.pc += 2;
        self.m = 2;
        self.t = 8;

        self.reg.c = self.mmu.working_ram[self.reg.pc as usize];
    }

    // Rotate contents of register A to the right
    fn rrca(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        // TODO: refactor this into a function, everywhere
        self.reg.f &= !u8::from(Flags::Zero);
        self.reg.f &= !u8::from(Flags::Operation);
        self.reg.f &= !u8::from(Flags::HalfCarry);

        self.reg.a = (self.reg.a >> 1) | (if self.reg.a & 0x01 == 0x01 { 0x80 } else { 0 })
    }

    pub fn decode_execute(&mut self) {
        self.current_opcode = self.mmu.read_byte(self.reg.pc.into());
        match self.current_opcode {
            0x00 => self.nop(),
            0x01 => self.load_bc(),
            0x02 => self.load_bc_a(),
            0x03 => self.inc_bc(),
            0x04 => self.inc_b(),
            0x05 => self.dec_b(),
            0x06 => self.load_b(),
            0x07 => self.rlca(),
            0x08 => self.load_sp_at_addr(self.current_opcode as u16),
            0x09 => self.add_hl_bc(),
            0x0A => self.ld_a_bc(),
            0x0B => self.dec_bc(),
            0x0C => self.inc_c(),
            0x0D => self.dec_c(),
            0x0E => self.ld_c(),
            0x0F => self.rrca(),
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
        let expected_pc = cpu.reg.pc + 1;

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
        let expected_m_cycles = 3;
        let expected_t_cycles = 12;
        let expected_pc = cpu.reg.pc + 3;
        // 244 << 8 | 128
        let expected_bc: u16 = 62592;
        cpu.mmu.working_ram[expected_pc as usize] = 244;
        cpu.mmu.working_ram[expected_pc as usize + 1] = 128;

        // Act
        cpu.load_bc();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_bc, cpu.reg.get_bc());
    }

    #[test]
    fn test_load_bc_a() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 2;
        let expected_t_cycles = 8;
        let expected_pc = cpu.reg.pc + 1;
        let register_a = 235;

        // Act
        cpu.reg.a = register_a;
        cpu.load_bc_a();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(register_a, cpu.mmu.working_ram[cpu.reg.get_bc() as usize]);
    }

    #[test]
    fn test_inc_bc() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 2;
        let expected_t_cycles = 8;
        let expected_pc = cpu.reg.pc + 1;
        let expected_register_bc = cpu.reg.get_bc() + 1;

        // Act
        cpu.inc_bc();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_register_bc, cpu.reg.get_bc());
    }

    #[test]
    // TODO: still need to test cases:
    // when b = 0
    // when b > 0x8
    fn test_inc_b() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 1;
        let expected_t_cycles = 4;
        let expected_pc = cpu.reg.pc + 1;
        let expected_register_b_value = 1;
        let expected_register_f_value = 0x0;

        // Act
        cpu.inc_b();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_register_b_value, cpu.reg.b);
        assert_eq!(expected_register_f_value, cpu.reg.f);
    }

    #[test]
    #[should_panic]
    fn test_dec_b() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 1;
        let expected_t_cycles = 4;
        let expected_pc = cpu.reg.pc + 1;
        let expected_register_b_value = 4;
        let expected_register_f_value = u8::from(Flags::Operation);

        // Act
        cpu.reg.b = 5;
        cpu.dec_b();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_register_b_value, cpu.reg.b);
        assert_eq!(expected_register_f_value, cpu.reg.f);

        cpu.reg.b = 17;
        cpu.dec_b();
        let expected_register_f_value = u8::from(Flags::HalfCarry) + u8::from(Flags::Operation);
        assert_eq!(expected_register_f_value, cpu.reg.f);

        // This should make the test panic
        cpu.reg.b = 0;
        cpu.dec_b();
    }

    #[test]
    fn test_load_b() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 2;
        let expected_t_cycles = 8;
        let expected_pc = cpu.reg.pc + 2;
        let expected_b_load_value = 235;
        // since the program counter starts at 0x0100
        // then we add two to the program counter we arrive
        // at position 0x0102
        cpu.mmu.working_ram[expected_pc as usize] = expected_b_load_value;

        // Act
        cpu.load_b();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_b_load_value, cpu.reg.b);
    }

    #[test]
    fn test_rlca() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 1;
        let expected_t_cycles = 4;
        let expected_pc = cpu.reg.pc + 1;
        let expected_register_f_value = 0;
        let expected_register_a_value: u8 = 4;
        let expected_register_a_value_with_carry = 1;

        // Act
        cpu.reg.a = 2;
        cpu.rlca();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_register_f_value, cpu.reg.f);
        assert_eq!(expected_register_a_value, cpu.reg.a);

        cpu.reg.a = 128;
        cpu.rlca();

        assert_eq!(expected_register_a_value_with_carry, cpu.reg.a);
    }

    #[test]
    fn test_load_sp() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 5;
        let expected_t_cycles = 20;
        let expected_pc = cpu.reg.pc + 3;
        // based on setting the sp at 32678
        let expected_sp_lower_byte = 166;
        let expected_sp_upper_byte = 127;

        // Act
        cpu.reg.sp = 32678;
        cpu.load_sp_at_addr(expected_pc);

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(
            expected_sp_lower_byte,
            cpu.mmu.working_ram[expected_pc as usize]
        );
        assert_eq!(
            expected_sp_upper_byte,
            cpu.mmu.working_ram[expected_pc as usize + 1]
        );
    }

    #[test]
    fn test_add_hl_bc() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 2;
        let expected_t_cycles = 8;
        let expected_pc = cpu.reg.pc + 1;
        let expected_hl = 16555;
        let expected_bc = 32678;
        let expected_f_register = cpu.reg.f & !u8::from(Flags::Operation);
        let expected_f_register_after_half_carry = u8::from(Flags::HalfCarry);
        let expected_f_register_after_carry = u8::from(Flags::Carry);

        // Act
        cpu.reg.set_hl(expected_hl);
        cpu.reg.set_bc(expected_bc);
        cpu.add_hl_bc();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_hl + expected_bc, cpu.reg.get_hl());
        assert_eq!(expected_f_register_after_carry, cpu.reg.f);

        cpu = Cpu::new();
        cpu.reg.set_hl(1000);
        cpu.reg.set_bc(2048);
        cpu.add_hl_bc();
        assert_eq!(expected_f_register_after_half_carry, cpu.reg.f);

        cpu = Cpu::new();
        cpu.reg.set_hl(155);
        cpu.reg.set_bc(155);
        cpu.add_hl_bc();
        assert_eq!(expected_f_register, cpu.reg.f);
    }

    #[test]
    fn test_ld_a_bc() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 2;
        let expected_t_cycles = 8;
        let expected_pc = cpu.reg.pc + 1;
        let expected_a = 255;
        cpu.mmu.working_ram[cpu.reg.get_bc() as usize] = expected_a;

        // Act
        cpu.ld_a_bc();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_a, cpu.mmu.working_ram[cpu.reg.get_bc() as usize]);
    }

    #[test]
    fn test_dec_bc() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 2;
        let expected_t_cycles = 8;
        let expected_pc = cpu.reg.pc + 1;
        let expected_bc = 9;
        cpu.reg.set_bc(10);

        // Act
        cpu.dec_bc();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_bc, cpu.reg.get_bc());
    }

    #[test]
    fn test_inc_c() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 1;
        let expected_t_cycles = 4;
        let expected_pc = cpu.reg.pc + 1;
        let expected_c = 241;
        cpu.reg.c = 240;

        // Act
        cpu.inc_c();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_c, cpu.reg.c);
        assert_eq!(
            cpu.reg.f & !u8::from(Flags::Operation) | u8::from(Flags::HalfCarry),
            cpu.reg.f
        );
    }

    #[test]
    fn test_dec_c() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 1;
        let expected_t_cycles = 4;
        let expected_pc = cpu.reg.pc + 1;
        let expected_c = 24;
        cpu.reg.c = 25;

        // Act
        cpu.dec_c();

        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_c, cpu.reg.c);

        cpu = Cpu::new();
        cpu.reg.c = 1;
        cpu.dec_c();
        println!("{}", cpu.reg.c);

        assert_eq!(
            u8::from(Flags::Operation) | u8::from(Flags::Zero),
            cpu.reg.f
        );

        cpu = Cpu::new();
        cpu.reg.c = 17;
        cpu.dec_c();

        assert_eq!(
            u8::from(Flags::Operation) | u8::from(Flags::HalfCarry),
            cpu.reg.f
        );
    }

    #[test]
    fn test_ld_c() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 2;
        let expected_t_cycles = 8;
        let expected_pc = cpu.reg.pc + 2;
        let expected_c = 25;
        cpu.mmu.working_ram[expected_pc as usize] = 25;

        // Act
        cpu.ld_c();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_c, cpu.reg.c);
    }

    #[test]
    fn test_rrca() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 1;
        let expected_t_cycles = 4;
        let expected_pc = cpu.reg.pc + 1;
        let expected_register_f = u8::from(Flags::None);
        // 240 >> 1
        let expected_a = 120;
        cpu.reg.a = 240;

        // Act
        cpu.rrca();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_register_f, cpu.reg.f);
        assert_eq!(expected_a, cpu.reg.a);

        cpu = Cpu::new();
        let expected_a = 248;
        cpu.reg.a = 241;
        cpu.rrca();

        assert_eq!(expected_a, cpu.reg.a);
    }
}
