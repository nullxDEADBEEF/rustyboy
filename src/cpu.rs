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

    // --------------------------- UTIL -----------------------------------------------
    fn reset_flags(&mut self) {
        self.reg.f &= !u8::from(Flags::Zero);
        self.reg.f &= !u8::from(Flags::HalfCarry);
        self.reg.f &= !u8::from(Flags::Carry);
    }

    fn get_flag(&self, flag: Flags) -> u8 {
        u8::from(flag)
    }

    fn set_flag(&mut self, flag: Flags) {
        self.reg.f |= u8::from(flag);
    }

    fn unset_flag(&mut self, flag: Flags) {
        self.reg.f &= !u8::from(flag);
    }

    fn flag_is_active(&self, flag: Flags) -> bool {
        self.reg.f & u8::from(flag.clone()) == u8::from(flag)
    }

    fn get_src_register(&self, src_register: u8) -> u8 {
        match src_register {
            0 => self.reg.b,
            1 => self.reg.c,
            2 => self.reg.d,
            3 => self.reg.e,
            4 => self.reg.h,
            5 => self.reg.l,
            6 => self.mmu.working_ram[self.reg.get_hl() as usize],
            7 => self.reg.a,
            _ => {
                println!("Didnt find a source register, got: {}", src_register);
                u8::MAX
            }
        }
    }

    fn set_register(&mut self, dest_register: u8, src_register: u8) {
        match dest_register {
            0 => self.reg.b = self.get_src_register(src_register),
            1 => self.reg.c = self.get_src_register(src_register),
            2 => self.reg.d = self.get_src_register(src_register),
            3 => self.reg.e = self.get_src_register(src_register),
            4 => self.reg.h = self.get_src_register(src_register),
            5 => self.reg.l = self.get_src_register(src_register),
            6 => {
                self.mmu.working_ram[self.reg.get_hl() as usize] =
                    self.get_src_register(src_register)
            }
            7 => self.reg.a = self.get_src_register(src_register),
            _ => println!("Didnt find a destination register, got: {}", dest_register),
        }
    }

    // --------------------------- OPCODES -----------------------------------------------

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

        self.reg.b = self.reg.b.wrapping_add(1);
        self.unset_flag(Flags::Operation);
        // set half carry flag if we overflowed the lower 4-bits
        if self.reg.b & 0xF > 0x8 {
            self.set_flag(Flags::HalfCarry);
        }
        if self.reg.b == 0 {
            self.set_flag(Flags::Zero);
        }
    }

    // decrement register B
    fn dec_b(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.b = self.reg.b.wrapping_sub(1);
        // set operation flag since we subtracted
        self.set_flag(Flags::Operation);

        if self.reg.b == 0 {
            self.set_flag(Flags::Zero);
        }

        // NOTE: borrow means if there was a carry/halfcarry from the preceeding operation
        // so the last 4-bits overflowed
        if self.reg.b & 0xF == 0xF {
            self.set_flag(Flags::HalfCarry);
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

        self.reset_flags();
        self.reg.a = (self.reg.a << 1)
            | (if self.reg.a & self.get_flag(Flags::Zero) == 0x80 {
                1
            } else {
                0
            });
    }

    // load stack pointer at given address
    fn load_sp_at_addr(&mut self) {
        self.reg.pc += 3;
        self.m = 5;
        self.t = 20;

        // store lower byte of sp at addr
        self.mmu.working_ram[self.reg.pc as usize] = self.reg.sp as u8;

        // store upper byte of sp at addr + 1
        self.mmu.working_ram[self.reg.pc as usize + 1] = (self.reg.sp >> 8 & 0xFF) as u8;
    }

    // add register BC to HL
    fn add_hl_bc(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg
            .set_hl(self.reg.get_hl().wrapping_add(self.reg.get_bc()));
        self.unset_flag(Flags::Operation);

        if self.reg.get_hl() > 0x7FA6 {
            self.set_flag(Flags::Carry);
            println!("CARRY");
        } else if self.reg.get_hl() > 0x800 {
            self.set_flag(Flags::HalfCarry);
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
        self.unset_flag(Flags::Operation);
        if self.reg.c == 0 {
            self.set_flag(Flags::Zero);
        } else if self.reg.c > 0x8 {
            self.set_flag(Flags::HalfCarry);
        }
    }

    // decrement contents of register C by 1
    fn dec_c(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.c = self.reg.c.wrapping_sub(1);
        self.set_flag(Flags::Operation);
        if self.reg.c == 0 {
            self.set_flag(Flags::Zero);
        } else if self.reg.c & 0xF == 0 {
            self.set_flag(Flags::HalfCarry);
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

        self.reset_flags();
        self.reg.a = (self.reg.a >> 1) | (if self.reg.a & 0x01 == 0x01 { 0x80 } else { 0 })
    }

    // stop system clock and oscillator circuit
    fn stop(&mut self) {}

    // load 2 bytes of immediate data into register pair DE
    fn ld_de(&mut self) {
        self.reg.pc += 3;
        self.m = 3;
        self.t = 12;

        self.reg.set_de(self.mmu.read_word(self.reg.pc.into()));
    }

    // store contents of register A in memory location specified by register pair DE
    fn ld_a(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.mmu.working_ram[self.reg.get_de() as usize] = self.reg.a;
    }

    // increment contents of register pair DE by 1
    fn inc_de(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg.set_de(self.reg.get_de().wrapping_add(1));
    }

    // increment contents of register D by 1
    // TODO: check that the flags are set correctly
    fn inc_d(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.d = self.reg.d.wrapping_add(1);
        self.unset_flag(Flags::Operation);
        if self.reg.d == 0 {
            self.set_flag(Flags::Zero);
        } else if self.reg.d & 0x8 == 0 {
            self.set_flag(Flags::HalfCarry);
        }
    }

    // decrement contents of register D by 1
    // TODO: check that flags are set correctly
    fn dec_d(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.d = self.reg.d.wrapping_sub(1);
        self.set_flag(Flags::Operation);
        if self.reg.d == 0 {
            self.set_flag(Flags::Zero);
        } else if self.reg.d & 0xF == 0 {
            self.set_flag(Flags::HalfCarry);
        }
    }

    // load 8-bit immediate operand into register D
    // TODO: check that correct 8-bit operand is loaded in
    fn ld_d(&mut self) {
        self.reg.pc += 2;
        self.m = 2;
        self.t = 8;

        self.reg.d = self.mmu.read_byte(self.reg.pc.into());
    }

    // rotate contents of register A to the left, through the carry flag
    // TODO: check if contents of carry flag are copied correctly
    fn rla(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reset_flags();
        self.reg.a = (self.reg.a << 1) | (if self.reg.a & 0x80 == 0x80 { 1 } else { 0 })
    }

    // jump s8 steps from current address in the pc
    // TODO: check up on correct implementation
    fn jr(&mut self) {
        self.reg.pc += 2;
        self.m = 3;
        self.t = 12;

        self.reg.pc += self.mmu.working_ram[self.reg.pc as usize] as u16;
    }

    // add contents of register pair DE to the contents of register pair HL
    fn add_hl_de(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg
            .set_hl(self.reg.get_hl().wrapping_add(self.reg.get_de()));
        self.unset_flag(Flags::Operation);
        if self.reg.get_hl() & 0x800 == 0 {
            self.set_flag(Flags::HalfCarry);
        } else if self.reg.get_hl() & 0x8000 == 0 {
            self.set_flag(Flags::Carry);
        }
    }

    // load 8-bit contents of memory specified by register pair DE into register A
    fn ld_a_de(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg.a = self.mmu.working_ram[self.reg.get_de() as usize];
    }

    // decrement contents of register pair DE by 1
    fn dec_de(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg.set_de(self.reg.get_de().wrapping_sub(1));
    }

    // increment contents of register E by 1
    fn inc_e(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.e = self.reg.e.wrapping_add(1);
        self.unset_flag(Flags::Operation);
        if self.reg.e == 0 {
            self.set_flag(Flags::Zero);
        } else if self.reg.e & 0xF == 0 {
            self.set_flag(Flags::HalfCarry);
        }
    }

    // decrement contents of register E by 1
    fn dec_e(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.e = self.reg.e.wrapping_sub(1);
        self.set_flag(Flags::Operation);
        if self.reg.e == 0 {
            self.set_flag(Flags::Zero);
        } else if self.reg.e & 0xF == 0 {
            self.set_flag(Flags::HalfCarry);
        }
    }

    // load 8-bit immediate operand into register E
    fn ld_e(&mut self) {
        self.reg.pc += 2;
        self.m = 2;
        self.t = 8;

        self.reg.e = self.mmu.read_byte(self.reg.pc.into());
    }

    // rotate contents of register A ro the right through carry flag
    fn rra(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.a = (self.reg.a >> 1) | (if self.reg.a & 0x01 == 0x01 { 0x80 } else { 0 })
    }

    // if z flag is 0, jump s8 steps from current address in pc
    // if not, instruction following is executed
    fn jr_nz(&mut self) {
        self.reg.pc += 2;
        self.m = 2;
        self.t = 8;

        if self.reg.f & self.get_flag(Flags::Zero) == 0 {
            self.reg.pc += self.mmu.working_ram[self.reg.pc as usize] as u16;
        } else {
            self.reg.pc += 1;
        }
    }

    // load 2 bytes of immediate data into register pair HL
    fn ld_hl(&mut self) {
        self.reg.pc += 3;
        self.m = 3;
        self.t = 12;

        self.reg.set_hl(self.mmu.read_word(self.reg.pc.into()));
    }

    // store contents of register A into memory location specified by register pair HL
    // and increment the contents of HL
    fn ld_hl_inc_a(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.mmu.working_ram[self.reg.get_hl() as usize] = self.reg.a;
        self.reg.set_hl(self.reg.get_hl().wrapping_add(1));
    }

    // increment contents of register pair HL by 1
    fn inc_hl(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg.set_hl(self.reg.get_hl().wrapping_add(1));
    }

    // increment contents of register H by 1
    fn inc_h(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.h = self.reg.h.wrapping_add(1);
        self.unset_flag(Flags::Operation);
        if self.reg.h == 0 {
            self.set_flag(Flags::Zero);
        } else if self.reg.h & 0xF == 0 {
            self.set_flag(Flags::HalfCarry);
        }
    }

    // decrement contents of register H by 1
    fn dec_h(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.h = self.reg.h.wrapping_sub(1);
        self.set_flag(Flags::Operation);
        if self.reg.h == 0 {
            self.set_flag(Flags::Zero);
        } else if self.reg.h & 0xF == 0 {
            self.set_flag(Flags::HalfCarry);
        }
    }

    // load 8-bit immediate operand into register H
    fn ld_h(&mut self) {
        self.reg.pc += 2;
        self.m = 2;
        self.t = 8;

        self.reg.h = self.mmu.read_byte(self.reg.pc.into());
    }

    // Decimal Adjust Accumulator, get binary-coded decimal representation after an arithmetic instruction
    // binary-coded decimal is a binary encoding of decimal numbers where each digit is represented
    // by a fixed number of bits, usually 4 or 8
    fn daa(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        // after addition
        if !self.get_flag(Flags::Operation) == 0 {
            if self.flag_is_active(Flags::HalfCarry) || self.reg.a & 0xF > 0x9 {
                self.reg.a = self.reg.a.wrapping_add(0x6);
            }
            if self.flag_is_active(Flags::Carry) || self.reg.a > 0x99 {
                self.reg.a = self.reg.a.wrapping_add(0x60);
                self.set_flag(Flags::Carry);
            }
        } else {
            // after subtraction
            if self.flag_is_active(Flags::Carry) {
                self.reg.a = self.reg.a.wrapping_sub(0x60);
            }
            if self.flag_is_active(Flags::HalfCarry) {
                self.reg.a = self.reg.a.wrapping_sub(0x6);
            }
        }

        if self.reg.a == 0 {
            self.set_flag(Flags::Zero);
        }
        self.unset_flag(Flags::HalfCarry);
    }

    // if z flag is active, jump s8 steps from current address else instruction following
    // is executed
    fn jr_z(&mut self) {
        self.reg.pc += 2;
        self.m = 2;
        self.t = 8;

        if self.flag_is_active(Flags::Zero) {
            self.reg.pc += self.mmu.working_ram[self.reg.pc as usize] as u16;
        } else {
            self.reg.pc += 1;
        }
    }

    // add contents of register pair HL to the contents of register pair HL and store in HL
    fn add_hl_hl(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg
            .set_hl(self.reg.get_hl().wrapping_add(self.reg.get_hl()));
        self.unset_flag(Flags::Operation);
        if self.reg.get_hl() & 0x800 == 0 {
            self.set_flag(Flags::HalfCarry);
        } else if self.reg.get_hl() & 0x8000 == 0 {
            self.set_flag(Flags::Carry);
        }
    }

    // load contents of memory specified by register pair HL into register A and increase
    // contents of HL
    fn ld_a_hl_plus(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg.a = self.mmu.working_ram[self.reg.get_hl() as usize];
        self.reg.set_hl(self.reg.get_hl().wrapping_add(1));
    }

    // decrement contents of register pair HL by 1
    fn dec_hl(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg.set_hl(self.reg.get_hl().wrapping_sub(1));
    }

    // increment contents of register L by 1
    fn inc_l(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.l = self.reg.l.wrapping_add(1);
        self.unset_flag(Flags::Operation);
        if self.reg.l == 0 {
            self.set_flag(Flags::Zero);
        } else if self.reg.l & 0x8 == 0 {
            self.set_flag(Flags::HalfCarry);
        }
    }

    // decrement contents of register L by 1
    fn dec_l(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.l = self.reg.l.wrapping_sub(1);
        self.set_flag(Flags::Operation);
        if self.reg.l == 0 {
            self.set_flag(Flags::Zero);
        } else if self.reg.l > 0xF {
            self.set_flag(Flags::HalfCarry);
        }
    }

    // load 8-bit immediate operand into register L
    fn ld_l(&mut self) {
        self.reg.pc += 2;
        self.m = 2;
        self.t = 8;

        self.reg.l = self.mmu.read_byte(self.reg.pc.into());
    }

    // flip all contents of register A
    fn cpl(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.a = !self.reg.a;
        self.set_flag(Flags::Operation);
        self.set_flag(Flags::HalfCarry);
    }

    // if CY flag is not set, jump s8 steps from current address
    // else instruction following JP is executed
    fn jr_nc(&mut self) {
        self.reg.pc += 2;
        self.m = 2;
        self.t = 8;

        if !self.flag_is_active(Flags::Carry) {
            self.reg.pc += self.mmu.working_ram[self.reg.pc as usize] as u16;
        } else {
            self.reg.pc += 1;
        }
    }

    // load 2 bytes of immediate data into register pair SP
    fn ld_sp(&mut self) {
        self.reg.pc += 3;
        self.m = 3;
        self.t = 12;

        self.reg.sp = self.mmu.read_word(self.reg.pc.into());
    }

    // store contents of register A in memory location specified by register pair HL
    // and decrement contents of HL
    fn ld_hlm_a(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.mmu.working_ram[self.reg.get_hl() as usize] = self.reg.a;
        self.reg.set_hl(self.reg.get_hl().wrapping_sub(1));
    }

    // increment contents of register pair SP by 1
    fn inc_sp(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg.sp = self.reg.sp.wrapping_add(1);
    }

    // increment contents of memory specified by register pair HL by 1
    fn inc_content_at_hl(&mut self) {
        self.reg.pc += 1;
        self.m = 3;
        self.t = 12;

        self.mmu.working_ram[self.reg.get_hl() as usize] += 1;
        self.unset_flag(Flags::Operation);
        if self.mmu.working_ram[self.reg.get_hl() as usize] == 0 {
            self.set_flag(Flags::Zero);
        } else if self.mmu.working_ram[self.reg.get_hl() as usize] & 0xF == 0 {
            self.set_flag(Flags::HalfCarry);
        }
    }

    // decrement contents of memory specifed by register pair HL by 1
    fn dec_content_at_hl(&mut self) {
        self.reg.pc += 1;
        self.m = 3;
        self.t = 12;

        self.mmu.working_ram[self.reg.get_hl() as usize] -= 1;
        self.set_flag(Flags::Operation);
        if self.mmu.working_ram[self.reg.get_hl() as usize] == 0 {
            self.set_flag(Flags::Zero);
        } else if self.mmu.working_ram[self.reg.get_hl() as usize] & 0xF == 0 {
            self.set_flag(Flags::HalfCarry);
        }
    }

    // store contents of 8-bit immediate operation into memory location
    // specified by register pair HL
    fn ld_hl_byte(&mut self) {
        self.reg.pc += 2;
        self.m = 3;
        self.t = 12;

        self.mmu.working_ram[self.reg.get_hl() as usize] = self.mmu.read_byte(self.reg.pc.into());
    }

    // set the carry flag
    fn scf(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.set_flag(Flags::Carry);
    }

    // if carry flag is active, jump s8 steps from current address
    // else instruction following jp is executed
    fn jr_c(&mut self) {
        self.reg.pc += 2;
        self.m = 2;
        self.t = 8;

        if self.flag_is_active(Flags::Carry) {
            self.reg.pc += self.mmu.working_ram[self.reg.pc as usize] as u16;
        } else {
            self.reg.pc += 1;
        }
    }

    // add contents of register pair SP to contents of register pair HL
    fn add_hl_sp(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg.set_hl(self.reg.get_hl().wrapping_add(self.reg.sp));
        self.unset_flag(Flags::Operation);
        if self.reg.get_hl() & 0x800 == 0 {
            self.set_flag(Flags::HalfCarry);
        } else if self.reg.get_hl() & 0x8000 == 0 {
            self.set_flag(Flags::Carry);
        }
    }

    // load contents specified by register pair HL into register A
    // decrement contents of HL
    fn ld_a_hl_dec(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg.a = self.mmu.working_ram[self.reg.get_hl() as usize];
        self.reg.set_hl(self.reg.get_hl().wrapping_sub(1));
    }

    // decrement contents of register pair SP by 1
    fn dec_sp(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg.sp = self.reg.sp.wrapping_sub(1);
    }

    // increment contents of register A by 1
    fn inc_a(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.a = self.reg.a.wrapping_add(1);
        self.unset_flag(Flags::Operation);
        if self.reg.a == 0 {
            self.set_flag(Flags::Zero);
        } else if self.reg.a & 0x8 == 0 {
            self.set_flag(Flags::HalfCarry);
        }
    }

    // decrement contents of register A by 1
    fn dec_a(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.a = self.reg.a.wrapping_sub(1);
        self.set_flag(Flags::Operation);
        if self.reg.a == 0 {
            self.set_flag(Flags::Zero);
        } else if self.reg.a & 0x8 == 0 {
            self.set_flag(Flags::HalfCarry);
        }
    }

    // load 8-bit immediate operand into register A
    fn ld_a_byte(&mut self) {
        self.reg.pc += 2;
        self.m = 2;
        self.t = 8;

        self.reg.a = self.mmu.read_byte(self.reg.pc.into());
    }

    // flip carry flag
    // TODO: check this for correctness
    fn ccf(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.unset_flag(Flags::Operation);
        self.unset_flag(Flags::HalfCarry);
        self.reg.f ^= u8::from(Flags::Carry);
    }

    // parses the opcodes from 0x40 to 0x7F
    // also handles the case of 0x76 which is the HALT opcode
    fn parse_load_opcodes(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        // HALT opcode
        if self.current_opcode == 0x76 {
            self.halted = true;
        } else {
            // LD opcodes
            // we can figure out what register to load what data into
            // by looking at the binary representation of the opcodes
            // we can see that the lowest 3-bits represents our
            // index we want to load from, and by shifting 3 bits to the right
            // we get the register we want to load into.
            // So we get, ld b,b and ld b, c and so on
            let src_register = self.current_opcode & 0x7;
            let dest_register = (self.current_opcode >> 3) & 0x7;
            self.set_register(dest_register, src_register);
        }
    }

    // parse math operations from 0x80 to 0x9F
    // we can use the same principle as the LD opcodes
    // math_operation == 0 => ADD
    // math_operation == 1 => ADC
    // math_operation == 2 => SUB
    // math_operation == 3 => SBC
    fn parse_math_opcodes(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        let register = self.current_opcode & 0x7;
        let math_operation = (self.current_opcode >> 3) & 0x7;

        if math_operation == 0 {
            self.reg.a = self.reg.a.wrapping_add(self.get_src_register(register));
            self.unset_flag(Flags::Operation);
            if self.reg.a == 0 {
                self.set_flag(Flags::Zero);
            } else if self.reg.a & 0x7 == 0 {
                self.set_flag(Flags::HalfCarry);
            } else if self.reg.a & 0x40 == 0 {
                self.set_flag(Flags::Carry);
            }
        } else if math_operation == 1 {
            self.reg.a = self
                .reg
                .a
                .wrapping_add(self.get_src_register(register) + self.get_flag(Flags::Carry));
            self.unset_flag(Flags::Operation);
            if self.reg.a == 0 {
                self.set_flag(Flags::Zero);
            } else if self.reg.a & 0x7 == 0 {
                self.set_flag(Flags::HalfCarry);
            } else if self.reg.a & 0x40 == 0 {
                self.set_flag(Flags::Carry);
            }
        } else if math_operation == 2 {
            self.reg.a -= self.reg.a.wrapping_sub(self.get_src_register(register));
            self.set_flag(Flags::Operation);
            if self.reg.a == 0 {
                self.set_flag(Flags::Zero);
            } else if self.reg.a > 0x8 {
                self.set_flag(Flags::HalfCarry);
            } else if self.get_src_register(register) > self.reg.a {
                self.set_flag(Flags::Carry);
            }
        } else if math_operation == 3 {
            self.reg.a = self.reg.a.wrapping_sub(
                self.get_src_register(register)
                    .wrapping_add(self.get_flag(Flags::Carry)),
            );
            self.set_flag(Flags::Operation);
            if self.reg.a == 0 {
                self.set_flag(Flags::Zero);
            } else if self.reg.a > 0x8 {
                self.set_flag(Flags::HalfCarry);
            } else if self
                .get_src_register(register)
                .wrapping_add(self.get_flag(Flags::Carry))
                > self.reg.a
            {
                self.set_flag(Flags::Carry);
            }
        }
    }

    // parse AND opcodes 0xA0 to 0xA7
    fn parse_and_opcodes(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        let register = self.current_opcode & 0x7;
        self.reg.a &= self.get_src_register(register);
        self.set_flag(Flags::HalfCarry);
        self.unset_flag(Flags::Operation);
        self.unset_flag(Flags::Carry);
        if self.reg.a == 0 {
            self.set_flag(Flags::Zero);
        }
    }

    // parse XOR opcodes from 0xA8 to 0xAF
    fn parse_xor_opcodes(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        let register = self.current_opcode & 0x7;
        self.reg.a ^= self.get_src_register(register);
        self.reset_flags();
        if self.reg.a == 0 {
            self.set_flag(Flags::Zero);
        }
    }

    // parse OR opcodes from 0xB0 to 0xB7
    fn parse_or_opcodes(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        let register = self.current_opcode & 0x7;
        self.reg.a |= self.get_src_register(register);
        self.reset_flags();
        if self.reg.a == 0 {
            self.set_flag(Flags::Zero);
        }
    }

    // parse CP opcodes from 0xB8 to 0xBF
    fn parse_cp_opcodes(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        let register = self.current_opcode & 0x7;
        let result = self.reg.a.wrapping_sub(self.get_src_register(register));
        self.set_flag(Flags::Operation);
        if result == 0 {
            self.set_flag(Flags::Zero);
        } else if result > 0x8 {
            self.set_flag(Flags::HalfCarry);
        } else if self.get_src_register(register) > self.reg.a {
            self.set_flag(Flags::Carry);
        }
    }

    pub fn decode_execute(&mut self) {
        self.current_opcode = self.mmu.read_byte(self.reg.pc.into());
        self.reg.pc += 1;
        match self.current_opcode {
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
            0x12 => self.ld_a(),
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
            0x40..=0x7F => self.parse_load_opcodes(),
            0x80..=0x9F => self.parse_math_opcodes(),
            0xA0..=0xA7 => self.parse_and_opcodes(),
            0xA8..=0xAF => self.parse_xor_opcodes(),
            0xB0..=0xB7 => self.parse_or_opcodes(),
            0xB8..=0xBF => self.parse_cp_opcodes(),
            _ => println!("{:#X} is not a recognized opcode...", self.current_opcode),
        }
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
        cpu.load_sp_at_addr();

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

    #[test]
    fn test_ld_de() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 3;
        let expected_t_cycles = 12;
        let expected_pc = cpu.reg.pc + 3;
        // 244 << 8 | 128
        let expected_de: u16 = 62592;
        cpu.mmu.working_ram[expected_pc as usize] = 244;
        cpu.mmu.working_ram[expected_pc as usize + 1] = 128;

        // Act
        cpu.ld_de();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_t_cycles, cpu.t);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_de, cpu.reg.get_de());
    }
}
