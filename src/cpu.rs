use std::path::Path;

use crate::{bus::Bus, register::Flags, register::Register};

// memory interface can address up to 65536 bytes (16-bit bus)
// programs are accessed through the same address bus as normal memory
// instruction size can be between one and three bytes

// timings assume a CPU frequency of 4.19 MHz, called "T-states"
// because timings are divisble by 4 many specify timings and clock frequency divided by 4, called "M-cycles"

// TODO: add timing for more accurate emulation

pub struct Cpu {
    pub reg: Register,
    pub bus: Bus,
    // clock for last instruction
    m: u8,
    halted: bool,
    ime: bool,
    enable_ime_next: bool,
    stopped: bool,
}

impl Cpu {
    pub fn new(rom_file: &Path) -> Self {
        Self {
            reg: Register::new(),
            bus: Bus::new(rom_file),
            m: 0,
            halted: false,
            ime: false, // IME should start disabled
            enable_ime_next: false,
            stopped: false,
        }
    }

    fn handle_interrupts(&mut self, push_return: bool) -> bool {
        let ie = self.bus.read_byte(0xFFFF);
        let iflag = self.bus.read_byte(0xFF0F);

        let pending = ie & iflag;
        if pending != 0 {
            self.halted = false;

            let (pc, bit) = if pending & 0x01 != 0 {
                (0x40, 0x01) // v-blank
            } else if pending & 0x02 != 0 {
                (0x48, 0x02) // LCD STAT
            } else if pending & 0x04 != 0 {
                (0x50, 0x04) // Timer
            } else if pending & 0x08 != 0 {
                (0x58, 0x08) // Serial
            } else if pending & 0x10 != 0 {
                (0x60, 0x10) // Joypad
            } else {
                return false;
            };

            if self.ime {
                self.ime = false;
                self.enable_ime_next = false;
            }

            // Clear IF bit for the interrupt being serviced
            self.bus.write_byte(0xFF0F, iflag & !bit);

            if push_return {
                self.reg.sp = self.reg.sp.wrapping_sub(2);
                self.bus.write_word(self.reg.sp, self.reg.pc);
            }

            // Jump to interrupt handler
            self.reg.pc = pc;
            self.m = 5;

            return true;
        }

        false
    }

    // --------------------------- UTIL -----------------------------------------------
    fn read_byte(&mut self) -> u8 {
        let byte = self.bus.read_byte(self.reg.pc);
        self.reg.pc = self.reg.pc.wrapping_add(1);
        byte
    }

    fn read_word(&mut self) -> u16 {
        let word = self.bus.read_word(self.reg.pc);
        self.reg.pc = self.reg.pc.wrapping_add(2);
        word
    }

    // ALU
    // increment register by 1
    fn inc_reg(&mut self, register: u8) -> u8 {
        let result = register.wrapping_add(1);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.unset_flag(Flags::Negative);
        self.set_flag_on_if(Flags::HalfCarry, (register & 0x0F) + 1 > 0x0F);

        result
    }

    // decrement register by 1
    fn dec_reg(&mut self, register: u8) -> u8 {
        let result = register.wrapping_sub(1);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.set_flag(Flags::Negative);
        self.set_flag_on_if(Flags::HalfCarry, (register & 0x0F) == 0);

        result
    }

    fn add16(&mut self, register: u16) {
        let hl = self.reg.get_hl();
        let result = hl.wrapping_add(register);

        self.unset_flag(Flags::Negative);
        self.set_flag_on_if(Flags::Carry, hl > 0xFFFF - register);
        self.set_flag_on_if(
            Flags::HalfCarry,
            (hl & 0x0FFF) + (register & 0x0FFF) > 0x0FFF,
        );
        self.reg.set_hl(result);
    }

    fn add16_imm(&mut self, register: u16) -> u16 {
        let value = self.read_byte() as i8 as i16;
        self.unset_flag(Flags::Negative);
        self.unset_flag(Flags::Zero);

        self.set_flag_on_if(
            Flags::HalfCarry,
            (register & 0xF) + ((value as u16) & 0xF) > 0xF,
        );
        self.set_flag_on_if(
            Flags::Carry,
            (register & 0xFF) + ((value as u16) & 0xFF) > 0xFF,
        );

        (register as i16).wrapping_add(value) as u16
    }

    fn add_a(&mut self, value: u8) {
        let result = self.reg.a.wrapping_add(value);
        self.unset_flag(Flags::Negative);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.set_flag_on_if(Flags::HalfCarry, (self.reg.a & 0xF) + (value & 0xF) > 0xF);
        self.set_flag_on_if(Flags::Carry, self.reg.a as u16 + value as u16 > 0xFF);
        self.reg.a = result;
    }
    fn adc_a_reg(&mut self, value: u8) {
        let carry = if self.flag_is_active(Flags::Carry) {
            1
        } else {
            0
        };
        let result = self.reg.a.wrapping_add(value).wrapping_add(carry);
        self.unset_flag(Flags::Negative);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.set_flag_on_if(
            Flags::HalfCarry,
            (self.reg.a & 0xF) + (value & 0xF) + carry > 0xF,
        );
        self.set_flag_on_if(
            Flags::Carry,
            self.reg.a as u16 + value as u16 + carry as u16 > 0xFF,
        );
        self.reg.a = result;
    }
    fn sub(&mut self, value: u8) {
        let result = self.reg.a.wrapping_sub(value);
        self.set_flag(Flags::Negative);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.set_flag_on_if(Flags::HalfCarry, (self.reg.a & 0x0F) < (value & 0x0F));
        self.set_flag_on_if(Flags::Carry, self.reg.a < value);
        self.reg.a = result;
    }
    fn sbc_a_reg(&mut self, value: u8) {
        let carry = if self.flag_is_active(Flags::Carry) {
            1
        } else {
            0
        };
        let result = self.reg.a.wrapping_sub(value).wrapping_sub(carry);
        self.set_flag(Flags::Negative);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.set_flag_on_if(
            Flags::HalfCarry,
            (self.reg.a & 0x0F) < (value & 0x0F) + carry,
        );
        self.set_flag_on_if(
            Flags::Carry,
            (self.reg.a as u16) < (value as u16) + (carry as u16),
        );
        self.reg.a = result;
    }

    fn and_a_helper(&mut self, value: u8) {
        let result = self.reg.a & value;
        self.set_flag(Flags::HalfCarry);
        self.unset_flag(Flags::Negative);
        self.unset_flag(Flags::Carry);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.reg.a = result;
    }

    fn xor_a_helper(&mut self, value: u8) {
        let result = self.reg.a ^ value;
        self.reset_flags();
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.reg.a = result;
    }

    fn or_a_helper(&mut self, value: u8) {
        let result = self.reg.a | value;
        self.reset_flags();
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.reg.a = result;
    }

    fn cp_a_helper(&mut self, value: u8) {
        let result = self.reg.a.wrapping_sub(value);
        self.set_flag(Flags::Negative);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.set_flag_on_if(Flags::HalfCarry, (self.reg.a & 0x0F) < (value & 0x0F));
        self.set_flag_on_if(Flags::Carry, self.reg.a < value);
    }

    // STACK OPERATIONS
    fn push_stack(&mut self, value: u16) {
        self.reg.sp = self.reg.sp.wrapping_sub(2);
        self.bus.write_word(self.reg.sp, value);
    }

    fn pop_stack(&mut self) -> u16 {
        let value = self.bus.read_word(self.reg.sp);
        self.reg.sp = self.reg.sp.wrapping_add(2);
        value
    }

    fn print_register_data(&self) {
        println!("A: {:02X} F: {:02X} B: {:02X} C: {:02X} D: {:02X} E: {:02X} H: {:02X} L: {:02X} SP: {:04X} PC: 00:{:04X} ({:02X} {:02X} {:02X} {:02X})",
        self.reg.a, self.reg.f, self.reg.b, self.reg.c, self.reg.d, self.reg.e, self.reg.h, self.reg.l, self.reg.sp, self.reg.pc,
        self.bus.read_byte(self.reg.pc),
        self.bus.read_byte(self.reg.pc.wrapping_add(1)),
        self.bus.read_byte(self.reg.pc.wrapping_add(2)),
        self.bus.read_byte(self.reg.pc.wrapping_add(3)));
    }
    fn reset_flags(&mut self) {
        self.reg.f = 0;
    }

    fn set_flag(&mut self, flag: Flags) {
        self.reg.f |= flag as u8;
        self.reg.f &= 0xF0;
    }

    fn unset_flag(&mut self, flag: Flags) {
        self.reg.f &= !(flag as u8);
    }

    fn flag_is_active(&self, flag: Flags) -> bool {
        self.reg.f & (flag as u8) == flag as u8
    }

    fn set_flag_on_if(&mut self, flag: Flags, condition: bool) {
        if condition {
            self.set_flag(flag);
        } else {
            self.unset_flag(flag);
        }
    }

    // rotate register left
    fn cb_rlc(&mut self, register: u8) -> u8 {
        self.m = 2;
        let carry = if register & 0x80 == 0x80 { 1 } else { 0 };
        let reg = register << 1 | carry;
        self.set_flag_on_if(Flags::Zero, reg == 0);
        self.unset_flag(Flags::Negative);
        self.unset_flag(Flags::HalfCarry);
        self.set_flag_on_if(Flags::Carry, register & 0x80 == 0x80);

        reg
    }

    // rotate register right
    fn cb_rrc(&mut self, register: u8) -> u8 {
        self.m = 2;
        let carry = if register & 0x01 == 0x01 { 0x80 } else { 0 };
        let reg = register >> 1 | carry;
        self.set_flag_on_if(Flags::Zero, reg == 0);
        self.unset_flag(Flags::Negative);
        self.unset_flag(Flags::HalfCarry);
        self.set_flag_on_if(Flags::Carry, register & 0x01 == 0x01);

        reg
    }

    // rotate bits in register left through carry
    fn cb_rl(&mut self, register: u8) -> u8 {
        self.m = 2;

        let reg = register << 1
            | if self.flag_is_active(Flags::Carry) {
                1
            } else {
                0
            };
        self.set_flag_on_if(Flags::Zero, reg == 0);
        self.unset_flag(Flags::Negative);
        self.unset_flag(Flags::HalfCarry);
        self.set_flag_on_if(Flags::Carry, register & 0x80 == 0x80);

        reg
    }

    // rotate bits in register right through carry
    fn cb_rr(&mut self, register: u8) -> u8 {
        self.m = 2;

        let reg = register >> 1
            | if self.flag_is_active(Flags::Carry) {
                0x80
            } else {
                0
            };
        self.set_flag_on_if(Flags::Zero, reg == 0);
        self.unset_flag(Flags::Negative);
        self.unset_flag(Flags::HalfCarry);
        self.set_flag_on_if(Flags::Carry, register & 0x01 == 0x01);

        reg
    }

    // shift left arithmetically (arithmetically is replicating the sign bit as needed to fill bit positions)
    // since sometimes it is not desirable to move zeroes into the higher order bits
    fn cb_sla(&mut self, register: u8) -> u8 {
        self.m = 2;

        let reg = register << 1;
        self.unset_flag(Flags::Negative);
        self.unset_flag(Flags::HalfCarry);
        self.set_flag_on_if(Flags::Zero, reg == 0);
        self.set_flag_on_if(Flags::Carry, register & 0x80 == 0x80);

        reg
    }

    // shift right arithmetically
    fn cb_sra(&mut self, register: u8) -> u8 {
        self.m = 2;

        let reg = (register >> 1) | (register & 0x80);
        self.unset_flag(Flags::Negative);
        self.unset_flag(Flags::HalfCarry);
        self.set_flag_on_if(Flags::Zero, reg == 0);
        self.set_flag_on_if(Flags::Carry, register & 0x01 == 0x01);

        reg
    }

    // swap upper 4 bits with the lower 4 in the register
    fn cb_swap(&mut self, register: u8) -> u8 {
        let upper = register >> 4;
        let lower = register << 4;
        let reg = upper | lower;
        self.reset_flags();
        self.set_flag_on_if(Flags::Zero, reg == 0);
        reg
    }

    // shift right logically (right logically moves bits to the right, higher order bits gets zeros and lower order bits are discarded)
    fn cb_srl(&mut self, register: u8) -> u8 {
        self.m = 2;

        let reg = register >> 1;
        self.unset_flag(Flags::Negative);
        self.unset_flag(Flags::HalfCarry);
        self.set_flag_on_if(Flags::Zero, reg == 0);
        self.set_flag_on_if(Flags::Carry, register & 0x01 == 0x01);

        reg
    }

    // test bit n in register, set zero flag if bit not set
    fn cb_bit(&mut self, bit: u8, register: u8) {
        self.m = 4;
        if register & (1 << bit) == 0 {
            self.set_flag(Flags::Zero);
        } else {
            self.unset_flag(Flags::Zero);
        }
        self.unset_flag(Flags::Negative);
        self.set_flag(Flags::HalfCarry);
    }

    // set bit n in register to 0
    fn cb_res(&mut self, bit: u8, register: u8) -> u8 {
        self.m = 2;
        register & !(1 << bit)
    }

    // set bit n in register to 1
    fn cb_set(&mut self, bit: u8, register: u8) -> u8 {
        self.m = 2;
        register | (1 << bit)
    }

    // --------------------------- OPCODES -----------------------------------------------

    // no operation, only advances the program counter by 1
    fn nop(&mut self) {
        self.m = 1;
    }

    // load 2 bytes of data into register pair BC
    fn load_bc(&mut self) {
        self.m = 3;

        let value = self.read_word();
        self.reg.set_bc(value);
    }

    // load data from register A to memory location specified by register pair BC
    fn load_bc_a(&mut self) {
        self.m = 2;

        self.bus.write_byte(self.reg.get_bc(), self.reg.a);
    }

    // increment register pair BC
    fn inc_bc(&mut self) {
        self.m = 2;

        self.reg.set_bc(self.reg.get_bc().wrapping_add(1));
    }

    // increment register B
    fn inc_b(&mut self) {
        self.m = 1;

        self.reg.b = self.inc_reg(self.reg.b);
    }

    // decrement register B
    fn dec_b(&mut self) {
        self.m = 1;

        self.reg.b = self.dec_reg(self.reg.b);
    }

    // load value into register B
    fn load_b(&mut self) {
        self.m = 2;

        self.reg.b = self.read_byte();
    }

    // rotate register A left
    fn rlca(&mut self) {
        self.m = 1;

        self.reset_flags();
        let carry = self.reg.a & 0x80 == 0x80;
        self.reg.a = (self.reg.a << 1) | (if carry { 1 } else { 0 });
        self.set_flag_on_if(Flags::Carry, carry);
    }

    // load stack pointer at given address
    fn load_sp_at_addr(&mut self) {
        self.m = 5;

        let address = self.read_word();
        self.bus.write_word(address, self.reg.sp);
    }

    // add register BC to HL
    fn add_hl_bc(&mut self) {
        self.m = 2;
        self.add16(self.reg.get_bc());
    }

    // load contents specified by register BC into register A
    fn ld_a_bc(&mut self) {
        self.m = 2;

        self.reg.a = self.bus.read_byte(self.reg.get_bc());
    }

    // decrement register pair BC by 1
    fn dec_bc(&mut self) {
        self.m = 2;

        self.reg.set_bc(self.reg.get_bc().wrapping_sub(1));
    }

    // increment contents of register C by 1
    fn inc_c(&mut self) {
        self.m = 1;

        self.reg.c = self.inc_reg(self.reg.c);
    }

    // decrement contents of register C by 1
    fn dec_c(&mut self) {
        self.m = 1;

        self.reg.c = self.dec_reg(self.reg.c);
    }

    // load immediate operand into register C
    fn ld_c(&mut self) {
        self.m = 2;

        self.reg.c = self.read_byte();
    }

    // Rotate contents of register A to the right
    fn rrca(&mut self) {
        self.m = 1;

        self.reset_flags();
        let carry = self.reg.a & 0x01 == 0x01;
        self.reg.a = (self.reg.a >> 1) | (if self.reg.a & 0x01 == 0x01 { 0x80 } else { 0 });
        self.set_flag_on_if(Flags::Carry, carry);
    }

    // stop system clock and oscillator circuit
    fn stop(&mut self) {
        self.m = 2;
        self.stopped = true;
        self.read_byte();
    }

    // load 2 bytes of immediate data into register pair DE
    fn ld_de(&mut self) {
        self.m = 3;

        let value = self.read_word();
        self.reg.set_de(value);
    }

    // store contents of register A in memory location specified by register pair DE
    fn ld_de_a(&mut self) {
        self.m = 2;

        let a = self.reg.a;
        self.bus.write_byte(self.reg.get_de(), a);

    }

    // increment contents of register pair DE by 1
    fn inc_de(&mut self) {
        self.m = 2;

        self.reg.set_de(self.reg.get_de().wrapping_add(1));
    }

    // increment contents of register D by 1
    fn inc_d(&mut self) {
        self.m = 1;

        self.reg.d = self.inc_reg(self.reg.d);
    }

    // decrement contents of register D by 1
    fn dec_d(&mut self) {
        self.m = 1;

        self.reg.d = self.dec_reg(self.reg.d);
    }

    // load 8-bit immediate operand into register D
    fn ld_d(&mut self) {
        self.m = 2;

        self.reg.d = self.read_byte();
    }

    // rotate contents of register A to the left, through the carry flag
    fn rla(&mut self) {
        self.m = 1;

        let old_carry = if self.flag_is_active(Flags::Carry) {
            1
        } else {
            0
        };
        let carry = self.reg.a & 0x80 == 0x80;

        self.reg.a = (self.reg.a << 1) | old_carry;

        self.reset_flags();
        self.set_flag_on_if(Flags::Carry, carry);
        //self.reset_flags();
        //let carry = self.reg.a & 0x80 == 0x80;
        //self.reg.a = (self.reg.a << 1) | (if carry { 1 } else { 0 });
        //self.set_flag_on_if(Flags::Carry, carry);
    }

    // jump s8 steps from current address in the pc
    fn jr(&mut self) {
        self.m = 3;
        let value = self.read_byte() as i8;
        self.reg.pc = ((self.reg.pc as u32 as i32) + (value as i32)) as u16;
    }

    // add contents of register pair DE to the contents of register pair HL
    fn add_hl_de(&mut self) {
        self.m = 2;

        self.add16(self.reg.get_de());
    }

    // load 8-bit contents of memory specified by register pair DE into register A
    fn ld_a_de(&mut self) {
        self.m = 2;

        self.reg.a = self.bus.read_byte(self.reg.get_de());
    }

    // decrement contents of register pair DE by 1
    fn dec_de(&mut self) {
        self.m = 2;

        self.reg.set_de(self.reg.get_de().wrapping_sub(1));
    }

    // increment contents of register E by 1
    fn inc_e(&mut self) {
        self.m = 1;

        self.reg.e = self.inc_reg(self.reg.e);
    }

    // decrement contents of register E by 1
    fn dec_e(&mut self) {
        self.m = 1;

        self.reg.e = self.dec_reg(self.reg.e);
    }

    // load 8-bit immediate operand into register E
    fn ld_e(&mut self) {
        self.m = 2;

        self.reg.e = self.read_byte();
    }

    // rotate contents of register A to the right through carry flag
    fn rra(&mut self) {
        self.m = 1;

        let old_carry = if self.flag_is_active(Flags::Carry) {
            1
        } else {
            0
        };
        let old_a = self.reg.a;
        let new_carry = old_a & 0x01;
        self.reg.a = (old_a >> 1) | (old_carry << 7);

        self.unset_flag(Flags::Zero);
        self.unset_flag(Flags::Negative);
        self.unset_flag(Flags::HalfCarry);
        self.set_flag_on_if(Flags::Carry, new_carry != 0);
    }

    // if z flag is 0, jump s8 steps from current address in pc
    // if not, instruction following is executed
    fn jr_nz(&mut self) {
        let offset = self.read_byte() as i8;
        if !self.flag_is_active(Flags::Zero) {
            self.m = 3;
            self.reg.pc = self.reg.pc.wrapping_add(offset as i16 as u16);
        } else {
            self.m = 2;
        }
    }

    // load 2 bytes of immediate data into register pair HL
    fn ld_hl(&mut self) {
        self.m = 3;

        let value = self.read_word();
        self.reg.set_hl(value);
    }

    // store contents of register A into memory location specified by register pair HL
    // and increment the contents of HL
    fn ld_hl_inc_a(&mut self) {
        self.m = 2;

        self.bus.write_byte(self.reg.get_hl(), self.reg.a);
        self.reg.set_hl(self.reg.get_hl().wrapping_add(1));
    }

    // increment contents of register pair HL by 1
    fn inc_hl(&mut self) {
        self.m = 2;

        self.reg.set_hl(self.reg.get_hl().wrapping_add(1));
    }

    // increment contents of register H by 1
    fn inc_h(&mut self) {
        self.m = 1;

        self.reg.h = self.inc_reg(self.reg.h);
    }

    // decrement contents of register H by 1
    fn dec_h(&mut self) {
        self.m = 1;

        self.reg.h = self.dec_reg(self.reg.h);
    }

    // load 8-bit immediate operand into register H
    fn ld_h(&mut self) {
        self.m = 2;

        self.reg.h = self.read_byte();
    }

    // Decimal Adjust Accumulator, get binary-coded decimal representation after an arithmetic instruction
    // binary-coded decimal is a binary encoding of decimal numbers where each digit is represented
    // by a fixed number of bits, usually 4 or 8
    fn daa(&mut self) {
        self.m = 1;

        let n = self.flag_is_active(Flags::Negative);
        let h = self.flag_is_active(Flags::HalfCarry);
        let c = self.flag_is_active(Flags::Carry);
        let mut a = self.reg.a;
        let mut new_c = c;

        if !n {
            if c || a > 0x99 {
                a = a.wrapping_add(0x60);
                new_c = true;
            }
            if h || (a & 0x0F) > 0x09 {
                a = a.wrapping_add(0x06);
            }
        } else {
            if h {
                a = a.wrapping_sub(6);
            }
            if c {
                a = a.wrapping_sub(0x60);
            }
        }

        self.reg.a = a;

        self.set_flag_on_if(Flags::Zero, a == 0);
        self.unset_flag(Flags::HalfCarry);
        self.set_flag_on_if(Flags::Carry, new_c);
    }

    // if z flag is active, jump s8 steps from current address else instruction following
    // is executed
    fn jr_z(&mut self) {
        if self.flag_is_active(Flags::Zero) {
            self.m = 3;
            let value = self.read_byte() as i8;
            self.reg.pc = ((self.reg.pc as u32 as i32) + (value as i32)) as u16;
        } else {
            self.m = 2;
            self.reg.pc = self.reg.pc.wrapping_add(1);
        }
    }

    // add contents of register pair HL to the contents of register pair HL and store in HL
    fn add_hl_hl(&mut self) {
        self.m = 2;

        self.add16(self.reg.get_hl());
    }

    // load contents of memory specified by register pair HL into register A and increase
    // contents of HL
    fn ld_a_hl_plus(&mut self) {
        self.m = 2;

        let hl = self.reg.get_hl();
        let value = self.bus.read_byte(hl);

        self.reg.a = value;
        self.reg.set_hl(hl.wrapping_add(1));
    }

    // decrement contents of register pair HL by 1
    fn dec_hl(&mut self) {
        self.m = 2;

        self.reg.set_hl(self.reg.get_hl().wrapping_sub(1));
    }

    // increment contents of register L by 1
    fn inc_l(&mut self) {
        self.m = 1;

        self.reg.l = self.inc_reg(self.reg.l);
    }

    // decrement contents of register L by 1
    fn dec_l(&mut self) {
        self.m = 1;

        self.reg.l = self.dec_reg(self.reg.l);
    }

    // load 8-bit immediate operand into register L
    fn ld_l(&mut self) {
        self.m = 2;

        self.reg.l = self.read_byte();
    }

    // flip all contents of register A
    fn cpl(&mut self) {
        self.m = 1;

        self.reg.a = !self.reg.a;
        self.set_flag(Flags::Negative);
        self.set_flag(Flags::HalfCarry);
    }

    // if CY flag is not set, jump s8 steps from current address
    // else instruction following JP is executed
    fn jr_nc(&mut self) {
        if !self.flag_is_active(Flags::Carry) {
            self.m = 3;
            let value = self.read_byte() as i8;
            self.reg.pc = ((self.reg.pc as u32 as i32) + (value as i32)) as u16;
        } else {
            self.m = 2;
            self.reg.pc = self.reg.pc.wrapping_add(1);
        }
    }

    // load 2 bytes of immediate data into register pair SP
    fn ld_sp(&mut self) {
        self.m = 3;

        self.reg.sp = self.read_word();
    }

    // store contents of register A in memory location specified by register pair HL
    // and decrement contents of HL
    fn ld_hlm_a(&mut self) {
        self.m = 2;

        self.bus.write_byte(self.reg.get_hl(), self.reg.a);
        self.reg.set_hl(self.reg.get_hl().wrapping_sub(1));
    }

    // increment contents of register pair SP by 1
    fn inc_sp(&mut self) {
        self.m = 2;

        self.reg.sp = self.reg.sp.wrapping_add(1);
    }

    // increment contents of memory specified by register pair HL by 1
    fn inc_content_at_hl(&mut self) {
        self.m = 3;
        let addr = self.reg.get_hl();
        let orig = self.bus.read_byte(addr);
        let value = orig.wrapping_add(1);
        self.unset_flag(Flags::Negative);
        self.set_flag_on_if(Flags::Zero, value == 0);
        self.set_flag_on_if(Flags::HalfCarry, (orig & 0xF) + 1 > 0xF);
        self.bus.write_byte(addr, value);
    }

    // decrement contents of memory specifed by register pair HL by 1
    fn dec_content_at_hl(&mut self) {
        self.m = 3;
        let addr = self.reg.get_hl();
        let orig = self.bus.read_byte(addr);
        let value = orig.wrapping_sub(1);

        self.set_flag(Flags::Negative);
        self.set_flag_on_if(Flags::Zero, value == 0);
        self.set_flag_on_if(Flags::HalfCarry, orig & 0xF == 0);
        self.bus.write_byte(addr, value);
    }

    // store contents of 8-bit immediate operation into memory location
    // specified by register pair HL
    fn ld_hl_byte(&mut self) {
        self.m = 3;
        let addr = self.reg.get_hl();
        let value = self.read_byte();
        self.bus.write_byte(addr, value);
    }

    // set the carry flag
    fn scf(&mut self) {
        self.m = 1;

        self.set_flag(Flags::Carry);
        self.unset_flag(Flags::Negative);
        self.unset_flag(Flags::HalfCarry);
    }

    // if carry flag is active, jump s8 steps from current address
    // else instruction following jp is executed
    fn jr_c(&mut self) {
        if self.flag_is_active(Flags::Carry) {
            self.m = 3;
            let value = self.read_byte() as i8;
            self.reg.pc = ((self.reg.pc as u32 as i32) + value as i32) as u16;
        } else {
            self.m = 2;
            self.reg.pc = self.reg.pc.wrapping_add(1);
        }
    }

    // add contents of register pair SP to contents of register pair HL
    fn add_hl_sp(&mut self) {
        self.m = 2;

        self.add16(self.reg.sp);
    }

    // load contents specified by register pair HL into register A
    // decrement contents of HL
    fn ld_a_hl_dec(&mut self) {
        self.m = 2;

        //self.reg.set_hl(self.reg.get_hl().wrapping_sub(1));
        self.reg.a = self.bus.read_byte(self.reg.get_hl());
        self.reg.set_hl(self.reg.get_hl().wrapping_sub(1));
    }

    // decrement contents of register pair SP by 1
    fn dec_sp(&mut self) {
        self.m = 2;

        self.reg.sp = self.reg.sp.wrapping_sub(1);
    }

    // increment contents of register A by 1
    fn inc_a(&mut self) {
        self.m = 1;

        self.reg.a = self.inc_reg(self.reg.a);
    }

    // decrement contents of register A by 1
    fn dec_a(&mut self) {
        self.m = 1;

        self.reg.a = self.dec_reg(self.reg.a);
    }

    // load 8-bit immediate operand into register A
    fn ld_a_byte(&mut self) {
        self.m = 2;

        self.reg.a = self.read_byte();
    }

    // flip carry flag
    fn ccf(&mut self) {
        self.m = 1;

        self.unset_flag(Flags::Negative);
        self.unset_flag(Flags::HalfCarry);
        self.reg.f ^= 1 << 4;
        self.reg.f &= 0xF0;
    }

    fn ld_b_b(&mut self) {
        self.m = 1;
        self.reg.b = self.reg.b;
    }
    fn ld_b_c(&mut self) {
        self.m = 1;
        self.reg.b = self.reg.c;
    }
    fn ld_b_d(&mut self) {
        self.m = 1;
        self.reg.b = self.reg.d;
    }
    fn ld_b_e(&mut self) {
        self.m = 1;
        self.reg.b = self.reg.e;
    }
    fn ld_b_h(&mut self) {
        self.m = 1;
        self.reg.b = self.reg.h;
    }
    fn ld_b_l(&mut self) {
        self.m = 1;
        self.reg.b = self.reg.l;
    }
    fn ld_b_hl(&mut self) {
        self.m = 2;
        let addr = self.reg.get_hl();
        self.reg.b = self.bus.read_byte(addr);
    }
    fn ld_b_a(&mut self) {
        self.m = 1;
        self.reg.b = self.reg.a;
    }

    fn ld_c_b(&mut self) {
        self.m = 1;
        self.reg.c = self.reg.b;
    }
    fn ld_c_c(&mut self) {
        self.m = 1;
        self.reg.c = self.reg.c;
    }
    fn ld_c_d(&mut self) {
        self.m = 1;
        self.reg.c = self.reg.d;
    }
    fn ld_c_e(&mut self) {
        self.m = 1;
        self.reg.c = self.reg.e;
    }
    fn ld_c_h(&mut self) {
        self.m = 1;
        self.reg.c = self.reg.h;
    }
    fn ld_c_l(&mut self) {
        self.m = 1;
        self.reg.c = self.reg.l;
    }
    fn ld_c_hl(&mut self) {
        self.m = 2;
        let addr = self.reg.get_hl();
        self.reg.c = self.bus.read_byte(addr);
    }
    fn ld_c_a(&mut self) {
        self.m = 1;
        self.reg.c = self.reg.a;
    }

    fn ld_d_b(&mut self) {
        self.m = 1;
        self.reg.d = self.reg.b;
    }
    fn ld_d_c(&mut self) {
        self.m = 1;
        self.reg.d = self.reg.c;
    }
    fn ld_d_d(&mut self) {
        self.m = 1;
        self.reg.d = self.reg.d;
    }
    fn ld_d_e(&mut self) {
        self.m = 1;
        self.reg.d = self.reg.e;
    }
    fn ld_d_h(&mut self) {
        self.m = 1;
        self.reg.d = self.reg.h;
    }
    fn ld_d_l(&mut self) {
        self.m = 1;
        self.reg.d = self.reg.l;
    }
    fn ld_d_hl(&mut self) {
        self.m = 2;
        let addr = self.reg.get_hl();
        self.reg.d = self.bus.read_byte(addr);
    }
    fn ld_d_a(&mut self) {
        self.m = 1;
        self.reg.d = self.reg.a;
    }

    fn ld_e_b(&mut self) {
        self.m = 1;
        self.reg.e = self.reg.b;
    }
    fn ld_e_c(&mut self) {
        self.m = 1;
        self.reg.e = self.reg.c;
    }
    fn ld_e_d(&mut self) {
        self.m = 1;
        self.reg.e = self.reg.d;
    }
    fn ld_e_e(&mut self) {
        self.m = 1;
        self.reg.e = self.reg.e;
    }
    fn ld_e_h(&mut self) {
        self.m = 1;
        self.reg.e = self.reg.h;
    }
    fn ld_e_l(&mut self) {
        self.m = 1;
        self.reg.e = self.reg.l;
    }
    fn ld_e_hl(&mut self) {
        self.m = 2;
        let addr = self.reg.get_hl();
        self.reg.e = self.bus.read_byte(addr);
    }
    fn ld_e_a(&mut self) {
        self.m = 1;
        self.reg.e = self.reg.a;
    }

    fn ld_h_b(&mut self) {
        self.m = 1;
        self.reg.h = self.reg.b;
    }
    fn ld_h_c(&mut self) {
        self.m = 1;
        self.reg.h = self.reg.c;
    }
    fn ld_h_d(&mut self) {
        self.m = 1;
        self.reg.h = self.reg.d;
    }
    fn ld_h_e(&mut self) {
        self.m = 1;
        self.reg.h = self.reg.e;
    }
    fn ld_h_h(&mut self) {
        self.m = 1;
        self.reg.h = self.reg.h;
    }
    fn ld_h_l(&mut self) {
        self.m = 1;
        self.reg.h = self.reg.l;
    }
    fn ld_h_hl(&mut self) {
        self.m = 2;
        let addr = self.reg.get_hl();
        self.reg.h = self.bus.read_byte(addr);
    }
    fn ld_h_a(&mut self) {
        self.m = 1;
        self.reg.h = self.reg.a;
    }

    fn ld_l_b(&mut self) {
        self.m = 1;
        self.reg.l = self.reg.b;
    }
    fn ld_l_c(&mut self) {
        self.m = 1;
        self.reg.l = self.reg.c;
    }
    fn ld_l_d(&mut self) {
        self.m = 1;
        self.reg.l = self.reg.d;
    }
    fn ld_l_e(&mut self) {
        self.m = 1;
        self.reg.l = self.reg.e;
    }
    fn ld_l_h(&mut self) {
        self.m = 1;
        self.reg.l = self.reg.h;
    }
    fn ld_l_l(&mut self) {
        self.m = 1;
        self.reg.l = self.reg.l;
    }
    fn ld_l_hl(&mut self) {
        self.m = 2;
        let addr = self.reg.get_hl();
        self.reg.l = self.bus.read_byte(addr);
    }
    fn ld_l_a(&mut self) {
        self.m = 1;
        self.reg.l = self.reg.a;
    }

    fn ld_hl_b(&mut self) {
        self.m = 2;
        let addr = self.reg.get_hl();
        self.bus.write_byte(addr, self.reg.b);
    }
    fn ld_hl_c(&mut self) {
        self.m = 2;
        let addr = self.reg.get_hl();
        self.bus.write_byte(addr, self.reg.c);
    }
    fn ld_hl_d(&mut self) {
        self.m = 2;
        let addr = self.reg.get_hl();
        self.bus.write_byte(addr, self.reg.d);
    }
    fn ld_hl_e(&mut self) {
        self.m = 2;
        let addr = self.reg.get_hl();
        self.bus.write_byte(addr, self.reg.e);
    }
    fn ld_hl_h(&mut self) {
        self.m = 2;
        let addr = self.reg.get_hl();
        self.bus.write_byte(addr, self.reg.h);
    }
    fn ld_hl_l(&mut self) {
        self.m = 2;
        let addr = self.reg.get_hl();
        self.bus.write_byte(addr, self.reg.l);
    }
    fn ld_hl_a(&mut self) {
        self.m = 2;
        let addr = self.reg.get_hl();
        self.bus.write_byte(addr, self.reg.a);
    }

    fn add_a_b(&mut self) {
        self.m = 1;
        self.add_a(self.reg.b);
    }
    fn add_a_c(&mut self) {
        self.m = 1;
        self.add_a(self.reg.c);
    }
    fn add_a_d(&mut self) {
        self.m = 1;
        self.add_a(self.reg.d);
    }
    fn add_a_e(&mut self) {
        self.m = 1;
        self.add_a(self.reg.e);
    }
    fn add_a_h(&mut self) {
        self.m = 1;
        self.add_a(self.reg.h);
    }
    fn add_a_l(&mut self) {
        self.m = 1;
        self.add_a(self.reg.l);
    }
    fn add_a_hl(&mut self) {
        self.m = 2;
        self.add_a(self.bus.read_byte(self.reg.get_hl()));
    }
    fn add_a_a(&mut self) {
        self.m = 1;
        self.add_a(self.reg.a);
    }

    fn adc_a_b(&mut self) {
        self.m = 1;
        self.adc_a_reg(self.reg.b);
    }
    fn adc_a_c(&mut self) {
        self.m = 1;
        self.adc_a_reg(self.reg.c);
    }
    fn adc_a_d(&mut self) {
        self.m = 1;
        self.adc_a_reg(self.reg.d);
    }
    fn adc_a_e(&mut self) {
        self.m = 1;
        self.adc_a_reg(self.reg.e);
    }
    fn adc_a_h(&mut self) {
        self.m = 1;
        self.adc_a_reg(self.reg.h);
    }
    fn adc_a_l(&mut self) {
        self.m = 1;
        self.adc_a_reg(self.reg.l);
    }
    fn adc_a_hl(&mut self) {
        self.m = 2;
        self.adc_a_reg(self.bus.read_byte(self.reg.get_hl()));
    }
    fn adc_a_a(&mut self) {
        self.m = 1;
        self.adc_a_reg(self.reg.a);
    }

    fn sub_b(&mut self) {
        self.m = 1;
        self.sub(self.reg.b);
    }
    fn sub_c(&mut self) {
        self.m = 1;
        self.sub(self.reg.c);
    }
    fn sub_d(&mut self) {
        self.m = 1;
        self.sub(self.reg.d);
    }
    fn sub_e(&mut self) {
        self.m = 1;
        self.sub(self.reg.e);
    }
    fn sub_h(&mut self) {
        self.m = 1;
        self.sub(self.reg.h);
    }
    fn sub_l(&mut self) {
        self.m = 1;
        self.sub(self.reg.l);
    }
    fn sub_hl(&mut self) {
        self.m = 2;
        self.sub(self.bus.read_byte(self.reg.get_hl()));
    }
    fn sub_a(&mut self) {
        self.m = 1;
        self.sub(self.reg.a);
    }

    fn sbc_a_b(&mut self) {
        self.m = 1;
        self.sbc_a_reg(self.reg.b);
    }
    fn sbc_a_c(&mut self) {
        self.m = 1;
        self.sbc_a_reg(self.reg.c);
    }
    fn sbc_a_d(&mut self) {
        self.m = 1;
        self.sbc_a_reg(self.reg.d);
    }
    fn sbc_a_e(&mut self) {
        self.m = 1;
        self.sbc_a_reg(self.reg.e);
    }
    fn sbc_a_h(&mut self) {
        self.m = 1;
        self.sbc_a_reg(self.reg.h);
    }
    fn sbc_a_l(&mut self) {
        self.m = 1;
        self.sbc_a_reg(self.reg.l);
    }
    fn sbc_a_hl(&mut self) {
        self.m = 2;
        self.sbc_a_reg(self.bus.read_byte(self.reg.get_hl()));
    }
    fn sbc_a_a(&mut self) {
        self.m = 1;
        self.sbc_a_reg(self.reg.a);
    }

    fn and_a_b(&mut self) {
        self.m = 1;
        self.and_a_helper(self.reg.b);
    }
    fn and_a_c(&mut self) {
        self.m = 1;
        self.and_a_helper(self.reg.c);
    }
    fn and_a_d(&mut self) {
        self.m = 1;
        self.and_a_helper(self.reg.d);
    }
    fn and_a_e(&mut self) {
        self.m = 1;
        self.and_a_helper(self.reg.e);
    }
    fn and_a_h(&mut self) {
        self.m = 1;
        self.and_a_helper(self.reg.h);
    }
    fn and_a_l(&mut self) {
        self.m = 1;
        self.and_a_helper(self.reg.l);
    }
    fn and_a_hl(&mut self) {
        self.m = 2;
        self.and_a_helper(self.bus.read_byte(self.reg.get_hl()));
    }
    fn and_a_a(&mut self) {
        self.m = 1;
        self.and_a_helper(self.reg.a);
    }

    fn xor_a_b(&mut self) {
        self.m = 1;
        self.xor_a_helper(self.reg.b);
    }
    fn xor_a_c(&mut self) {
        self.m = 1;
        self.xor_a_helper(self.reg.c);
    }
    fn xor_a_d(&mut self) {
        self.m = 1;
        self.xor_a_helper(self.reg.d);
    }
    fn xor_a_e(&mut self) {
        self.m = 1;
        self.xor_a_helper(self.reg.e);
    }
    fn xor_a_h(&mut self) {
        self.m = 1;
        self.xor_a_helper(self.reg.h);
    }
    fn xor_a_l(&mut self) {
        self.m = 1;
        self.xor_a_helper(self.reg.l);
    }
    fn xor_a_hl(&mut self) {
        self.m = 2;
        self.xor_a_helper(self.bus.read_byte(self.reg.get_hl()));
    }
    fn xor_a_a(&mut self) {
        self.m = 1;
        self.xor_a_helper(self.reg.a);
    }

    fn or_a_b(&mut self) {
        self.m = 1;
        self.or_a_helper(self.reg.b);
    }
    fn or_a_c(&mut self) {
        self.m = 1;
        self.or_a_helper(self.reg.c);
    }
    fn or_a_d(&mut self) {
        self.m = 1;
        self.or_a_helper(self.reg.d);
    }
    fn or_a_e(&mut self) {
        self.m = 1;
        self.or_a_helper(self.reg.e);
    }
    fn or_a_h(&mut self) {
        self.m = 1;
        self.or_a_helper(self.reg.h);
    }
    fn or_a_l(&mut self) {
        self.m = 1;
        self.or_a_helper(self.reg.l);
    }
    fn or_a_hl(&mut self) {
        self.m = 2;
        self.or_a_helper(self.bus.read_byte(self.reg.get_hl()));
    }
    fn or_a_a(&mut self) {
        self.m = 1;
        self.or_a_helper(self.reg.a);
    }

    fn cp_a_b(&mut self) {
        self.m = 1;
        self.cp_a_helper(self.reg.b);
    }
    fn cp_a_c(&mut self) {
        self.m = 1;
        self.cp_a_helper(self.reg.c);
    }
    fn cp_a_d(&mut self) {
        self.m = 1;
        self.cp_a_helper(self.reg.d);
    }
    fn cp_a_e(&mut self) {
        self.m = 1;
        self.cp_a_helper(self.reg.e);
    }
    fn cp_a_h(&mut self) {
        self.m = 1;
        self.cp_a_helper(self.reg.h);
    }
    fn cp_a_l(&mut self) {
        self.m = 1;
        self.cp_a_helper(self.reg.l);
    }
    fn cp_a_hl(&mut self) {
        self.m = 2;
        self.cp_a_helper(self.bus.read_byte(self.reg.get_hl()));
    }
    fn cp_a_a(&mut self) {
        self.m = 1;
        self.cp_a_helper(self.reg.a);
    }

    // return from subroutine if nz
    fn ret_nz(&mut self) {
        if !self.flag_is_active(Flags::Zero) {
            self.m = 5;
            self.reg.pc = self.pop_stack();
        } else {
            self.m = 2;
        }
    }

    // pop contents of memory stack into register pair BC
    fn pop_bc(&mut self) {
        self.m = 3;

        let value = self.pop_stack();
        self.reg.set_bc(value);
    }

    // jump to address if condition is met
    fn jp_nz(&mut self) {
        if !self.flag_is_active(Flags::Zero) {
            self.m = 4;
            self.reg.pc = self.read_word();
        } else {
            self.m = 3;
            self.reg.pc = self.reg.pc.wrapping_add(2);
        }
    }

    // jump to address
    fn jp(&mut self) {
        self.m = 4;
        self.reg.pc = self.read_word();
    }

    // call address if condition is met
    fn call_nz(&mut self) {
        if !self.flag_is_active(Flags::Zero) {
            self.m = 6;
            self.push_stack(self.reg.pc.wrapping_add(2));
            self.reg.pc = self.read_word();
        } else {
            self.reg.pc = self.reg.pc.wrapping_add(2);
            self.m = 3;
        }
    }

    // push contents of register pair BC onto the memory stack
    fn push_bc(&mut self) {
        self.m = 4;

        self.push_stack(self.reg.get_bc());
    }

    // add 8-bit immediate to register A
    fn add_a_byte(&mut self) {
        self.m = 2;

        let value = self.read_byte();
        let result = self.reg.a.wrapping_add(value);
        self.unset_flag(Flags::Negative);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.set_flag_on_if(Flags::HalfCarry, (self.reg.a & 0xF) + (value & 0xF) > 0xF);
        self.set_flag_on_if(Flags::Carry, self.reg.a as u16 + value as u16 > 0xFF);
        self.reg.a = result;
    }

    // call address
    fn rst_zero(&mut self) {
        self.m = 4;

        self.push_stack(self.reg.pc);
        self.reg.pc = 0x00
    }

    // return from subroutine if condition is met
    fn ret_z(&mut self) {
        if self.flag_is_active(Flags::Zero) {
            self.m = 5;
            self.reg.pc = self.pop_stack();
        } else {
            self.m = 2;
        }
    }

    // return from subroutine
    fn ret(&mut self) {
        self.m = 4;

        self.reg.pc = self.pop_stack();
    }

    // jump to address if condition is met
    fn jp_z(&mut self) {
        if self.flag_is_active(Flags::Zero) {
            self.m = 4;
            self.reg.pc = self.read_word();
        } else {
            self.m = 3;
            self.reg.pc = self.reg.pc.wrapping_add(2);
        }
    }

    // call opcode from from the CB-prefix table
    fn call_cb(&mut self) {
        let opcode = self.read_byte();
        match opcode {
            0x00 => self.reg.b = self.cb_rlc(self.reg.b),
            0x01 => self.reg.c = self.cb_rlc(self.reg.c),
            0x02 => self.reg.d = self.cb_rlc(self.reg.d),
            0x03 => self.reg.e = self.cb_rlc(self.reg.e),
            0x04 => self.reg.h = self.cb_rlc(self.reg.h),
            0x05 => self.reg.l = self.cb_rlc(self.reg.l),
            0x06 => {
                self.m = 4;
                let value = self.cb_rlc(self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0x07 => self.reg.a = self.cb_rlc(self.reg.a),
            0x08 => self.reg.b = self.cb_rrc(self.reg.b),
            0x09 => self.reg.c = self.cb_rrc(self.reg.c),
            0x0A => self.reg.d = self.cb_rrc(self.reg.d),
            0x0B => self.reg.e = self.cb_rrc(self.reg.e),
            0x0C => self.reg.h = self.cb_rrc(self.reg.h),
            0x0D => self.reg.l = self.cb_rrc(self.reg.l),
            0x0E => {
                self.m = 4;
                let value = self.cb_rrc(self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value)
            }
            0x0F => self.reg.a = self.cb_rrc(self.reg.a),
            0x10 => self.reg.b = self.cb_rl(self.reg.b),
            0x11 => self.reg.c = self.cb_rl(self.reg.c),
            0x12 => self.reg.d = self.cb_rl(self.reg.d),
            0x13 => self.reg.e = self.cb_rl(self.reg.e),
            0x14 => self.reg.h = self.cb_rl(self.reg.h),
            0x15 => self.reg.l = self.cb_rl(self.reg.l),
            0x16 => {
                self.m = 4;
                let value = self.cb_rl(self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0x17 => self.reg.a = self.cb_rl(self.reg.a),
            0x18 => self.reg.b = self.cb_rr(self.reg.b),
            0x19 => self.reg.c = self.cb_rr(self.reg.c),
            0x1A => self.reg.d = self.cb_rr(self.reg.d),
            0x1B => self.reg.e = self.cb_rr(self.reg.e),
            0x1C => self.reg.h = self.cb_rr(self.reg.h),
            0x1D => self.reg.l = self.cb_rr(self.reg.l),
            0x1E => {
                self.m = 4;
                let value = self.cb_rr(self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0x1F => self.reg.a = self.cb_rr(self.reg.a),
            0x20 => self.reg.b = self.cb_sla(self.reg.b),
            0x21 => self.reg.c = self.cb_sla(self.reg.c),
            0x22 => self.reg.d = self.cb_sla(self.reg.d),
            0x23 => self.reg.e = self.cb_sla(self.reg.e),
            0x24 => self.reg.h = self.cb_sla(self.reg.h),
            0x25 => self.reg.l = self.cb_sla(self.reg.l),
            0x26 => {
                self.m = 4;
                let value = self.cb_sla(self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0x27 => self.reg.a = self.cb_sla(self.reg.a),
            0x28 => self.reg.b = self.cb_sra(self.reg.b),
            0x29 => self.reg.c = self.cb_sra(self.reg.c),
            0x2A => self.reg.d = self.cb_sra(self.reg.d),
            0x2B => self.reg.e = self.cb_sra(self.reg.e),
            0x2C => self.reg.h = self.cb_sra(self.reg.h),
            0x2D => self.reg.l = self.cb_sra(self.reg.l),
            0x2E => {
                self.m = 4;
                let value = self.cb_sra(self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0x2F => self.reg.a = self.cb_sra(self.reg.a),
            0x30 => self.reg.b = self.cb_swap(self.reg.b),
            0x31 => self.reg.c = self.cb_swap(self.reg.c),
            0x32 => self.reg.d = self.cb_swap(self.reg.d),
            0x33 => self.reg.e = self.cb_swap(self.reg.e),
            0x34 => self.reg.h = self.cb_swap(self.reg.h),
            0x35 => self.reg.l = self.cb_swap(self.reg.l),
            0x36 => {
                self.m = 4;
                let value = self.cb_swap(self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0x37 => self.reg.a = self.cb_swap(self.reg.a),
            0x38 => self.reg.b = self.cb_srl(self.reg.b),
            0x39 => self.reg.c = self.cb_srl(self.reg.c),
            0x3A => self.reg.d = self.cb_srl(self.reg.d),
            0x3B => self.reg.e = self.cb_srl(self.reg.e),
            0x3C => self.reg.h = self.cb_srl(self.reg.h),
            0x3D => self.reg.l = self.cb_srl(self.reg.l),
            0x3E => {
                self.m = 4;
                let value = self.cb_srl(self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0x3F => self.reg.a = self.cb_srl(self.reg.a),
            0x40 => self.cb_bit(0, self.reg.b),
            0x41 => self.cb_bit(0, self.reg.c),
            0x42 => self.cb_bit(0, self.reg.d),
            0x43 => self.cb_bit(0, self.reg.e),
            0x44 => self.cb_bit(0, self.reg.h),
            0x45 => self.cb_bit(0, self.reg.l),
            0x46 => self.cb_bit(0, self.bus.read_byte(self.reg.get_hl())),
            0x47 => self.cb_bit(0, self.reg.a),
            0x48 => self.cb_bit(1, self.reg.b),
            0x49 => self.cb_bit(1, self.reg.c),
            0x4A => self.cb_bit(1, self.reg.d),
            0x4B => self.cb_bit(1, self.reg.e),
            0x4C => self.cb_bit(1, self.reg.h),
            0x4D => self.cb_bit(1, self.reg.l),
            0x4E => self.cb_bit(1, self.bus.read_byte(self.reg.get_hl())),
            0x4F => self.cb_bit(1, self.reg.a),
            0x50 => self.cb_bit(2, self.reg.b),
            0x51 => self.cb_bit(2, self.reg.c),
            0x52 => self.cb_bit(2, self.reg.d),
            0x53 => self.cb_bit(2, self.reg.e),
            0x54 => self.cb_bit(2, self.reg.h),
            0x55 => self.cb_bit(2, self.reg.l),
            0x56 => self.cb_bit(2, self.bus.read_byte(self.reg.get_hl())),
            0x57 => self.cb_bit(2, self.reg.a),
            0x58 => self.cb_bit(3, self.reg.b),
            0x59 => self.cb_bit(3, self.reg.c),
            0x5A => self.cb_bit(3, self.reg.d),
            0x5B => self.cb_bit(3, self.reg.e),
            0x5C => self.cb_bit(3, self.reg.h),
            0x5D => self.cb_bit(3, self.reg.l),
            0x5E => self.cb_bit(3, self.bus.read_byte(self.reg.get_hl())),
            0x5F => self.cb_bit(3, self.reg.a),
            0x60 => self.cb_bit(4, self.reg.b),
            0x61 => self.cb_bit(4, self.reg.c),
            0x62 => self.cb_bit(4, self.reg.d),
            0x63 => self.cb_bit(4, self.reg.e),
            0x64 => self.cb_bit(4, self.reg.h),
            0x65 => self.cb_bit(4, self.reg.l),
            0x66 => self.cb_bit(4, self.bus.read_byte(self.reg.get_hl())),
            0x67 => self.cb_bit(4, self.reg.a),
            0x68 => self.cb_bit(5, self.reg.b),
            0x69 => self.cb_bit(5, self.reg.c),
            0x6A => self.cb_bit(5, self.reg.d),
            0x6B => self.cb_bit(5, self.reg.e),
            0x6C => self.cb_bit(5, self.reg.h),
            0x6D => self.cb_bit(5, self.reg.l),
            0x6E => self.cb_bit(5, self.bus.read_byte(self.reg.get_hl())),
            0x6F => self.cb_bit(5, self.reg.a),
            0x70 => self.cb_bit(6, self.reg.b),
            0x71 => self.cb_bit(6, self.reg.c),
            0x72 => self.cb_bit(6, self.reg.d),
            0x73 => self.cb_bit(6, self.reg.e),
            0x74 => self.cb_bit(6, self.reg.h),
            0x75 => self.cb_bit(6, self.reg.l),
            0x76 => self.cb_bit(6, self.bus.read_byte(self.reg.get_hl())),
            0x77 => self.cb_bit(6, self.reg.a),
            0x78 => self.cb_bit(7, self.reg.b),
            0x79 => self.cb_bit(7, self.reg.c),
            0x7A => self.cb_bit(7, self.reg.d),
            0x7B => self.cb_bit(7, self.reg.e),
            0x7C => self.cb_bit(7, self.reg.h),
            0x7D => self.cb_bit(7, self.reg.l),
            0x7E => self.cb_bit(7, self.bus.read_byte(self.reg.get_hl())),
            0x7F => self.cb_bit(7, self.reg.a),
            0x80 => self.reg.b = self.cb_res(0, self.reg.b),
            0x81 => self.reg.c = self.cb_res(0, self.reg.c),
            0x82 => self.reg.d = self.cb_res(0, self.reg.d),
            0x83 => self.reg.e = self.cb_res(0, self.reg.e),
            0x84 => self.reg.h = self.cb_res(0, self.reg.h),
            0x85 => self.reg.l = self.cb_res(0, self.reg.l),
            0x86 => {
                let value = self.cb_res(0, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0x87 => self.reg.a = self.cb_res(0, self.reg.a),
            0x88 => self.reg.b = self.cb_res(1, self.reg.b),
            0x89 => self.reg.c = self.cb_res(1, self.reg.c),
            0x8A => self.reg.d = self.cb_res(1, self.reg.d),
            0x8B => self.reg.e = self.cb_res(1, self.reg.e),
            0x8C => self.reg.h = self.cb_res(1, self.reg.h),
            0x8D => self.reg.l = self.cb_res(1, self.reg.l),
            0x8E => {
                let value = self.cb_res(1, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0x8F => self.reg.a = self.cb_res(1, self.reg.a),
            0x90 => self.reg.b = self.cb_res(2, self.reg.b),
            0x91 => self.reg.c = self.cb_res(2, self.reg.c),
            0x92 => self.reg.d = self.cb_res(2, self.reg.d),
            0x93 => self.reg.e = self.cb_res(2, self.reg.e),
            0x94 => self.reg.h = self.cb_res(2, self.reg.h),
            0x95 => self.reg.l = self.cb_res(2, self.reg.l),
            0x96 => {
                let value = self.cb_res(2, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0x97 => self.reg.a = self.cb_res(2, self.reg.a),
            0x98 => self.reg.b = self.cb_res(3, self.reg.b),
            0x99 => self.reg.c = self.cb_res(3, self.reg.c),
            0x9A => self.reg.d = self.cb_res(3, self.reg.d),
            0x9B => self.reg.e = self.cb_res(3, self.reg.e),
            0x9C => self.reg.h = self.cb_res(3, self.reg.h),
            0x9D => self.reg.l = self.cb_res(3, self.reg.l),
            0x9E => {
                let value = self.cb_res(3, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0x9F => self.reg.a = self.cb_res(3, self.reg.a),
            0xA0 => self.reg.b = self.cb_res(4, self.reg.b),
            0xA1 => self.reg.c = self.cb_res(4, self.reg.c),
            0xA2 => self.reg.d = self.cb_res(4, self.reg.d),
            0xA3 => self.reg.e = self.cb_res(4, self.reg.e),
            0xA4 => self.reg.h = self.cb_res(4, self.reg.h),
            0xA5 => self.reg.l = self.cb_res(4, self.reg.l),
            0xA6 => {
                let value = self.cb_res(4, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0xA7 => self.reg.a = self.cb_res(4, self.reg.a),
            0xA8 => self.reg.b = self.cb_res(5, self.reg.b),
            0xA9 => self.reg.c = self.cb_res(5, self.reg.c),
            0xAA => self.reg.d = self.cb_res(5, self.reg.d),
            0xAB => self.reg.e = self.cb_res(5, self.reg.e),
            0xAC => self.reg.h = self.cb_res(5, self.reg.h),
            0xAD => self.reg.l = self.cb_res(5, self.reg.l),
            0xAE => {
                let value = self.cb_res(5, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0xAF => self.reg.a = self.cb_res(5, self.reg.a),
            0xB0 => self.reg.b = self.cb_res(6, self.reg.b),
            0xB1 => self.reg.c = self.cb_res(6, self.reg.c),
            0xB2 => self.reg.d = self.cb_res(6, self.reg.d),
            0xB3 => self.reg.e = self.cb_res(6, self.reg.e),
            0xB4 => self.reg.h = self.cb_res(6, self.reg.h),
            0xB5 => self.reg.l = self.cb_res(6, self.reg.l),
            0xB6 => {
                let value = self.cb_res(6, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0xB7 => self.reg.a = self.cb_res(6, self.reg.a),
            0xB8 => self.reg.b = self.cb_res(7, self.reg.b),
            0xB9 => self.reg.c = self.cb_res(7, self.reg.c),
            0xBA => self.reg.d = self.cb_res(7, self.reg.d),
            0xBB => self.reg.e = self.cb_res(7, self.reg.e),
            0xBC => self.reg.h = self.cb_res(7, self.reg.h),
            0xBD => self.reg.l = self.cb_res(7, self.reg.l),
            0xBE => {
                let value = self.cb_res(7, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0xBF => self.reg.a = self.cb_res(7, self.reg.a),
            0xC0 => self.reg.b = self.cb_set(0, self.reg.b),
            0xC1 => self.reg.c = self.cb_set(0, self.reg.c),
            0xC2 => self.reg.d = self.cb_set(0, self.reg.d),
            0xC3 => self.reg.e = self.cb_set(0, self.reg.e),
            0xC4 => self.reg.h = self.cb_set(0, self.reg.h),
            0xC5 => self.reg.l = self.cb_set(0, self.reg.l),
            0xC6 => {
                let value = self.cb_set(0, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0xC7 => self.reg.a = self.cb_set(0, self.reg.a),
            0xC8 => self.reg.b = self.cb_set(1, self.reg.b),
            0xC9 => self.reg.c = self.cb_set(1, self.reg.c),
            0xCA => self.reg.d = self.cb_set(1, self.reg.d),
            0xCB => self.reg.e = self.cb_set(1, self.reg.e),
            0xCC => self.reg.h = self.cb_set(1, self.reg.h),
            0xCD => self.reg.l = self.cb_set(1, self.reg.l),
            0xCE => {
                let value = self.cb_set(1, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0xCF => self.reg.a = self.cb_set(1, self.reg.a),
            0xD0 => self.reg.b = self.cb_set(2, self.reg.b),
            0xD1 => self.reg.c = self.cb_set(2, self.reg.c),
            0xD2 => self.reg.d = self.cb_set(2, self.reg.d),
            0xD3 => self.reg.e = self.cb_set(2, self.reg.e),
            0xD4 => self.reg.h = self.cb_set(2, self.reg.h),
            0xD5 => self.reg.l = self.cb_set(2, self.reg.l),
            0xD6 => {
                let value = self.cb_set(2, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0xD7 => self.reg.a = self.cb_set(2, self.reg.a),
            0xD8 => self.reg.b = self.cb_set(3, self.reg.b),
            0xD9 => self.reg.c = self.cb_set(3, self.reg.c),
            0xDA => self.reg.d = self.cb_set(3, self.reg.d),
            0xDB => self.reg.e = self.cb_set(3, self.reg.e),
            0xDC => self.reg.h = self.cb_set(3, self.reg.h),
            0xDD => self.reg.l = self.cb_set(3, self.reg.l),
            0xDE => {
                let value = self.cb_set(3, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0xDF => self.reg.a = self.cb_set(3, self.reg.a),
            0xE0 => self.reg.b = self.cb_set(4, self.reg.b),
            0xE1 => self.reg.c = self.cb_set(4, self.reg.c),
            0xE2 => self.reg.d = self.cb_set(4, self.reg.d),
            0xE3 => self.reg.e = self.cb_set(4, self.reg.e),
            0xE4 => self.reg.h = self.cb_set(4, self.reg.h),
            0xE5 => self.reg.l = self.cb_set(4, self.reg.l),
            0xE6 => {
                let value = self.cb_set(4, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0xE7 => self.reg.a = self.cb_set(4, self.reg.a),
            0xE8 => self.reg.b = self.cb_set(5, self.reg.b),
            0xE9 => self.reg.c = self.cb_set(5, self.reg.c),
            0xEA => self.reg.d = self.cb_set(5, self.reg.d),
            0xEB => self.reg.e = self.cb_set(5, self.reg.e),
            0xEC => self.reg.h = self.cb_set(5, self.reg.h),
            0xED => self.reg.l = self.cb_set(5, self.reg.l),
            0xEE => {
                let value = self.cb_set(5, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0xEF => self.reg.a = self.cb_set(5, self.reg.a),
            0xF0 => self.reg.b = self.cb_set(6, self.reg.b),
            0xF1 => self.reg.c = self.cb_set(6, self.reg.c),
            0xF2 => self.reg.d = self.cb_set(6, self.reg.d),
            0xF3 => self.reg.e = self.cb_set(6, self.reg.e),
            0xF4 => self.reg.h = self.cb_set(6, self.reg.h),
            0xF5 => self.reg.l = self.cb_set(6, self.reg.l),
            0xF6 => {
                let value = self.cb_set(6, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0xF7 => self.reg.a = self.cb_set(6, self.reg.a),
            0xF8 => self.reg.b = self.cb_set(7, self.reg.b),
            0xF9 => self.reg.c = self.cb_set(7, self.reg.c),
            0xFA => self.reg.d = self.cb_set(7, self.reg.d),
            0xFB => self.reg.e = self.cb_set(7, self.reg.e),
            0xFC => self.reg.h = self.cb_set(7, self.reg.h),
            0xFD => self.reg.l = self.cb_set(7, self.reg.l),
            0xFE => {
                let value = self.cb_set(7, self.bus.read_byte(self.reg.get_hl()));
                self.bus.write_byte(self.reg.get_hl(), value);
            }
            0xFF => self.reg.a = self.cb_set(7, self.reg.a),
        }
    }

    // call address if condition is met
    fn call_z(&mut self) {
        if self.flag_is_active(Flags::Zero) {
            self.m = 6;
            self.push_stack(self.reg.pc.wrapping_add(2));
            self.reg.pc = self.read_word();
        } else {
            self.m = 3;
            self.reg.pc = self.reg.pc.wrapping_add(2);
        }
    }

    // push address of instruction on the stack
    fn call(&mut self) {
        self.m = 6;
        self.push_stack(self.reg.pc.wrapping_add(2));
        self.reg.pc = self.read_word();
    }

    // add 8-bit immediate and carry flag to register A
    fn adc_a(&mut self) {
        self.m = 2;

        let carry = if self.flag_is_active(Flags::Carry) {
            1
        } else {
            0
        };
        let value = self.read_byte();
        let result = self.reg.a.wrapping_add(value).wrapping_add(carry);
        self.unset_flag(Flags::Negative);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.set_flag_on_if(
            Flags::HalfCarry,
            (self.reg.a & 0xF) + (value & 0xF) + carry > 0xF,
        );
        self.set_flag_on_if(
            Flags::Carry,
            self.reg.a as u16 + value as u16 + carry as u16 > 0xFF,
        );
        self.reg.a = result;
    }

    // call address
    fn rst_one(&mut self) {
        self.m = 4;

        self.push_stack(self.reg.pc);
        self.reg.pc = 0x08;
    }

    // return from subroutine if condition is met
    fn ret_nc(&mut self) {
        if !self.flag_is_active(Flags::Carry) {
            self.m = 5;
            self.reg.pc = self.pop_stack();
        } else {
            self.m = 2;
        }
    }

    // pop contents from memory stack onto register pair DE
    fn pop_de(&mut self) {
        self.m = 3;

        let value = self.pop_stack();
        self.reg.set_de(value);
    }

    // jump to address if condition is met
    fn jp_nc(&mut self) {
        if !self.flag_is_active(Flags::Carry) {
            self.m = 4;
            self.reg.pc = self.read_word();
        } else {
            self.m = 3;
            self.reg.pc = self.reg.pc.wrapping_add(2);
        }
    }

    // call address if condition is met
    fn call_nc(&mut self) {
        if !self.flag_is_active(Flags::Carry) {
            self.m = 6;
            self.push_stack(self.reg.pc.wrapping_add(2));
            self.reg.pc = self.read_word();
        } else {
            self.m = 3;
            self.reg.pc = self.reg.pc.wrapping_add(2);
        }
    }

    // push contents of register pair DE onto the memeory stack
    fn push_de(&mut self) {
        self.m = 4;

        self.push_stack(self.reg.get_de());
    }

    // subtract 8-bit immediate from contents of register A
    fn sub_imm(&mut self) {
        self.m = 2;

        let value = self.read_byte();
        let result = self.reg.a.wrapping_sub(value);
        self.set_flag(Flags::Negative);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.set_flag_on_if(Flags::HalfCarry, (self.reg.a & 0xF) < (value & 0xF));
        self.set_flag_on_if(Flags::Carry, self.reg.a < value);
        self.reg.a = result;
    }

    // call address
    fn rst_two(&mut self) {
        self.m = 4;

        self.push_stack(self.reg.pc);
        self.reg.pc = 0x10;
    }

    // return from subroutine if condition is met
    fn ret_c(&mut self) {
        if self.flag_is_active(Flags::Carry) {
            self.m = 5;
            self.reg.pc = self.pop_stack();
        } else {
            self.m = 2;
        }
    }

    // return from subroutine and enable interrupts
    fn reti(&mut self) {
        self.m = 4;

        self.reg.pc = self.pop_stack();
        self.ime = true;
    }

    // jump to address if condition is met
    fn jp_c(&mut self) {
        if self.flag_is_active(Flags::Carry) {
            self.m = 4;
            self.reg.pc = self.read_word();
        } else {
            self.m = 3;
            self.reg.pc = self.reg.pc.wrapping_add(2);
        }
    }

    // call address if condition is met
    fn call_c(&mut self) {
        if self.flag_is_active(Flags::Carry) {
            self.m = 6;
            self.push_stack(self.reg.pc.wrapping_add(2));
            self.reg.pc = self.read_word();
        } else {
            self.m = 3;
            self.reg.pc = self.reg.pc.wrapping_add(2);
        }
    }

    // subtract contents of 8-bit immediate and carry flag from register A
    fn sbc_a(&mut self) {
        self.m = 2;

        let carry = if self.flag_is_active(Flags::Carry) {
            1
        } else {
            0
        };
        let value = self.read_byte();
        let result = self.reg.a.wrapping_sub(value).wrapping_sub(carry);
        self.set_flag(Flags::Negative);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.set_flag_on_if(Flags::HalfCarry, (self.reg.a & 0xF) < (value & 0xF) + carry);
        self.set_flag_on_if(
            Flags::Carry,
            (self.reg.a as u16) < (value as u16) + (carry as u16),
        );
        self.reg.a = result;
    }

    // call adress
    fn rst_three(&mut self) {
        self.m = 4;

        self.push_stack(self.reg.pc);
        self.reg.pc = 0x18;
    }

    // store contents of register A in internal ram, port register or mode register
    fn ld_addr_a(&mut self) {
        self.m = 3;

        let value = self.read_byte();
        self.bus.write_byte(0xFF00 | value as u16, self.reg.a);
    }

    // pop contents from memory stack into register pair HL
    fn pop_hl(&mut self) {
        self.m = 3;

        let value = self.pop_stack();
        self.reg.set_hl(value);
    }

    // store contents of register A in the internal ram, port register or mode register
    fn ld_addr_c_a(&mut self) {
        self.m = 2;
        self.bus.write_byte(0xFF00 | self.reg.c as u16, self.reg.a);
    }

    // push contents of register pair HL onto the memory stack
    fn push_hl(&mut self) {
        self.m = 4;

        self.push_stack(self.reg.get_hl());
    }

    // bitwise AND value with register A
    fn and_a(&mut self) {
        self.m = 2;

        let result = self.reg.a & self.read_byte();
        self.unset_flag(Flags::Negative);
        self.unset_flag(Flags::Carry);
        self.set_flag(Flags::HalfCarry);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.reg.a = result;
    }

    // call address
    fn rst_four(&mut self) {
        self.m = 4;

        self.push_stack(self.reg.pc);
        self.reg.pc = 0x20;
    }

    // Add contents of 2's complement immediate operand to the sp
    fn add_sp(&mut self) {
        self.m = 4;

        self.reg.sp = self.add16_imm(self.reg.sp);
    }

    // load contents of register pair HL into the pc
    fn jp_hl(&mut self) {
        self.m = 1;

        self.reg.pc = self.reg.get_hl();
    }

    // store contents of register A in the internal ram
    // or register specifed by the 16-bit immediate
    fn ld_addr_a16_a(&mut self) {
        self.m = 4;

        let address = self.read_word();
        self.bus.write_byte(address, self.reg.a);
    }

    // bitwise xor a and 8-bit immediate operand
    fn xor_d8(&mut self) {
        self.m = 2;

        let value = self.read_byte();
        let result = self.reg.a ^ value;
        self.unset_flag(Flags::Negative);
        self.unset_flag(Flags::HalfCarry);
        self.unset_flag(Flags::Carry);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.reg.a = result;
    }

    // call adress
    fn rst_five(&mut self) {
        self.m = 4;

        self.push_stack(self.reg.pc);
        self.reg.pc = 0x28;
    }

    // load into register A the contents of the internal ram, port register or mode register
    fn ld_a_a8(&mut self) {
        self.m = 3;

        let value = 0xFF00 | self.read_byte() as u16;
        self.reg.a = self.bus.read_byte(value);
    }

    // pop contents of the memory stack into register pair AF
    fn pop_af(&mut self) {
        self.m = 3;

        let value = self.pop_stack();
        self.reg.set_af(value & 0xFFF0);
    }

    // load into register A the contents of internal ram, port register or mode register
    fn ld_a_c_addr(&mut self) {
        self.m = 2;
        self.reg.a = self.bus.read_byte(0xFF00 | self.reg.c as u16);
    }

    // reset interrupt master enable(IME) flag and prohibit maskable interrupts
    fn di(&mut self) {
        self.m = 1;
        self.ime = false;
    }

    // push contents of register pair AF onto the memory stack
    fn push_af(&mut self) {
        self.m = 4;

        self.push_stack(self.reg.get_af());
    }

    // store bitwise OR of 8-bit immediate operand and register A
    fn or_d8(&mut self) {
        self.m = 2;

        let result = self.reg.a | self.read_byte();
        self.unset_flag(Flags::Negative);
        self.unset_flag(Flags::HalfCarry);
        self.unset_flag(Flags::Carry);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.reg.a = result;
    }

    // call adress
    fn rst_six(&mut self) {
        self.m = 4;

        self.push_stack(self.reg.pc);
        self.reg.pc = 0x30;
    }

    // add 8-bit signed to sp and store in register pair HL
    fn ld_hl_sp_s8(&mut self) {
        self.m = 3;

        let value = self.add16_imm(self.reg.sp);
        // Note: This instruction doesn't modify SP, it just stores the result in HL
        self.reg.set_hl(value);
    }

    // load contents of register pair HL into sp
    fn ld_sp_hl(&mut self) {
        self.m = 2;

        self.reg.sp = self.reg.get_hl();
    }

    // load contents of internal ram or register specified
    // by 16-bit immediate operand into register A
    fn ld_a_a16(&mut self) {
        self.m = 4;

        let value = self.read_word();
        self.reg.a = self.bus.read_byte(value);
    }

    // set the interrupt master enable(IME) flag and
    // enable maskable interrupts
    fn ei(&mut self) {
        self.m = 1;
        self.enable_ime_next = true;
    }

    // compare contents of register A and 8-bit immediate operand
    fn cp_d8(&mut self) {
        self.m = 2;

        let value = self.read_byte();
        let result = self.reg.a.wrapping_sub(value);
        self.set_flag(Flags::Negative);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.set_flag_on_if(Flags::HalfCarry, (self.reg.a & 0xF) < (value & 0xF));
        self.set_flag_on_if(Flags::Carry, self.reg.a < value);
    }

    // call address
    fn rst_seven(&mut self) {
        self.m = 4;

        self.push_stack(self.reg.pc);
        self.reg.pc = 0x38;
    }

    fn decode_execute_opcode(&mut self, opcode: u8) {
        match opcode {
            0x00 => self.nop(),
            0x01 => self.load_bc(),
            0x02 => self.load_bc_a(),
            0x03 => self.inc_bc(),
            0x04 => self.inc_b(),
            0x05 => self.dec_b(),
            0x06 => self.load_b(),
            0x07 => self.rlca(),
            0x08 => self.load_sp_at_addr(),
            0x09 => self.add_hl_bc(),
            0x0A => self.ld_a_bc(),
            0x0B => self.dec_bc(),
            0x0C => self.inc_c(),
            0x0D => self.dec_c(),
            0x0E => self.ld_c(),
            0x0F => self.rrca(),
            0x10 => self.stop(),
            0x11 => self.ld_de(),
            0x12 => self.ld_de_a(),
            0x13 => self.inc_de(),
            0x14 => self.inc_d(),
            0x15 => self.dec_d(),
            0x16 => self.ld_d(),
            0x17 => self.rla(),
            0x18 => self.jr(),
            0x19 => self.add_hl_de(),
            0x1A => self.ld_a_de(),
            0x1B => self.dec_de(),
            0x1C => self.inc_e(),
            0x1D => self.dec_e(),
            0x1E => self.ld_e(),
            0x1F => self.rra(),
            0x20 => self.jr_nz(),
            0x21 => self.ld_hl(),
            0x22 => self.ld_hl_inc_a(),
            0x23 => self.inc_hl(),
            0x24 => self.inc_h(),
            0x25 => self.dec_h(),
            0x26 => self.ld_h(),
            0x27 => self.daa(),
            0x28 => self.jr_z(),
            0x29 => self.add_hl_hl(),
            0x2A => self.ld_a_hl_plus(),
            0x2B => self.dec_hl(),
            0x2C => self.inc_l(),
            0x2D => self.dec_l(),
            0x2E => self.ld_l(),
            0x2F => self.cpl(),
            0x30 => self.jr_nc(),
            0x31 => self.ld_sp(),
            0x32 => self.ld_hlm_a(),
            0x33 => self.inc_sp(),
            0x34 => self.inc_content_at_hl(),
            0x35 => self.dec_content_at_hl(),
            0x36 => self.ld_hl_byte(),
            0x37 => self.scf(),
            0x38 => self.jr_c(),
            0x39 => self.add_hl_sp(),
            0x3A => self.ld_a_hl_dec(),
            0x3B => self.dec_sp(),
            0x3C => self.inc_a(),
            0x3D => self.dec_a(),
            0x3E => self.ld_a_byte(),
            0x3F => self.ccf(),
            0x40 => self.ld_b_b(),
            0x41 => self.ld_b_c(),
            0x42 => self.ld_b_d(),
            0x43 => self.ld_b_e(),
            0x44 => self.ld_b_h(),
            0x45 => self.ld_b_l(),
            0x46 => self.ld_b_hl(),
            0x47 => self.ld_b_a(),
            0x48 => self.ld_c_b(),
            0x49 => self.ld_c_c(),
            0x4A => self.ld_c_d(),
            0x4B => self.ld_c_e(),
            0x4C => self.ld_c_h(),
            0x4D => self.ld_c_l(),
            0x4E => self.ld_c_hl(),
            0x4F => self.ld_c_a(),
            0x50 => self.ld_d_b(),
            0x51 => self.ld_d_c(),
            0x52 => self.ld_d_d(),
            0x53 => self.ld_d_e(),
            0x54 => self.ld_d_h(),
            0x55 => self.ld_d_l(),
            0x56 => self.ld_d_hl(),
            0x57 => self.ld_d_a(),
            0x58 => self.ld_e_b(),
            0x59 => self.ld_e_c(),
            0x5A => self.ld_e_d(),
            0x5B => self.ld_e_e(),
            0x5C => self.ld_e_h(),
            0x5D => self.ld_e_l(),
            0x5E => self.ld_e_hl(),
            0x5F => self.ld_e_a(),
            0x60 => self.ld_h_b(),
            0x61 => self.ld_h_c(),
            0x62 => self.ld_h_d(),
            0x63 => self.ld_h_e(),
            0x64 => self.ld_h_h(),
            0x65 => self.ld_h_l(),
            0x66 => self.ld_h_hl(),
            0x67 => self.ld_h_a(),
            0x68 => self.ld_l_b(),
            0x69 => self.ld_l_c(),
            0x6A => self.ld_l_d(),
            0x6B => self.ld_l_e(),
            0x6C => self.ld_l_h(),
            0x6D => self.ld_l_l(),
            0x6E => self.ld_l_hl(),
            0x6F => self.ld_l_a(),
            0x70 => self.ld_hl_b(),
            0x71 => self.ld_hl_c(),
            0x72 => self.ld_hl_d(),
            0x73 => self.ld_hl_e(),
            0x74 => self.ld_hl_h(),
            0x75 => self.ld_hl_l(),
            0x76 => self.halt(),
            0x77 => self.ld_hl_a(),
            0x78 => self.ld_a_b(),
            0x79 => self.ld_a_c(),
            0x7A => self.ld_a_d(),
            0x7B => self.ld_a_e(),
            0x7C => self.ld_a_h(),
            0x7D => self.ld_a_l(),
            0x7E => self.ld_a_hl(),
            0x7F => self.ld_a_a(),
            0x80 => self.add_a_b(),
            0x81 => self.add_a_c(),
            0x82 => self.add_a_d(),
            0x83 => self.add_a_e(),
            0x84 => self.add_a_h(),
            0x85 => self.add_a_l(),
            0x86 => self.add_a_hl(),
            0x87 => self.add_a_a(),
            0x88 => self.adc_a_b(),
            0x89 => self.adc_a_c(),
            0x8A => self.adc_a_d(),
            0x8B => self.adc_a_e(),
            0x8C => self.adc_a_h(),
            0x8D => self.adc_a_l(),
            0x8E => self.adc_a_hl(),
            0x8F => self.adc_a_a(),
            0x90 => self.sub_b(),
            0x91 => self.sub_c(),
            0x92 => self.sub_d(),
            0x93 => self.sub_e(),
            0x94 => self.sub_h(),
            0x95 => self.sub_l(),
            0x96 => self.sub_hl(),
            0x97 => self.sub_a(),
            0x98 => self.sbc_a_b(),
            0x99 => self.sbc_a_c(),
            0x9A => self.sbc_a_d(),
            0x9B => self.sbc_a_e(),
            0x9C => self.sbc_a_h(),
            0x9D => self.sbc_a_l(),
            0x9E => self.sbc_a_hl(),
            0x9F => self.sbc_a_a(),
            0xA0 => self.and_a_b(),
            0xA1 => self.and_a_c(),
            0xA2 => self.and_a_d(),
            0xA3 => self.and_a_e(),
            0xA4 => self.and_a_h(),
            0xA5 => self.and_a_l(),
            0xA6 => self.and_a_hl(),
            0xA7 => self.and_a_a(),
            0xA8 => self.xor_a_b(),
            0xA9 => self.xor_a_c(),
            0xAA => self.xor_a_d(),
            0xAB => self.xor_a_e(),
            0xAC => self.xor_a_h(),
            0xAD => self.xor_a_l(),
            0xAE => self.xor_a_hl(),
            0xAF => self.xor_a_a(),
            0xB0 => self.or_a_b(),
            0xB1 => self.or_a_c(),
            0xB2 => self.or_a_d(),
            0xB3 => self.or_a_e(),
            0xB4 => self.or_a_h(),
            0xB5 => self.or_a_l(),
            0xB6 => self.or_a_hl(),
            0xB7 => self.or_a_a(),
            0xB8 => self.cp_a_b(),
            0xB9 => self.cp_a_c(),
            0xBA => self.cp_a_d(),
            0xBB => self.cp_a_e(),
            0xBC => self.cp_a_h(),
            0xBD => self.cp_a_l(),
            0xBE => self.cp_a_hl(),
            0xBF => self.cp_a_a(),
            0xC0 => self.ret_nz(),
            0xC1 => self.pop_bc(),
            0xC2 => self.jp_nz(),
            0xC3 => self.jp(),
            0xC4 => self.call_nz(),
            0xC5 => self.push_bc(),
            0xC6 => self.add_a_byte(),
            0xC7 => self.rst_zero(),
            0xC8 => self.ret_z(),
            0xC9 => self.ret(),
            0xCA => self.jp_z(),
            0xCB => self.call_cb(),
            0xCC => self.call_z(),
            0xCD => self.call(),
            0xCE => self.adc_a(),
            0xCF => self.rst_one(),
            0xD0 => self.ret_nc(),
            0xD1 => self.pop_de(),
            0xD2 => self.jp_nc(),
            0xD3 => self.nop(), // Undefined opcode
            0xD4 => self.call_nc(),
            0xD5 => self.push_de(),
            0xD6 => self.sub_imm(),
            0xD7 => self.rst_two(),
            0xD8 => self.ret_c(),
            0xD9 => self.reti(),
            0xDA => self.jp_c(),
            0xDB => self.nop(), // Undefined opcode
            0xDC => self.call_c(),
            0xDD => self.nop(), // Undefined opcode
            0xDE => self.sbc_a(),
            0xDF => self.rst_three(),
            0xE0 => self.ld_addr_a(),
            0xE1 => self.pop_hl(),
            0xE2 => self.ld_addr_c_a(),
            0xE3 => self.nop(), // Undefined opcode
            0xE4 => self.nop(), // Undefined opcode
            0xE5 => self.push_hl(),
            0xE6 => self.and_a(),
            0xE7 => self.rst_four(),
            0xE8 => self.add_sp(),
            0xE9 => self.jp_hl(),
            0xEA => self.ld_addr_a16_a(),
            0xEB => self.nop(), // Undefined opcode
            0xEC => self.nop(), // Undefined opcode
            0xED => self.nop(), // Undefined opcode
            0xEE => self.xor_d8(),
            0xEF => self.rst_five(),
            0xF0 => self.ld_a_a8(),
            0xF1 => self.pop_af(),
            0xF2 => self.ld_a_c_addr(),
            0xF3 => self.di(),
            0xF5 => self.push_af(),
            0xF6 => self.or_d8(),
            0xF7 => self.rst_six(),
            0xF8 => self.ld_hl_sp_s8(),
            0xF9 => self.ld_sp_hl(),
            0xFA => self.ld_a_a16(),
            0xFB => self.ei(),
            0xFC => self.nop(), // Undefined opcode
            0xFD => self.nop(), // Undefined opcode
            0xFE => self.cp_d8(),
            0xFF => self.rst_seven(),
            _ => println!("{opcode:#X} is not a recognized opcode..."),
        }
    }

    fn decode_execute(&mut self) {
        let opcode = self.read_byte();
        match opcode {
            0x00 => self.nop(),
            0x01 => self.load_bc(),
            0x02 => self.load_bc_a(),
            0x03 => self.inc_bc(),
            0x04 => self.inc_b(),
            0x05 => self.dec_b(),
            0x06 => self.load_b(),
            0x07 => self.rlca(),
            0x08 => self.load_sp_at_addr(),
            0x09 => self.add_hl_bc(),
            0x0A => self.ld_a_bc(),
            0x0B => self.dec_bc(),
            0x0C => self.inc_c(),
            0x0D => self.dec_c(),
            0x0E => self.ld_c(),
            0x0F => self.rrca(),
            0x10 => self.stop(),
            0x11 => self.ld_de(),
            0x12 => self.ld_de_a(),
            0x13 => self.inc_de(),
            0x14 => self.inc_d(),
            0x15 => self.dec_d(),
            0x16 => self.ld_d(),
            0x17 => self.rla(),
            0x18 => self.jr(),
            0x19 => self.add_hl_de(),
            0x1A => self.ld_a_de(),
            0x1B => self.dec_de(),
            0x1C => self.inc_e(),
            0x1D => self.dec_e(),
            0x1E => self.ld_e(),
            0x1F => self.rra(),
            0x20 => self.jr_nz(),
            0x21 => self.ld_hl(),
            0x22 => self.ld_hl_inc_a(),
            0x23 => self.inc_hl(),
            0x24 => self.inc_h(),
            0x25 => self.dec_h(),
            0x26 => self.ld_h(),
            0x27 => self.daa(),
            0x28 => self.jr_z(),
            0x29 => self.add_hl_hl(),
            0x2A => self.ld_a_hl_plus(),
            0x2B => self.dec_hl(),
            0x2C => self.inc_l(),
            0x2D => self.dec_l(),
            0x2E => self.ld_l(),
            0x2F => self.cpl(),
            0x30 => self.jr_nc(),
            0x31 => self.ld_sp(),
            0x32 => self.ld_hlm_a(),
            0x33 => self.inc_sp(),
            0x34 => self.inc_content_at_hl(),
            0x35 => self.dec_content_at_hl(),
            0x36 => self.ld_hl_byte(),
            0x37 => self.scf(),
            0x38 => self.jr_c(),
            0x39 => self.add_hl_sp(),
            0x3A => self.ld_a_hl_dec(),
            0x3B => self.dec_sp(),
            0x3C => self.inc_a(),
            0x3D => self.dec_a(),
            0x3E => self.ld_a_byte(),
            0x3F => self.ccf(),
            0x40 => self.ld_b_b(),
            0x41 => self.ld_b_c(),
            0x42 => self.ld_b_d(),
            0x43 => self.ld_b_e(),
            0x44 => self.ld_b_h(),
            0x45 => self.ld_b_l(),
            0x46 => self.ld_b_hl(),
            0x47 => self.ld_b_a(),
            0x48 => self.ld_c_b(),
            0x49 => self.ld_c_c(),
            0x4A => self.ld_c_d(),
            0x4B => self.ld_c_e(),
            0x4C => self.ld_c_h(),
            0x4D => self.ld_c_l(),
            0x4E => self.ld_c_hl(),
            0x4F => self.ld_c_a(),
            0x50 => self.ld_d_b(),
            0x51 => self.ld_d_c(),
            0x52 => self.ld_d_d(),
            0x53 => self.ld_d_e(),
            0x54 => self.ld_d_h(),
            0x55 => self.ld_d_l(),
            0x56 => self.ld_d_hl(),
            0x57 => self.ld_d_a(),
            0x58 => self.ld_e_b(),
            0x59 => self.ld_e_c(),
            0x5A => self.ld_e_d(),
            0x5B => self.ld_e_e(),
            0x5C => self.ld_e_h(),
            0x5D => self.ld_e_l(),
            0x5E => self.ld_e_hl(),
            0x5F => self.ld_e_a(),
            0x60 => self.ld_h_b(),
            0x61 => self.ld_h_c(),
            0x62 => self.ld_h_d(),
            0x63 => self.ld_h_e(),
            0x64 => self.ld_h_h(),
            0x65 => self.ld_h_l(),
            0x66 => self.ld_h_hl(),
            0x67 => self.ld_h_a(),
            0x68 => self.ld_l_b(),
            0x69 => self.ld_l_c(),
            0x6A => self.ld_l_d(),
            0x6B => self.ld_l_e(),
            0x6C => self.ld_l_h(),
            0x6D => self.ld_l_l(),
            0x6E => self.ld_l_hl(),
            0x6F => self.ld_l_a(),
            0x70 => self.ld_hl_b(),
            0x71 => self.ld_hl_c(),
            0x72 => self.ld_hl_d(),
            0x73 => self.ld_hl_e(),
            0x74 => self.ld_hl_h(),
            0x75 => self.ld_hl_l(),
            0x76 => self.halt(),
            0x77 => self.ld_hl_a(),
            0x78 => self.ld_a_b(),
            0x79 => self.ld_a_c(),
            0x7A => self.ld_a_d(),
            0x7B => self.ld_a_e(),
            0x7C => self.ld_a_h(),
            0x7D => self.ld_a_l(),
            0x7E => self.ld_a_hl(),
            0x7F => self.ld_a_a(),
            0x80 => self.add_a_b(),
            0x81 => self.add_a_c(),
            0x82 => self.add_a_d(),
            0x83 => self.add_a_e(),
            0x84 => self.add_a_h(),
            0x85 => self.add_a_l(),
            0x86 => self.add_a_hl(),
            0x87 => self.add_a_a(),
            0x88 => self.adc_a_b(),
            0x89 => self.adc_a_c(),
            0x8A => self.adc_a_d(),
            0x8B => self.adc_a_e(),
            0x8C => self.adc_a_h(),
            0x8D => self.adc_a_l(),
            0x8E => self.adc_a_hl(),
            0x8F => self.adc_a_a(),
            0x90 => self.sub_b(),
            0x91 => self.sub_c(),
            0x92 => self.sub_d(),
            0x93 => self.sub_e(),
            0x94 => self.sub_h(),
            0x95 => self.sub_l(),
            0x96 => self.sub_hl(),
            0x97 => self.sub_a(),
            0x98 => self.sbc_a_b(),
            0x99 => self.sbc_a_c(),
            0x9A => self.sbc_a_d(),
            0x9B => self.sbc_a_e(),
            0x9C => self.sbc_a_h(),
            0x9D => self.sbc_a_l(),
            0x9E => self.sbc_a_hl(),
            0x9F => self.sbc_a_a(),
            0xA0 => self.and_a_b(),
            0xA1 => self.and_a_c(),
            0xA2 => self.and_a_d(),
            0xA3 => self.and_a_e(),
            0xA4 => self.and_a_h(),
            0xA5 => self.and_a_l(),
            0xA6 => self.and_a_hl(),
            0xA7 => self.and_a_a(),
            0xA8 => self.xor_a_b(),
            0xA9 => self.xor_a_c(),
            0xAA => self.xor_a_d(),
            0xAB => self.xor_a_e(),
            0xAC => self.xor_a_h(),
            0xAD => self.xor_a_l(),
            0xAE => self.xor_a_hl(),
            0xAF => self.xor_a_a(),
            0xB0 => self.or_a_b(),
            0xB1 => self.or_a_c(),
            0xB2 => self.or_a_d(),
            0xB3 => self.or_a_e(),
            0xB4 => self.or_a_h(),
            0xB5 => self.or_a_l(),
            0xB6 => self.or_a_hl(),
            0xB7 => self.or_a_a(),
            0xB8 => self.cp_a_b(),
            0xB9 => self.cp_a_c(),
            0xBA => self.cp_a_d(),
            0xBB => self.cp_a_e(),
            0xBC => self.cp_a_h(),
            0xBD => self.cp_a_l(),
            0xBE => self.cp_a_hl(),
            0xBF => self.cp_a_a(),
            0xC0 => self.ret_nz(),
            0xC1 => self.pop_bc(),
            0xC2 => self.jp_nz(),
            0xC3 => self.jp(),
            0xC4 => self.call_nz(),
            0xC5 => self.push_bc(),
            0xC6 => self.add_a_byte(),
            0xC7 => self.rst_zero(),
            0xC8 => self.ret_z(),
            0xC9 => self.ret(),
            0xCA => self.jp_z(),
            0xCB => self.call_cb(),
            0xCC => self.call_z(),
            0xCD => self.call(),
            0xCE => self.adc_a(),
            0xCF => self.rst_one(),
            0xD0 => self.ret_nc(),
            0xD1 => self.pop_de(),
            0xD2 => self.jp_nc(),
            0xD3 => self.nop(), // Undefined opcode
            0xD4 => self.call_nc(),
            0xD5 => self.push_de(),
            0xD6 => self.sub_imm(),
            0xD7 => self.rst_two(),
            0xD8 => self.ret_c(),
            0xD9 => self.reti(),
            0xDA => self.jp_c(),
            0xDB => self.nop(), // Undefined opcode
            0xDC => self.call_c(),
            0xDD => self.nop(), // Undefined opcode
            0xDE => self.sbc_a(),
            0xDF => self.rst_three(),
            0xE0 => self.ld_addr_a(),
            0xE1 => self.pop_hl(),
            0xE2 => self.ld_addr_c_a(),
            0xE3 => self.nop(), // Undefined opcode
            0xE4 => self.nop(), // Undefined opcode
            0xE5 => self.push_hl(),
            0xE6 => self.and_a(),
            0xE7 => self.rst_four(),
            0xE8 => self.add_sp(),
            0xE9 => self.jp_hl(),
            0xEA => self.ld_addr_a16_a(),
            0xEB => self.nop(), // Undefined opcode
            0xEC => self.nop(), // Undefined opcode
            0xED => self.nop(), // Undefined opcode
            0xEE => self.xor_d8(),
            0xEF => self.rst_five(),
            0xF0 => self.ld_a_a8(),
            0xF1 => self.pop_af(),
            0xF2 => self.ld_a_c_addr(),
            0xF3 => self.di(),
            0xF5 => self.push_af(),
            0xF6 => self.or_d8(),
            0xF7 => self.rst_six(),
            0xF8 => self.ld_hl_sp_s8(),
            0xF9 => self.ld_sp_hl(),
            0xFA => self.ld_a_a16(),
            0xFB => self.ei(),
            0xFC => self.nop(), // Undefined opcode
            0xFD => self.nop(), // Undefined opcode
            0xFE => self.cp_d8(),
            0xFF => self.rst_seven(),
            _ => println!("{opcode:#X} is not a recognized opcode..."),
        }
    }

    pub fn run_cycle(&mut self) -> u8 {
        // handle delayed EI
        if self.enable_ime_next {
            self.ime = true;
            self.enable_ime_next = false;
        }

        if self.halted {
            self.m = 1;
            self.bus.if_ = self.bus.ppu.update_ly(self.m);
            self.bus.timer.update(self.m);

            if self.bus.timer.interrupt {
                self.bus.if_ |= 0x04;
                self.bus.timer.interrupt = false;
            }

            let ie = self.bus.read_byte(0xFFFF);
            let iflag = self.bus.read_byte(0xFF0F);
            let pending = ie & iflag;

            if pending != 0 {
                self.halted = false;
                if self.ime {
                    self.handle_interrupts(true);
                }
            }
            return self.m;
        }

        if self.stopped {
            // STOP mode continues until an interrupt occurs
            // For now, we'll simulate a joypad interrupt after some time
            static mut STOP_CYCLES: u32 = 0;
            unsafe {
                STOP_CYCLES += self.m as u32;
                if STOP_CYCLES >= 456 {
                    // Simulate after one scanline
                    self.bus.if_ |= 0x10; // Set Joypad interrupt
                    self.stopped = false;
                    STOP_CYCLES = 0;
                }
            }
            return self.m;
        }

        if self.ime && self.handle_interrupts(true) {
            // interrupt serviced, do not execute instructions this cycle
            return self.m;
        }

        if !self.halted {
            self.decode_execute();
            //self.print_register_data();

            self.bus.if_ = self.bus.ppu.update_ly(self.m);
            self.bus.timer.update(self.m);

            if self.bus.timer.interrupt {
                self.bus.if_ |= 0x04;
                self.bus.timer.interrupt = false;
            }

            if self.bus.serial_mut().step(self.m) {
                self.bus.if_ |= 0x08;
            }
        }
        self.m
    }

    fn halt(&mut self) {
        self.m = 1;
        self.halted = true;
    }

    fn ld_a_b(&mut self) {
        self.m = 1;
        self.reg.a = self.reg.b;
    }
    fn ld_a_c(&mut self) {
        self.m = 1;
        self.reg.a = self.reg.c;
    }
    fn ld_a_d(&mut self) {
        self.m = 1;
        self.reg.a = self.reg.d;
    }
    fn ld_a_e(&mut self) {
        self.m = 1;
        self.reg.a = self.reg.e;
    }
    fn ld_a_h(&mut self) {
        self.m = 1;
        self.reg.a = self.reg.h;
    }
    fn ld_a_l(&mut self) {
        self.m = 1;
        self.reg.a = self.reg.l;
    }
    fn ld_a_hl(&mut self) {
        self.m = 2;
        let addr = self.reg.get_hl();
        self.reg.a = self.bus.read_byte(addr);
    }
    fn ld_a_a(&mut self) {
        self.m = 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_correct_resetting_of_flags() {
        let mut cpu = Cpu::new(Path::new(
            "gb-test-roms/cpu_instrs/individual/01-special.gb",
        ));
        cpu.reset_flags();
        assert_eq!(0, cpu.reg.f);
    }

    #[test]
    fn test_correct_setting_of_flag() {
        let mut cpu = Cpu::new(Path::new(
            "gb-test-roms/cpu_instrs/individual/01-special.gb",
        ));
        cpu.reset_flags();
        cpu.set_flag(Flags::Carry);
        assert_eq!(0x10, cpu.reg.f);
        cpu.set_flag(Flags::HalfCarry);
        assert_eq!(0x30, cpu.reg.f);
    }

    #[test]
    fn test_correct_unsetting_of_flag() {
        let mut cpu = Cpu::new(Path::new(
            "gb-test-roms/cpu_instrs/individual/01-special.gb",
        ));
        cpu.unset_flag(Flags::Zero);
        assert_eq!(0x30, cpu.reg.f);
    }

    #[test]
    fn test_if_flag_is_active() {
        let cpu = Cpu::new(Path::new(
            "gb-test-roms/cpu_instrs/individual/01-special.gb",
        ));
        assert_eq!(true, cpu.flag_is_active(Flags::Zero));
        assert_eq!(true, cpu.flag_is_active(Flags::Carry));
        assert_eq!(true, cpu.flag_is_active(Flags::HalfCarry));
    }
}
