pub struct Joypad {
    select: u8,
    pub action_state: u8,
    pub direction_state: u8,
}

impl Joypad {
    pub fn new() -> Self {
        Self {
            select: 0xFF,
            action_state: 0xF,
            direction_state: 0xF,
        }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        let mut lower = 0x0F;

        match addr {
            0xFF00 => {
                if self.select & 0x20 == 0 {
                    lower &= self.action_state
                }
                if self.select & 0x10 == 0 {
                    lower &= self.direction_state
                }

                (self.select & 0x30) | lower
            },
            _ => 0xFF,
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        if addr == 0xFF00 {
            self.select = value;
        }
    }
}