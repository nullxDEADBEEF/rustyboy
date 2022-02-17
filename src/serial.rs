pub struct Serial {
    pub data: u8, // TODO: make private when done testing
    pub control: u8,
}

impl Serial {
    pub fn new() -> Self {
        Self {
            data: 0,
            control: 0,
        }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0xFF01 => self.data,
            0xFF02 => self.control,
            _ => panic!("Serial read error at address: {}", addr),
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF01 => self.data = value,
            0xFF02 => {
                self.control = value;
                if value == 0x81 {
                    self.data = value;
                    println!("Serial data stuff!!!!");
                }
            }
            _ => panic!("Serial write error at address: {}", addr),
        }
    }
}

impl Default for Serial {
    fn default() -> Self {
        Self::new()
    }
}
