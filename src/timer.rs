// built-in timer in the gameboy

// TIMA timer updates at a configurable rate, depends on frequency set in TAC register
// when TIMA overflows an interrupt is issued and TIMA is reset to TMA's value
// should only increment timer if timer is enabled in TAC register
// NOTE: we are doing machine cycles and not clock cycles

// TODO: check if timer bit is active or not in tac

const MAX_M_CYCLES_FOR_OPCODE: u8 = 4;

pub struct Clock {
    primary: u32,
    div: u32,
    tima: u32,
    instr_cycles: u32,
}

pub struct Timer {
    // divider register
    // used to update sweep(channel 1), fade in/out
    div: u8,
    // timer counter
    // updates at a specific rate, 16384 Hz
    // cpu is 4.12 Mhz => 4194304 Hz / 16384 Hz = 256 clock cycles
    // in machine cycles: 262144 Hz / 16384 Hz = 16 machines cycles
    tima: u8,
    // timer modulo
    tma: u8,
    // timer control
    tac: u8,
    // enable interrupt
    pub interrupt: bool,

    speed: u8,
    pub clock: Clock,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            div: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            interrupt: false,

            speed: 0,
            clock: Clock {
                primary: 0,
                div: 0,
                tima: 0,
                instr_cycles: 0,
            },
        }
    }

    pub fn update(&mut self, opcode_cycles: u8) {
        // check if 4 m-cycles have occured
        // since no opcode takes more than 4 m-cycles
        self.clock.instr_cycles += opcode_cycles as u32;
        if self.clock.instr_cycles >= MAX_M_CYCLES_FOR_OPCODE as u32 {
            self.clock.primary += 1;
            self.clock.div += 1;
            self.clock.instr_cycles -= MAX_M_CYCLES_FOR_OPCODE as u32;
            if self.clock.div == 0x10 {
                self.div = self.div.wrapping_add(1);
                self.clock.div = 0;
            }
        }

        self.get_clock_speed();

        // increment timer(tima) by 1 when primary clock surpasses timer speed
        if self.clock.primary >= self.speed as u32 {
            self.clock.primary = 0;
            self.tima += 1;
            if self.clock.tima > 0xFF {
                println!("INTERRUPT NOOOOOOOOW");
                self.tima = self.tma;
                self.clock.tima -= 0xFF;
                self.interrupt = true;
            }
        }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0xFF04 => self.div,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            0xFF07 => self.speed,
            _ => panic!("timer.read_byte() went wrong at: {}", addr),
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF04 => self.div = 0x00,
            0xFF05 => self.tima = value,
            0xFF06 => self.tma = value,
            0xFF07 => self.tac = value & 0x7,
            _ => panic!("timer.write_byte() went wrong at: {}", addr),
        }
    }

    fn get_clock_speed(&mut self) {
        self.speed = match self.tac & 0x3 {
            0x00 => 0x40,
            0x01 => 0x1,
            0x02 => 0x4,
            0x03 => 0x10,
            _ => panic!("not valid tac speeds"),
        }
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}
