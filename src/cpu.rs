use crate::{mmu::Mmu, register::Register};

// memory interface can address up to 65536 bytes (16-bit bus)
// programs are accessed through the same address bus as normal memory
// instruction size can be between one and three bytes

// timings assume a CPU frequency of 4.19 MHz, called "T-states"
// because timings are divisble by 4 many specify timings and clock frequency divided by 4, called "M-cycles"

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
    ei: bool,
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
            ei: false,
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

        // load one byte into b
        self.reg.b = self.mmu.working_ram[(self.current_opcode << 8) as usize];
        // load one byte into c
        self.reg.c = self.mmu.working_ram[self.current_opcode as usize];
    }

    // load data from register A to the register pair BC
    fn load_bc_a(&mut self) {
        self.m = 2;
        self.t = 8;

        self.reg.b = self.reg.a;
        self.reg.c = self.reg.a;

        self.reg.pc += 1;
    }

    // increment register pair BC
    fn inc_bc(&mut self) {
        self.reg.pc += 1;
        self.m = 2;
        self.t = 8;

        self.reg.b += 1;
        self.reg.c += 1;
    }

    // increment register B
    fn inc_b(&mut self) {
        self.reg.pc += 1;
        self.m = 1;
        self.t = 4;

        self.reg.b += 1;
    }

    pub fn decode_execute(&mut self) {
        // something along these lines
        self.current_opcode = self.mmu.read_byte(self.reg.pc as u8);
        println!("{}", self.current_opcode);
        // match opcode {
        //     0x00 => self.nop(),
        //     0x01 => self.load_bc(),
        //     0x02 => self.load_bc_a(),
        //     0x03 => self.inc_bc(),
        //     0x04 => self.inc_b(),
        //     _ => println!("not a recognized instruction"),
        // }
    }
}
