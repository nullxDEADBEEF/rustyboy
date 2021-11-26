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
    halted: bool,
    ei: bool,
    di: bool,
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
            halted: false,
            ei: false,
            di: false,
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

    fn set_flag_on_if(&mut self, flag: Flags, condition: bool) {
        if condition {
            self.set_flag(flag);
        } else {
            self.unset_flag(flag);
        }
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
    }

    // load 2 bytes of data into register pair BC
    fn load_bc(&mut self) {
        self.m = 3;

        self.reg.set_bc(self.mmu.read_word(self.reg.pc.into()));
        self.reg.pc += 3;
    }

    // load data from register A to memory location specified by register pair BC
    fn load_bc_a(&mut self) {
        self.m = 2;

        self.mmu.working_ram[self.reg.get_bc() as usize] = self.reg.a;
        self.reg.pc += 1;
    }

    // increment register pair BC
    fn inc_bc(&mut self) {
        self.m = 2;

        self.reg.set_bc(self.reg.get_bc() + 1);
        self.reg.pc += 1;
    }

    // increment register B
    fn inc_b(&mut self) {
        self.m = 1;

        self.reg.b = self.reg.b.wrapping_add(1);
        self.unset_flag(Flags::Operation);
        // set half carry flag if we overflowed the lower 4-bits
        self.set_flag_on_if(Flags::HalfCarry, self.reg.b > 0xF);
        self.set_flag_on_if(Flags::Zero, self.reg.b == 0);

        self.reg.pc += 1;
    }

    // decrement register B
    fn dec_b(&mut self) {
        self.m = 1;

        self.reg.b = self.reg.b.wrapping_sub(1);
        // set operation flag since we subtracted
        self.set_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.b == 0);

        // NOTE: borrow means if there was a carry/halfcarry from the preceeding operation
        // so the last 4-bits overflowed
        self.set_flag_on_if(Flags::HalfCarry, self.reg.b & 0xF == 0);
        self.reg.pc += 1;
    }

    // load value into register B
    fn load_b(&mut self) {
        self.m = 2;

        self.reg.b = self.mmu.read_byte(self.reg.pc.into());
        self.reg.pc += 2;
    }

    // rotate register A left
    fn rlca(&mut self) {
        self.m = 1;

        self.reset_flags();
        self.reg.a = (self.reg.a << 1)
            | (if self.reg.a & self.get_flag(Flags::Zero) == 0x80 {
                1
            } else {
                0
            });
        self.reg.pc += 1;
    }

    // load stack pointer at given address
    fn load_sp_at_addr(&mut self) {
        self.m = 5;

        // store lower byte of sp at addr
        self.mmu.working_ram[self.reg.pc as usize] = self.reg.sp as u8;

        // store upper byte of sp at addr + 1
        self.mmu.working_ram[self.reg.pc as usize + 1] = (self.reg.sp >> 8 & 0xFF) as u8;
        self.reg.pc += 3;
    }

    // add register BC to HL
    fn add_hl_bc(&mut self) {
        self.m = 2;

        self.reg
            .set_hl(self.reg.get_hl().wrapping_add(self.reg.get_bc()));
        self.unset_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Carry, self.reg.get_hl() > 0x7FA6);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.get_hl() > 0x800);
        self.reg.pc += 1;
    }

    // load contents specified by register BC into register A
    fn ld_a_bc(&mut self) {
        self.m = 2;

        self.reg.a = self.mmu.working_ram[self.reg.get_bc() as usize];
        self.reg.pc += 1;
    }

    // decrement register pair BC by 1
    fn dec_bc(&mut self) {
        self.m = 2;

        self.reg.set_bc(self.reg.get_bc() - 1);
        self.reg.pc += 1;
    }

    // increment contents of register C by 1
    fn inc_c(&mut self) {
        self.m = 1;

        self.reg.c = self.reg.c.wrapping_add(1);
        self.unset_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.c == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.c > 0xF);
        self.reg.pc += 1;
    }

    // decrement contents of register C by 1
    fn dec_c(&mut self) {
        self.m = 1;

        self.reg.c = self.reg.c.wrapping_sub(1);
        self.set_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.c == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.c & 0xF == 0);
        self.reg.pc += 1;
    }

    // load immediate operand into register C
    fn ld_c(&mut self) {
        self.m = 2;

        self.reg.c = self.mmu.working_ram[self.reg.pc as usize];
        self.reg.pc += 2;
    }

    // Rotate contents of register A to the right
    fn rrca(&mut self) {
        self.m = 1;

        self.reset_flags();
        self.reg.a = (self.reg.a >> 1) | (if self.reg.a & 0x01 == 0x01 { 0x80 } else { 0 });
        self.reg.pc += 1;
    }

    // stop system clock and oscillator circuit
    // TODO: implement
    fn stop(&mut self) {
        self.m = 1;
    }

    // load 2 bytes of immediate data into register pair DE
    fn ld_de(&mut self) {
        self.m = 3;

        self.reg.set_de(self.mmu.read_word(self.reg.pc.into()));
        self.reg.pc += 3;
    }

    // store contents of register A in memory location specified by register pair DE
    fn ld_a(&mut self) {
        self.m = 2;

        self.mmu.working_ram[self.reg.get_de() as usize] = self.reg.a;
        self.reg.pc += 1;
    }

    // increment contents of register pair DE by 1
    fn inc_de(&mut self) {
        self.m = 2;

        self.reg.set_de(self.reg.get_de().wrapping_add(1));
        self.reg.pc += 1;
    }

    // increment contents of register D by 1
    fn inc_d(&mut self) {
        self.m = 1;

        self.reg.d = self.reg.d.wrapping_add(1);
        self.unset_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.d == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.d > 0xF);
        self.reg.pc += 1;
    }

    // decrement contents of register D by 1
    fn dec_d(&mut self) {
        self.m = 1;

        self.reg.d = self.reg.d.wrapping_sub(1);
        self.set_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.d == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.d & 0xF == 0);
        self.reg.pc += 1;
    }

    // load 8-bit immediate operand into register D
    fn ld_d(&mut self) {
        self.m = 2;

        self.reg.d = self.mmu.read_byte(self.reg.pc.into());
        self.reg.pc += 2;
    }

    // rotate contents of register A to the left, through the carry flag
    fn rla(&mut self) {
        self.m = 1;

        self.reset_flags();
        self.reg.a = (self.reg.a << 1) | (if self.reg.a & 0x80 == 0x80 { 1 } else { 0 });
        self.reg.pc += 1;
    }

    // jump s8 steps from current address in the pc
    fn jr(&mut self) {
        self.m = 3;

        self.reg.pc += self.mmu.working_ram[self.reg.pc as usize] as u16;
    }

    // add contents of register pair DE to the contents of register pair HL
    fn add_hl_de(&mut self) {
        self.m = 2;

        self.reg
            .set_hl(self.reg.get_hl().wrapping_add(self.reg.get_de()));
        self.unset_flag(Flags::Operation);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.get_hl() & 0x07FF == 0);
        self.set_flag_on_if(Flags::Carry, self.reg.get_hl() & 0x7FFF == 0);
        self.reg.pc += 1;
    }

    // load 8-bit contents of memory specified by register pair DE into register A
    fn ld_a_de(&mut self) {
        self.m = 2;

        self.reg.a = self.mmu.working_ram[self.reg.get_de() as usize];
        self.reg.pc += 1;
    }

    // decrement contents of register pair DE by 1
    fn dec_de(&mut self) {
        self.m = 2;

        self.reg.set_de(self.reg.get_de().wrapping_sub(1));
        self.reg.pc += 1;
    }

    // increment contents of register E by 1
    fn inc_e(&mut self) {
        self.m = 1;

        self.reg.e = self.reg.e.wrapping_add(1);
        self.unset_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.e == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.e & 0xF == 0);
        self.reg.pc += 1;
    }

    // decrement contents of register E by 1
    fn dec_e(&mut self) {
        self.m = 1;

        self.reg.e = self.reg.e.wrapping_sub(1);
        self.set_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.e == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.e & 0xF == 0);
        self.reg.pc += 1;
    }

    // load 8-bit immediate operand into register E
    fn ld_e(&mut self) {
        self.m = 2;

        self.reg.e = self.mmu.read_byte(self.reg.pc.into());
        self.reg.pc += 2;
    }

    // rotate contents of register A ro the right through carry flag
    fn rra(&mut self) {
        self.m = 1;

        self.reg.a = (self.reg.a >> 1) | (if self.reg.a & 0x01 == 0x01 { 0x80 } else { 0 });
        self.reg.pc += 1;
    }

    // if z flag is 0, jump s8 steps from current address in pc
    // if not, instruction following is executed
    fn jr_nz(&mut self) {
        self.m = 2;

        if !self.flag_is_active(Flags::Zero) {
            self.reg.pc += self.mmu.working_ram[self.reg.pc as usize] as u16;
        } else {
            self.reg.pc += 1;
        }
    }

    // load 2 bytes of immediate data into register pair HL
    fn ld_hl(&mut self) {
        self.m = 3;

        self.reg.set_hl(self.mmu.read_word(self.reg.pc.into()));
        self.reg.pc += 3;
    }

    // store contents of register A into memory location specified by register pair HL
    // and increment the contents of HL
    fn ld_hl_inc_a(&mut self) {
        self.m = 2;

        self.mmu.working_ram[self.reg.get_hl() as usize] = self.reg.a;
        self.reg.set_hl(self.reg.get_hl().wrapping_add(1));
        self.reg.pc += 1;
    }

    // increment contents of register pair HL by 1
    fn inc_hl(&mut self) {
        self.m = 2;

        self.reg.set_hl(self.reg.get_hl().wrapping_add(1));
        self.reg.pc += 1;
    }

    // increment contents of register H by 1
    fn inc_h(&mut self) {
        self.m = 1;

        self.reg.h = self.reg.h.wrapping_add(1);
        self.unset_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.h == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.h & 0xF == 0);
        self.reg.pc += 1;
    }

    // decrement contents of register H by 1
    fn dec_h(&mut self) {
        self.m = 1;

        self.reg.h = self.reg.h.wrapping_sub(1);
        self.set_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.h == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.h & 0xF == 0);
        self.reg.pc += 1;
    }

    // load 8-bit immediate operand into register H
    fn ld_h(&mut self) {
        self.m = 2;

        self.reg.h = self.mmu.read_byte(self.reg.pc.into());
        self.reg.pc += 2;
    }

    // Decimal Adjust Accumulator, get binary-coded decimal representation after an arithmetic instruction
    // binary-coded decimal is a binary encoding of decimal numbers where each digit is represented
    // by a fixed number of bits, usually 4 or 8
    fn daa(&mut self) {
        self.m = 1;

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
        self.reg.pc += 1;
    }

    // if z flag is active, jump s8 steps from current address else instruction following
    // is executed
    fn jr_z(&mut self) {
        self.m = 2;

        if self.flag_is_active(Flags::Zero) {
            self.reg.pc += self.mmu.working_ram[self.reg.pc as usize] as u16;
        } else {
            self.reg.pc += 1;
        }
    }

    // add contents of register pair HL to the contents of register pair HL and store in HL
    fn add_hl_hl(&mut self) {
        self.m = 2;

        self.reg
            .set_hl(self.reg.get_hl().wrapping_add(self.reg.get_hl()));
        self.unset_flag(Flags::Operation);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.get_hl() & 0x07FF == 0);
        self.set_flag_on_if(Flags::Carry, self.reg.get_hl() & 0x7FF == 0);
        self.reg.pc += 1;
    }

    // load contents of memory specified by register pair HL into register A and increase
    // contents of HL
    fn ld_a_hl_plus(&mut self) {
        self.m = 2;

        self.reg.a = self.mmu.working_ram[self.reg.get_hl() as usize];
        self.reg.set_hl(self.reg.get_hl().wrapping_add(1));
        self.reg.pc += 1;
    }

    // decrement contents of register pair HL by 1
    fn dec_hl(&mut self) {
        self.m = 2;

        self.reg.set_hl(self.reg.get_hl().wrapping_sub(1));
        self.reg.pc += 1;
    }

    // increment contents of register L by 1
    fn inc_l(&mut self) {
        self.m = 1;

        self.reg.l = self.reg.l.wrapping_add(1);
        self.unset_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.l == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.l & 0x8 == 0);
        self.reg.pc += 1;
    }

    // decrement contents of register L by 1
    fn dec_l(&mut self) {
        self.m = 1;

        self.reg.l = self.reg.l.wrapping_sub(1);
        self.set_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.l == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.l > 0xF);
        self.reg.pc += 1;
    }

    // load 8-bit immediate operand into register L
    fn ld_l(&mut self) {
        self.m = 2;

        self.reg.l = self.mmu.read_byte(self.reg.pc.into());
        self.reg.pc += 2;
    }

    // flip all contents of register A
    fn cpl(&mut self) {
        self.m = 1;

        self.reg.a = !self.reg.a;
        self.set_flag(Flags::Operation);
        self.set_flag(Flags::HalfCarry);
        self.reg.pc += 1;
    }

    // if CY flag is not set, jump s8 steps from current address
    // else instruction following JP is executed
    fn jr_nc(&mut self) {
        self.m = 2;

        if !self.flag_is_active(Flags::Carry) {
            self.reg.pc += self.mmu.working_ram[self.reg.pc as usize] as u16;
        } else {
            self.reg.pc += 1;
        }
    }

    // load 2 bytes of immediate data into register pair SP
    fn ld_sp(&mut self) {
        self.m = 3;

        self.reg.sp = self.mmu.read_word(self.reg.pc.into());
        self.reg.pc += 3;
    }

    // store contents of register A in memory location specified by register pair HL
    // and decrement contents of HL
    fn ld_hlm_a(&mut self) {
        self.m = 2;

        self.mmu.working_ram[self.reg.get_hl() as usize] = self.reg.a;
        self.reg.set_hl(self.reg.get_hl().wrapping_sub(1));
        self.reg.pc += 1;
    }

    // increment contents of register pair SP by 1
    fn inc_sp(&mut self) {
        self.m = 2;

        self.reg.sp = self.reg.sp.wrapping_add(1);
        self.reg.pc += 1;
    }

    // increment contents of memory specified by register pair HL by 1
    fn inc_content_at_hl(&mut self) {
        self.m = 3;

        self.mmu.working_ram[self.reg.get_hl() as usize] += 1;
        self.unset_flag(Flags::Operation);
        self.set_flag_on_if(
            Flags::Zero,
            self.mmu.working_ram[self.reg.get_hl() as usize] == 0,
        );
        self.set_flag_on_if(
            Flags::HalfCarry,
            self.mmu.working_ram[self.reg.get_hl() as usize] & 0xF == 0,
        );
        self.reg.pc += 1;
    }

    // decrement contents of memory specifed by register pair HL by 1
    fn dec_content_at_hl(&mut self) {
        self.m = 3;

        self.mmu.working_ram[self.reg.get_hl() as usize] -= 1;
        self.set_flag(Flags::Operation);
        self.set_flag_on_if(
            Flags::Zero,
            self.mmu.working_ram[self.reg.get_hl() as usize] == 0,
        );
        self.set_flag_on_if(
            Flags::HalfCarry,
            self.mmu.working_ram[self.reg.get_hl() as usize] & 0xF == 0,
        );
        self.reg.pc += 1;
    }

    // store contents of 8-bit immediate operation into memory location
    // specified by register pair HL
    fn ld_hl_byte(&mut self) {
        self.m = 3;

        self.mmu.working_ram[self.reg.get_hl() as usize] = self.mmu.read_byte(self.reg.pc.into());
        self.reg.pc += 2;
    }

    // set the carry flag
    fn scf(&mut self) {
        self.m = 1;

        self.set_flag(Flags::Carry);
        self.reg.pc += 1;
    }

    // if carry flag is active, jump s8 steps from current address
    // else instruction following jp is executed
    fn jr_c(&mut self) {
        self.m = 2;

        if self.flag_is_active(Flags::Carry) {
            self.reg.pc += self.mmu.working_ram[self.reg.pc as usize] as u16;
        } else {
            self.reg.pc += 1;
        }
    }

    // add contents of register pair SP to contents of register pair HL
    fn add_hl_sp(&mut self) {
        self.m = 2;

        self.reg.set_hl(self.reg.get_hl().wrapping_add(self.reg.sp));
        self.unset_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.get_hl() & 0x07FF == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.get_hl() & 0x7FF == 0);
        self.reg.pc += 1;
    }

    // load contents specified by register pair HL into register A
    // decrement contents of HL
    fn ld_a_hl_dec(&mut self) {
        self.m = 2;

        self.reg.a = self.mmu.working_ram[self.reg.get_hl() as usize];
        self.reg.set_hl(self.reg.get_hl().wrapping_sub(1));
        self.reg.pc += 1;
    }

    // decrement contents of register pair SP by 1
    fn dec_sp(&mut self) {
        self.m = 2;

        self.reg.sp = self.reg.sp.wrapping_sub(1);
        self.reg.pc += 1;
    }

    // increment contents of register A by 1
    fn inc_a(&mut self) {
        self.m = 1;

        self.reg.a = self.reg.a.wrapping_add(1);
        self.unset_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.a & 0x8 == 0);
        self.reg.pc += 1;
    }

    // decrement contents of register A by 1
    fn dec_a(&mut self) {
        self.m = 1;

        self.reg.a = self.reg.a.wrapping_sub(1);
        self.set_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.a & 0x8 == 0);
        self.reg.pc += 1;
    }

    // load 8-bit immediate operand into register A
    fn ld_a_byte(&mut self) {
        self.m = 2;

        self.reg.a = self.mmu.read_byte(self.reg.pc.into());
        self.reg.pc += 2;
    }

    // flip carry flag
    fn ccf(&mut self) {
        self.m = 1;

        self.unset_flag(Flags::Operation);
        self.unset_flag(Flags::HalfCarry);
        self.reg.f ^= u8::from(Flags::Carry);
        self.reg.pc += 1;
    }

    // parses the opcodes from 0x40 to 0x7F
    // also handles the case of 0x76 which is the HALT opcode
    fn parse_load_opcodes(&mut self) {
        self.m = 1;

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
        self.reg.pc += 1;
    }

    // parse math operations from 0x80 to 0x9F
    // we can use the same principle as the LD opcodes
    // math_operation == 0 => ADD
    // math_operation == 1 => ADC
    // math_operation == 2 => SUB
    // math_operation == 3 => SBC
    //
    // TODO: Figure out if we need to handle special case with (HL)
    // if so then we need to increment the pc
    fn parse_math_opcodes(&mut self) {
        self.m = 1;

        let register = self.current_opcode & 0x7;
        let math_operation = (self.current_opcode >> 3) & 0x7;

        if math_operation == 0 {
            self.reg.a = self.reg.a.wrapping_add(self.get_src_register(register));
            self.unset_flag(Flags::Operation);
            self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
            self.set_flag_on_if(Flags::HalfCarry, self.reg.a & 0x7 == 0);
            self.set_flag_on_if(Flags::Carry, self.reg.a & 0x40 == 0);
        } else if math_operation == 1 {
            self.reg.a = self
                .reg
                .a
                .wrapping_add(self.get_src_register(register) + self.get_flag(Flags::Carry));
            self.unset_flag(Flags::Operation);
            self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
            self.set_flag_on_if(Flags::HalfCarry, self.reg.a & 0x7 == 0);
            self.set_flag_on_if(Flags::Carry, self.reg.a & 0x40 == 0);
        } else if math_operation == 2 {
            self.reg.a -= self.reg.a.wrapping_sub(self.get_src_register(register));
            self.set_flag(Flags::Operation);
            self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
            self.set_flag_on_if(Flags::HalfCarry, self.reg.a > 0x8);
            self.set_flag_on_if(Flags::Carry, self.get_src_register(register) > self.reg.a);
        } else if math_operation == 3 {
            self.reg.a = self.reg.a.wrapping_sub(
                self.get_src_register(register)
                    .wrapping_add(self.get_flag(Flags::Carry)),
            );
            self.set_flag(Flags::Operation);
            self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
            self.set_flag_on_if(Flags::HalfCarry, self.reg.a > 0x8);
            self.set_flag_on_if(
                Flags::Carry,
                self.get_src_register(register)
                    .wrapping_add(self.get_flag(Flags::Carry))
                    > self.reg.a,
            );
        }
        self.reg.pc += 1;
    }

    // parse AND opcodes 0xA0 to 0xA7
    fn parse_and_opcodes(&mut self) {
        self.m = 1;

        let register = self.current_opcode & 0x7;
        self.reg.a &= self.get_src_register(register);
        self.set_flag(Flags::HalfCarry);
        self.unset_flag(Flags::Operation);
        self.unset_flag(Flags::Carry);
        self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
        self.reg.pc += 1;
    }

    // parse XOR opcodes from 0xA8 to 0xAF
    fn parse_xor_opcodes(&mut self) {
        self.m = 1;

        let register = self.current_opcode & 0x7;
        self.reg.a ^= self.get_src_register(register);
        self.reset_flags();
        self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
        self.reg.pc += 1;
    }

    // parse OR opcodes from 0xB0 to 0xB7
    fn parse_or_opcodes(&mut self) {
        self.m = 1;

        let register = self.current_opcode & 0x7;
        self.reg.a |= self.get_src_register(register);
        self.reset_flags();
        self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
        self.reg.pc += 1;
    }

    // parse CP opcodes from 0xB8 to 0xBF
    fn parse_cp_opcodes(&mut self) {
        self.m = 1;

        let register = self.current_opcode & 0x7;
        let result = self.reg.a.wrapping_sub(self.get_src_register(register));
        self.set_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.set_flag_on_if(Flags::HalfCarry, result > 0x8);
        self.set_flag_on_if(Flags::Carry, self.get_src_register(register) > self.reg.a);
        self.reg.pc += 1;
    }

    // return from subroutine if nz
    fn ret_nz(&mut self) {
        self.m = 5;

        if !self.flag_is_active(Flags::Zero) {
            self.reg.pc = self.mmu.read_word(self.reg.sp.into());
            self.reg.sp += 2;
        }
        self.reg.pc += 1;
    }

    // pop contents of memory stack into register pair BC
    fn pop_bc(&mut self) {
        self.m = 3;

        let lower_byte = self.mmu.read_byte(self.reg.sp.into());
        self.reg.c = lower_byte;
        self.reg.sp += 1;
        let upper_byte = self.mmu.read_byte(self.reg.sp.into());
        self.reg.b = upper_byte;
        self.reg.sp += 1;
        self.reg.pc += 1;
    }

    // jump to address if condition is met
    fn jp_nz(&mut self) {
        self.m = 4;

        if !self.flag_is_active(Flags::Zero) {
            self.reg.pc = self.mmu.read_word(self.reg.pc.into());
        } else {
            self.reg.pc += 1;
        }
    }

    // jump to address
    fn jp(&mut self) {
        self.m = 4;
        self.reg.pc = self.mmu.read_word(self.reg.pc.into());
    }

    // call address if condition is met
    fn call_nz(&mut self) {
        if !self.flag_is_active(Flags::Zero) {
            self.m = 6;
            self.reg.sp -= 2;
            self.mmu.write_word(self.reg.sp.into(), self.reg.pc + 2);
            self.reg.pc = self.mmu.read_word(self.reg.pc.into());
        } else {
            self.reg.pc += 2;
            self.m = 3;
        }
    }

    // push contents of register pair BC onto the memory stack
    fn push_bc(&mut self) {
        self.m = 4;

        self.reg.sp -= 2;
        self.mmu.write_word(self.reg.sp.into(), self.reg.get_bc());
        self.reg.pc += 1;
    }

    // add 8-bit immediate to register A
    fn add_a_byte(&mut self) {
        self.m = 2;

        self.reg.a = self
            .reg
            .a
            .wrapping_add(self.mmu.read_byte(self.reg.pc.into()));
        self.unset_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.a & 0x7 == 0);
        self.set_flag_on_if(Flags::Carry, self.reg.a & 0x80 == 0);
        self.reg.pc += 2;
    }

    // call address
    fn rst_zero(&mut self) {
        self.m = 4;

        self.reg.sp -= 2;
        self.mmu.write_word(self.reg.sp.into(), self.reg.pc);
        self.reg.pc = 0x00
    }

    // return from subroutine if condition is met
    fn ret_z(&mut self) {
        if self.flag_is_active(Flags::Zero) {
            self.m = 5;
            self.reg.pc = self.mmu.read_word(self.reg.sp.into());
            self.reg.sp += 2;
        } else {
            self.m = 2;
        }
    }

    // return from subroutine
    fn ret(&mut self) {
        self.m = 4;

        self.reg.pc = self.mmu.read_word(self.reg.sp.into());
        self.reg.sp += 2;
    }

    // jump to address if condition is met
    fn jp_z(&mut self) {
        if self.flag_is_active(Flags::Zero) {
            self.m = 4;
            self.reg.pc = self.mmu.read_word(self.reg.pc.into());
        } else {
            self.m = 3;
            self.reg.pc += 2;
        }
    }

    // call address if condition is met
    fn call_z(&mut self) {
        if self.flag_is_active(Flags::Zero) {
            self.m = 6;
            self.reg.sp -= 2;
            self.mmu.write_word(self.reg.sp.into(), self.reg.pc + 2);
            self.reg.pc = self.mmu.read_word(self.reg.pc.into());
        } else {
            self.m = 3;
            self.reg.pc += 2;
        }
    }

    // push address of instruction on the stack
    fn call(&mut self) {
        self.m = 6;

        self.reg.sp -= 2;
        self.mmu.write_word(self.reg.sp.into(), self.reg.pc + 2);
        self.reg.pc = self.mmu.read_word(self.reg.pc.into());
    }

    // add 8-bit immediate and carry flag to register A
    fn adc_a(&mut self) {
        self.m = 2;

        self.reg.a = self
            .reg
            .a
            .wrapping_add(self.mmu.read_byte(self.reg.pc.into()) + self.get_flag(Flags::Carry));
        self.unset_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.a & 0x7 == 0);
        self.set_flag_on_if(Flags::Carry, self.reg.a & 0x80 == 0);
        self.reg.pc += 2;
    }

    // call address
    fn rst_one(&mut self) {
        self.m = 4;

        self.reg.sp -= 2;
        self.mmu.write_word(self.reg.sp.into(), self.reg.pc);
        self.reg.pc = 0x08;
    }

    // return from subroutine if condition is met
    fn ret_nc(&mut self) {
        if !self.flag_is_active(Flags::Carry) {
            self.m = 5;
            self.reg.pc = self.mmu.read_word(self.reg.sp.into());
            self.reg.sp += 2;
        } else {
            self.m = 2;
        }
    }

    // pop contents from memory stack onto register pair DE
    fn pop_de(&mut self) {
        self.m = 3;

        self.reg.set_de(self.mmu.read_word(self.reg.sp.into()));
        self.reg.sp += 2;
    }

    // jump to address if condition is met
    fn jp_nc(&mut self) {
        if !self.flag_is_active(Flags::Carry) {
            self.m = 4;
            self.reg.pc = self.mmu.read_word(self.reg.pc.into());
        } else {
            self.m = 3;
            self.reg.pc += 2;
        }
    }

    // call address if condition is met
    fn call_nc(&mut self) {
        if !self.flag_is_active(Flags::Carry) {
            self.m = 6;
            self.reg.sp -= 2;
            self.mmu.write_word(self.reg.sp.into(), self.reg.pc + 2);
            self.reg.pc = self.mmu.read_word(self.reg.pc.into());
        } else {
            self.m = 3;
            self.reg.pc += 2;
        }
    }

    // push contents of register pair DE onto the memeory stack
    fn push_de(&mut self) {
        self.m = 4;

        self.reg.sp -= 2;
        self.mmu.write_word(self.reg.sp.into(), self.reg.get_de());
    }

    // subtract 8-bit immediate from contents of register A
    fn sub(&mut self) {
        self.m = 2;

        self.reg.a = self
            .reg
            .a
            .wrapping_sub(self.mmu.read_byte(self.reg.pc.into()));
        self.set_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.a > 0xF);
        self.set_flag_on_if(
            Flags::Carry,
            self.reg.a < self.mmu.read_byte(self.reg.pc.into()),
        );
        self.reg.pc += 1;
    }

    // call address
    fn rst_two(&mut self) {
        self.m = 4;

        self.reg.sp -= 2;
        self.mmu.write_word(self.reg.sp.into(), self.reg.pc);
        self.reg.pc = 0x10;
    }

    // return from subroutine if condition is met
    fn ret_c(&mut self) {
        self.reg.pc += 1;

        if self.flag_is_active(Flags::Carry) {
            self.m = 5;
            self.reg.pc = self.mmu.read_word(self.reg.sp.into());
            self.reg.sp += 2;
        } else {
            self.m = 2;
        }
    }

    // return from subroutine and enable interrupts
    fn reti(&mut self) {
        self.m = 4;

        self.reg.pc = self.mmu.read_word(self.reg.sp.into());
        self.reg.sp = self.reg.sp.wrapping_add(2);
        self.ei = true;
    }

    // jump to address if condition is met
    fn jp_c(&mut self) {
        if self.flag_is_active(Flags::Carry) {
            self.m = 4;
            self.reg.pc = self.mmu.read_word(self.reg.pc.into());
        } else {
            self.m = 3;
            self.reg.pc += 2;
        }
    }

    // call address if condition is met
    fn call_c(&mut self) {
        if self.flag_is_active(Flags::Carry) {
            self.m = 6;
            self.reg.sp -= 2;
            self.mmu.write_word(self.reg.sp.into(), self.reg.pc + 2);
            self.reg.pc = self.mmu.read_word(self.reg.pc.into());
        } else {
            self.m = 3;
            self.reg.pc += 2;
        }
    }

    // subtract contents of 8-bit immediate and carry flag from register A
    fn sbc_a(&mut self) {
        self.m = 2;

        self.reg.a = self
            .reg
            .a
            .wrapping_sub(self.mmu.read_byte(self.reg.pc.into()) + self.get_flag(Flags::Carry));
        self.set_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.a > 0xF);
        self.set_flag_on_if(
            Flags::Carry,
            self.mmu
                .read_byte(self.reg.pc.into())
                .wrapping_add(self.get_flag(Flags::Carry))
                > self.reg.a,
        );
        self.reg.pc += 1;
    }

    // call adress
    fn rst_three(&mut self) {
        self.reg.pc += 1;
        self.m = 4;

        self.reg.sp -= 2;
        self.mmu.write_word(self.reg.sp.into(), self.reg.pc);
        self.reg.pc = 0x18;
    }

    // store contents of register A in internal ram, port register or mode register
    fn ld_addr_a(&mut self) {
        self.m = 3;

        self.mmu.write_byte(
            (0xFF00 | self.mmu.read_byte(self.reg.pc.into()) as u16).into(),
            self.reg.a,
        );
        self.reg.pc += 1;
    }

    // pop contents from memory stack into register pair HL
    fn pop_hl(&mut self) {
        self.m = 3;

        self.reg.set_hl(self.mmu.read_word(self.reg.sp.into()));
        self.reg.sp += 2;
    }

    // store contents of register A in the internal ram, port register or mode register
    fn ld_addr_c_a(&mut self) {
        self.m = 2;
        self.mmu
            .write_byte((0xFF00 | self.reg.c as u16).into(), self.reg.a);
    }

    // push contents of register pair HL onto the memory stack
    fn push_hl(&mut self) {
        self.m = 4;

        self.reg.sp -= 2;
        self.mmu.write_word(self.reg.sp.into(), self.reg.get_hl());
    }

    // bitwise AND value with register A
    fn and_a(&mut self) {
        self.m = 2;

        self.reg.a &= self.mmu.read_byte(self.reg.pc.into());
        self.unset_flag(Flags::Operation);
        self.unset_flag(Flags::Carry);
        self.set_flag(Flags::HalfCarry);
        self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
        self.reg.pc += 1;
    }

    // call adress
    fn rst_four(&mut self) {
        self.m = 4;

        self.reg.sp -= 2;
        self.mmu.write_word(self.reg.sp.into(), self.reg.pc);
        self.reg.pc = 0x20;
    }

    // Add contents of 2's complement immediate operand to the sp
    fn add_sp(&mut self) {
        self.m = 4;

        self.reg.sp = self
            .reg
            .sp
            .wrapping_add(self.mmu.read_byte(self.reg.pc.into()) as i8 as i16 as u16);
        self.unset_flag(Flags::Operation);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.sp > 0x07FF);
        self.set_flag_on_if(Flags::Carry, self.reg.sp > 0x7FF);
        self.reg.pc += 1;
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

        self.mmu
            .write_byte((self.mmu.read_word(self.reg.pc.into())).into(), self.reg.a);
        self.reg.pc += 2;
    }

    // bitwise xor a and 8-bit immediate operand
    fn xor_d8(&mut self) {
        self.m = 2;

        self.reg.a ^= self.mmu.read_byte(self.reg.pc.into());
        self.unset_flag(Flags::Operation);
        self.unset_flag(Flags::HalfCarry);
        self.unset_flag(Flags::Carry);
        self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
        self.reg.pc += 1;
    }

    // call adress
    fn rst_five(&mut self) {
        self.reg.pc += 1;
        self.m = 4;

        self.reg.sp -= 2;
        self.mmu.write_word(self.reg.sp.into(), self.reg.pc);
        self.reg.pc = 0x28;
    }

    // load into register A the contents of the internal ram, port register or mode register
    fn ld_a_a8(&mut self) {
        self.m = 3;
        self.reg.a = self
            .mmu
            .read_byte((0xFF00 | self.mmu.read_byte(self.reg.pc.into()) as u16).into());
    }

    // pop contents of the memory stack into register pair AF
    fn pop_af(&mut self) {
        self.reg.pc += 1;
        self.m = 3;

        self.reg
            .set_af(self.mmu.read_word(self.reg.sp.into()) & 0xFFF0);
        self.reg.sp += 2;
    }

    // load into register A the contents of internal ram, port register or mode register
    fn ld_a_c_addr(&mut self) {
        self.m = 2;
        self.reg.a = self.mmu.read_byte((0xFF00 | self.reg.c as u16).into());
    }

    // reset interrupt master enable(IME) flag and prohibit maskable interrupts
    fn di(&mut self) {
        self.m = 1;
        self.di = true;
    }

    // push contents of register pair AF onto the memory stack
    fn push_af(&mut self) {
        self.reg.pc += 1;
        self.m = 4;

        self.reg.sp -= 2;
        self.mmu.write_word(self.reg.sp.into(), self.reg.get_af());
    }

    // store bitwise OR of 8-bit immediate operand and register A
    fn or_d8(&mut self) {
        self.m = 2;

        self.reg.a |= self.mmu.read_byte(self.reg.pc.into());
        self.unset_flag(Flags::Operation);
        self.unset_flag(Flags::HalfCarry);
        self.unset_flag(Flags::Carry);
        self.set_flag_on_if(Flags::Zero, self.reg.a == 0);
        self.reg.pc += 1;
    }

    // call adress
    fn rst_six(&mut self) {
        self.m = 4;

        self.reg.sp -= 2;
        self.mmu.write_word(self.reg.sp.into(), self.reg.pc);
        self.reg.pc = 0x30;
    }

    // add 8-bit signed to sp and store in register pair HL
    fn ld_hl_sp_s8(&mut self) {
        self.m = 3;

        let operand = self.mmu.read_byte(self.reg.pc.into()) as i8 as i16 as u16;
        self.reg.set_hl(self.reg.sp.wrapping_add(operand));
        self.unset_flag(Flags::Zero);
        self.unset_flag(Flags::Operation);
        self.set_flag_on_if(Flags::HalfCarry, self.reg.get_hl() > 0x07FF);
        self.set_flag_on_if(Flags::Carry, self.reg.get_hl() > 0x7FF);
        self.reg.pc += 1;
    }

    // load contents of register pair HL into sp
    fn ld_sp_hl(&mut self) {
        self.m = 2;

        self.reg.sp = self.reg.get_hl();
    }

    // load contents of internal ram or register specified
    // by 16-bit immediate operand into register A
    // TODO: confirm it works
    fn ld_a_a16(&mut self) {
        self.reg.pc += 3;
        self.m = 4;

        self.reg.a = self
            .mmu
            .read_byte((self.mmu.read_word(self.reg.pc.into())).into());
    }

    // set the interrupt master enable(IME) flag and
    // enable maskable interrupts
    // TODO: confirm it works
    fn ei(&mut self) {
        self.m = 1;
        self.ei = true;
    }

    // compare contents of register A and 8-bit immediate operand
    fn cp_d8(&mut self) {
        self.m = 2;

        let result = self
            .reg
            .a
            .wrapping_sub(self.mmu.read_byte(self.reg.pc as usize));
        self.reg.a = result;
        self.set_flag(Flags::Operation);
        self.set_flag_on_if(Flags::Zero, result == 0);
        self.set_flag_on_if(Flags::HalfCarry, result > 0xF);
        self.set_flag_on_if(
            Flags::Carry,
            self.mmu.working_ram[self.reg.pc as usize] > self.reg.a,
        );
        self.reg.pc += 1;
    }

    // call address
    fn rst_seven(&mut self) {
        self.m = 4;

        self.reg.sp -= 2;
        self.mmu.write_word(self.reg.sp.into(), self.reg.pc);
        self.reg.pc = 0x38;
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
            0xCC => self.call_z(),
            0xCD => self.call(),
            0xCE => self.adc_a(),
            0xCF => self.rst_one(),
            0xD0 => self.ret_nc(),
            0xD1 => self.pop_de(),
            0xD2 => self.jp_nc(),
            0xD4 => self.call_nc(),
            0xD5 => self.push_de(),
            0xD6 => self.sub(),
            0xD7 => self.rst_two(),
            0xD8 => self.ret_c(),
            0xD9 => self.reti(),
            0xDA => self.jp_c(),
            0xDC => self.call_c(),
            0xDE => self.sbc_a(),
            0xDF => self.rst_three(),
            0xE0 => self.ld_addr_a(),
            0xE1 => self.pop_hl(),
            0xE2 => self.ld_addr_c_a(),
            0xE5 => self.push_hl(),
            0xE6 => self.and_a(),
            0xE7 => self.rst_four(),
            0xE8 => self.add_sp(),
            0xE9 => self.jp_hl(),
            0xEA => self.ld_addr_a16_a(),
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
            0xFE => self.cp_d8(),
            0xFF => self.rst_seven(),
            _ => println!("{:#X} is not a recognized opcode...", self.current_opcode),
        }
        println!(" {:#X}", self.current_opcode);
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
        let expected_pc = cpu.reg.pc + 1;

        // Act
        cpu.nop();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_pc, cpu.reg.pc);
    }

    #[test]
    fn test_load_bc() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 3;
        let expected_pc = cpu.reg.pc + 3;
        // 244 << 8 | 128
        let expected_bc: u16 = 62592;
        cpu.mmu.working_ram[expected_pc as usize] = 244;
        cpu.mmu.working_ram[expected_pc as usize + 1] = 128;

        // Act
        cpu.load_bc();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_bc, cpu.reg.get_bc());
    }

    #[test]
    fn test_load_bc_a() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 2;
        let expected_pc = cpu.reg.pc + 1;
        let register_a = 235;

        // Act
        cpu.reg.a = register_a;
        cpu.load_bc_a();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(register_a, cpu.mmu.working_ram[cpu.reg.get_bc() as usize]);
    }

    #[test]
    fn test_inc_bc() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 2;
        let expected_pc = cpu.reg.pc + 1;
        let expected_register_bc = cpu.reg.get_bc() + 1;

        // Act
        cpu.inc_bc();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
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
        let expected_pc = cpu.reg.pc + 1;
        let expected_register_b_value = 1;
        let expected_register_f_value = 0x0;

        // Act
        cpu.inc_b();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
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
        let expected_pc = cpu.reg.pc + 1;
        let expected_register_b_value = 4;
        let expected_register_f_value = u8::from(Flags::Operation);

        // Act
        cpu.reg.b = 5;
        cpu.dec_b();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
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
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_b_load_value, cpu.reg.b);
    }

    #[test]
    fn test_rlca() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 1;
        let expected_pc = cpu.reg.pc + 1;
        let expected_register_f_value = 0;
        let expected_register_a_value: u8 = 4;
        let expected_register_a_value_with_carry = 1;

        // Act
        cpu.reg.a = 2;
        cpu.rlca();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
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
        let expected_pc = cpu.reg.pc + 3;
        // based on setting the sp at 32678
        let expected_sp_lower_byte = 166;
        let expected_sp_upper_byte = 127;

        // Act
        cpu.reg.sp = 32678;
        cpu.load_sp_at_addr();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
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
        let expected_pc = cpu.reg.pc + 1;
        let expected_a = 255;
        cpu.mmu.working_ram[cpu.reg.get_bc() as usize] = expected_a;

        // Act
        cpu.ld_a_bc();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_a, cpu.mmu.working_ram[cpu.reg.get_bc() as usize]);
    }

    #[test]
    fn test_dec_bc() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 2;
        let expected_pc = cpu.reg.pc + 1;
        let expected_bc = 9;
        cpu.reg.set_bc(10);

        // Act
        cpu.dec_bc();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_bc, cpu.reg.get_bc());
    }

    #[test]
    fn test_inc_c() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 1;
        let expected_pc = cpu.reg.pc + 1;
        let expected_c = 241;
        cpu.reg.c = 240;

        // Act
        cpu.inc_c();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
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
        let expected_pc = cpu.reg.pc + 1;
        let expected_c = 24;
        cpu.reg.c = 25;

        // Act
        cpu.dec_c();

        assert_eq!(expected_m_cycles, cpu.m);
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
        let expected_pc = cpu.reg.pc + 2;
        let expected_c = 25;
        cpu.mmu.working_ram[expected_pc as usize] = 25;

        // Act
        cpu.ld_c();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_c, cpu.reg.c);
    }

    #[test]
    fn test_rrca() {
        // Arrange
        let mut cpu = Cpu::new();
        let expected_m_cycles = 1;
        let expected_pc = cpu.reg.pc + 1;
        let expected_register_f = u8::from(Flags::None);
        // 240 >> 1
        let expected_a = 120;
        cpu.reg.a = 240;

        // Act
        cpu.rrca();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
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
        let expected_pc = cpu.reg.pc + 3;
        // 244 << 8 | 128
        let expected_de: u16 = 62592;
        cpu.mmu.working_ram[expected_pc as usize] = 244;
        cpu.mmu.working_ram[expected_pc as usize + 1] = 128;

        // Act
        cpu.ld_de();

        // Assert
        assert_eq!(expected_m_cycles, cpu.m);
        assert_eq!(expected_pc, cpu.reg.pc);
        assert_eq!(expected_de, cpu.reg.get_de());
    }
}
