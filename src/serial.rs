use std::io::Write;

pub struct Serial {
    pub data: u8,
    pub control: u8,
    transfer_in_progress: bool,
    transfer_cycles: u16,
}

impl Serial {
    pub fn new() -> Self {
        Self {
            data: 0,
            control: 0,
            transfer_in_progress: false,
            transfer_cycles: 0,
        }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0xFF01 => self.data,
            0xFF02 => self.control,
            _ => panic!("Serial read error at address: {addr}"),
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF01 => {
                self.data = value;
            }
            0xFF02 => {
                self.control = value;
                if value & 0x81 == 0x81 {
                    let c = self.data as char;
                    print!("{c}");
                    std::io::stdout().flush().unwrap();
                    self.control &= !0x80;
                }
            }
            _ => panic!("Serial write error at address: {addr}"),
        }
    }

    pub fn step(&mut self, cycles: u8) -> bool {
        if self.transfer_in_progress {
            self.transfer_cycles = self.transfer_cycles.saturating_sub(cycles as u16);
            if self.transfer_cycles == 0 {
                self.transfer_in_progress = false;
                self.control = 0;
                return true; // Signal that interrupt should be set
            }
        }
        false
    }
}

impl Default for Serial {
    fn default() -> Self {
        Self::new()
    }
}
