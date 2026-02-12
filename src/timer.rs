use std::cmp::min;

// built-in timer in the gameboy

// TIMA timer updates at a configurable rate, depends on frequency set in TAC register
// when TIMA overflows an interrupt is issued and TIMA is reset to TMA's value
// should only increment timer if timer is enabled in TAC register
// NOTE: we are doing machine cycles and not clock cycles

// TODO: check if timer bit is active or not in tac

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
    div_counter: u16,

    // enable interrupt
    pub interrupt: bool,
    tima_reload_delay: Option<u16>,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            div: 0,
            tima: 0,
            tma: 0,
            tac: 0,
            div_counter: 0,
            interrupt: false,
            tima_reload_delay: None,
        }
    }

    fn timer_enabled(&self) -> bool {
        self.tac & 0x04 != 0
    }

    fn get_div_bit(&self, tac: u8) -> u8 {
        match tac & 0x03 {
            0 => 9, // 4096 Hz (1024 T-cycles)
            1 => 3, // 262144 Hz (16 T-cycles)
            2 => 5, // 65536 Hz (64 T-cycles)
            3 => 7, // 16384 Hz (256 T-cycles)
            _ => 9,
        }
    }

    pub fn update(&mut self, m_cycles: u8) {
        // Step 4 T-cycles at a time (1 M-cycle) to avoid missing falling
        // edges on the fastest timer frequency (bit 3, period 16 T-cycles).
        // Bulk-adding all T-cycles at once can jump over an entire bit period,
        // causing the edge detection to see no change.
        for _ in 0..m_cycles {
            let old_div_counter = self.div_counter;
            self.div_counter = self.div_counter.wrapping_add(4);
            self.div = (self.div_counter >> 8) as u8;

            let bit = self.get_div_bit(self.tac);
            let old_bit = ((old_div_counter >> bit) & 1) != 0;
            let new_bit = ((self.div_counter >> bit) & 1) != 0;
            let timer_enabled = self.timer_enabled();

            if timer_enabled && old_bit && !new_bit {
                if self.tima == 0xFF {
                    self.tima = 0;
                    self.tima_reload_delay = Some(4);
                } else {
                    self.tima = self.tima.wrapping_add(1);
                }
            }

            if let Some(ref mut delay) = self.tima_reload_delay {
                let dec = min(*delay, 4);
                *delay -= dec;
                if *delay == 0 {
                    self.tima = self.tma;
                    self.interrupt = true;
                    self.tima_reload_delay = None;
                }
            }
        }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0xFF04 => self.div,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            0xFF07 => self.tac,
            _ => panic!("timer.read_byte() went wrong at: {addr}"),
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF04 => {
                let bit = self.get_div_bit(self.tac);
                let old_bit = ((self.div_counter >> bit) & 1) != 0;
                self.div_counter = 0;
                self.div = 0;
                let new_bit = false;
                if self.timer_enabled() && old_bit && !new_bit {
                    if self.tima == 0xFF {
                        self.tima = 0;
                        self.tima_reload_delay = Some(4);
                    } else {
                        self.tima = self.tima.wrapping_add(1);
                    }
                }
            }
            0xFF05 => {
                self.tima = value;
                if self.tima_reload_delay.is_some() {
                    self.tima_reload_delay = None;
                }
            }
            0xFF06 => {
                self.tma = value;
            }
            0xFF07 => {
                let old_bit = ((self.div_counter >> self.get_div_bit(self.tac)) & 1) != 0;
                let old_enabled = self.timer_enabled();
                let new_tac = value & 0x7;
                let new_bit = ((self.div_counter >> self.get_div_bit(new_tac)) & 1) != 0;
                let new_enabled = new_tac & 0x4 != 0;
                if old_enabled && old_bit && (!new_enabled || !new_bit) {
                    if self.tima == 0xFF {
                        self.tima = 0;
                        self.tima_reload_delay = Some(4);
                    } else {
                        self.tima = self.tima.wrapping_add(1);
                    }
                }
                self.tac = new_tac;
            }
            _ => panic!("timer.write_byte() went wrong at: {addr}"),
        }
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}
