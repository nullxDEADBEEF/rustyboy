use blip_buf::BlipBuf;

pub struct Apu {
    enabled: bool, // master power
    nr50: u8,      // master volume register
    nr51: u8,      // sound panning register
    ch1: SquareChannel,
    ch2: SquareChannel,

    blip_left: BlipBuf,
    blip_right: BlipBuf,
    t_clock: u32, // t-cycles since last frame
}

const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1], // 12.5%
    [0, 0, 0, 0, 0, 0, 1, 1], // 25%
    [0, 0, 0, 0, 1, 1, 1, 1], // 50%
    [1, 1, 1, 1, 1, 1, 0, 0], // 75%
];

impl Apu {
    pub fn new(sample_rate: u32) -> Self {
        let mut apu = Self {
            enabled: false,
            nr50: 0x00,
            nr51: 0x00,
            ch1: SquareChannel::new(),
            ch2: SquareChannel::new(),
            blip_left: BlipBuf::new(sample_rate / 60 + 100), // one frame plus padding
            blip_right: BlipBuf::new(sample_rate / 60 + 100),
            t_clock: 0,
        };

        apu.blip_left.set_rates(4_194_304.0, sample_rate as f64);
        apu.blip_right.set_rates(4_194_304.0, sample_rate as f64);

        apu
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        if !self.enabled && addr != 0xFF26 && !(0xFF30..=0xFF3F).contains(&addr) {
            return 0xFF; // APU disabled, most registers read as 0xFF
        }

        match addr {
            0xFF16 => (self.ch2.duty_cycle << 6) | 0x3F,
            0xFF17 => {
                (self.ch2.initial_volume << 4)
                    | ((self.ch2.env_add_mode as u8) << 3)
                    | self.ch2.env_period
            }
            0xFF18 => 0xFF, // read-only
            0xFF19 => 0xBF | ((self.ch2.length_enable as u8) << 6),
            0xFF24 => self.nr50,
            0xFF25 => self.nr51,
            0xFF26 => 0x70 | ((self.enabled as u8) << 7),
            _ => 0xFF,
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        if !self.enabled && addr != 0xFF26 && !(0xFF30..=0xFF3F).contains(&addr) {
            return; // APU disabled, ignore writes except power and wave RAM
        }

        match addr {
            0xFF16 => {
                self.ch2.duty_cycle = (value >> 6) & 0x03;
                self.ch2.length_load = value & 0x3F;
            }
            0xFF17 => {
                self.ch2.initial_volume = (value >> 4) & 0x0F;
                self.ch2.env_add_mode = value & 0x08 != 0;
                self.ch2.env_period = value & 0x07;
                self.ch2.dac_enabled = value & 0xF8 != 0;
                if !self.ch2.dac_enabled {
                    self.ch2.enabled = false;
                }
            }
            0xFF18 => {
                self.ch2.freq_low = value;
            }
            0xFF19 => {
                self.ch2.length_enable = value & 0x40 != 0;
                self.ch2.freq_high = value & 0x07;
                if value & 0x80 != 0 && self.ch2.dac_enabled {
                    self.ch2.enabled = true;
                    self.ch2.current_volume = self.ch2.initial_volume;
                    let freq = (self.ch2.freq_high as u16) << 8 | self.ch2.freq_low as u16;
                    self.ch2.freq_timer = (2048 - freq) * 4;
                    self.ch2.duty_position = 0;
                }
            }
            0xFF24 => {
                self.nr50 = value;
            }
            0xFF25 => {
                self.nr51 = value;
            }
            0xFF26 => {
                if value & 0x80 == 0 {
                    self.enabled = false;
                    self.nr50 = 0x00;
                    self.nr51 = 0x00;
                } else {
                    self.enabled = true;
                }
            }
            _ => {}
        }
    }

    pub fn step(&mut self, cycles: u8) {
        let t_cycles = cycles * 4;

        for _ in 0..t_cycles {
            self.t_clock += 1;

            if let Some(amp_delta) = self.ch2.tick() {
                if self.nr51 & 0x20 != 0 {
                    self.blip_left.add_delta(self.t_clock, amp_delta);
                }
                if self.nr51 & 0x02 != 0 {
                    self.blip_right.add_delta(self.t_clock, amp_delta);
                }
            }
        }
    }

    pub fn end_frame(&mut self) -> Vec<i16> {
        self.blip_left.end_frame(self.t_clock);
        self.blip_right.end_frame(self.t_clock);

        self.t_clock = 0;

        let left_available_samples = self.blip_left.samples_avail();
        let right_available_samples = self.blip_right.samples_avail();

        let mut left_buf = vec![0i16; left_available_samples as usize];
        let mut right_buf = vec![0i16; right_available_samples as usize];

        self.blip_left.read_samples(&mut left_buf, false);
        self.blip_right.read_samples(&mut right_buf, false);

        let interleaved_samples = left_buf
            .iter()
            .zip(right_buf.iter())
            .flat_map(|(l, r)| [*l, *r])
            .collect();

        interleaved_samples
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nr52_power_on_off() {
        let mut apu = Apu::new(256);

        // power on
        apu.write_byte(0xFF26, 0x80);
        assert_eq!(apu.read_byte(0xFF26) & 0x80, 0x80);

        // write nr50/nr51 while powered on
        apu.write_byte(0xFF24, 0x77);
        assert_eq!(apu.read_byte(0xFF24), 0x77);

        // power off should zero registers
        apu.write_byte(0xFF26, 0x00);
        assert_eq!(apu.read_byte(0xFF26) & 0x80, 0x00);
        assert_eq!(apu.read_byte(0xFF24), 0xFF); // disabled -> reads as 0xFF
    }

    #[test]
    fn ch2_tick_advances_duty_and_produces_amplitude() {
        let mut apu = Apu::new(256);

        // Power on APU
        apu.write_byte(0xFF26, 0x80);
        // NR21: duty 50% (0b10), length 0
        apu.write_byte(0xFF16, 0x80);
        // NR22: volume 15, no envelope
        apu.write_byte(0xFF17, 0xF0);
        // NR23: freq low = 0x00
        apu.write_byte(0xFF18, 0x00);
        // NR24: trigger + freq high = 7 → freq = 0x700 = 1792, timer = (2048-1792)*4 = 1024
        apu.write_byte(0xFF19, 0x87);

        assert!(apu.ch2.enabled);
        assert_eq!(apu.ch2.duty_position, 0);
        assert_eq!(apu.ch2.freq_timer, 1024);

        // Step 1024 T-cycles (256 M-cycles) to expire the timer once and advance duty_position
        apu.step(0); // no-op, just to be safe
                     // 256 M-cycles = 1024 T-cycles → freq_timer hits 0, duty_position goes from 0 to 1
        for _ in 0..256 {
            apu.step(1);
        }

        assert_eq!(apu.ch2.duty_position, 1);
        // duty 50% position 1 = 0, so amplitude should be 0
        assert_eq!(apu.ch2.last_amp, 0);

        // Step another 1024 T-cycles → duty_position goes to 2
        for _ in 0..256 {
            apu.step(1);
        }

        assert_eq!(apu.ch2.duty_position, 2);

        // Step two more periods → duty_position 3, then 4
        for _ in 0..512 {
            apu.step(1);
        }

        assert_eq!(apu.ch2.duty_position, 4);
        // duty 50% position 4 = 1, volume 15 → amplitude = 1 * 15 * 256 = 3840
        assert_eq!(apu.ch2.last_amp, 3840);
    }
}

struct SquareChannel {
    duty_cycle: u8,
    length_load: u8,
    initial_volume: u8,
    env_add_mode: bool,
    env_period: u8,
    freq_low: u8,
    freq_high: u8,
    length_enable: bool,

    enabled: bool,
    dac_enabled: bool,
    freq_timer: u16,
    duty_position: u8,
    current_volume: u8,
    last_amp: i32,
}

impl SquareChannel {
    fn new() -> Self {
        Self {
            duty_cycle: 0,
            length_load: 0,
            initial_volume: 0,
            env_add_mode: false,
            env_period: 0,
            freq_low: 0,
            freq_high: 0,
            length_enable: false,

            enabled: false,
            dac_enabled: false,
            freq_timer: 0,
            duty_position: 0,
            current_volume: 0,
            last_amp: 0,
        }
    }

    fn tick(&mut self) -> Option<i32> {
        if self.freq_timer > 0 {
            self.freq_timer -= 1;
        }

        if self.freq_timer == 0 {
            let freq = (self.freq_high as u16) << 8 | self.freq_low as u16;
            self.freq_timer = (2048 - freq) * 4;
            self.duty_position = (self.duty_position + 1) & 7;
        }

        let amplitude: i32 = if self.enabled && self.dac_enabled {
            let duty = DUTY_TABLE[self.duty_cycle as usize][self.duty_position as usize] as i32;
            duty * self.current_volume as i32 * 256
        } else {
            0
        };

        if amplitude != self.last_amp {
            let amp_delta = amplitude - self.last_amp;
            self.last_amp = amplitude;

            return Some(amp_delta);
        }

        None
    }
}
