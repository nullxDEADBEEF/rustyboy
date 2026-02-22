use blip_buf::BlipBuf;

pub struct Apu {
    enabled: bool, // master power
    nr50: u8,      // master volume register
    nr51: u8,      // sound panning register
    ch1: SquareChannel,
    ch2: SquareChannel,
    ch3: WaveChannel,

    frame_sequencer_counter: u32,
    frame_sequencer_step: u8,

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
            ch3: WaveChannel::new(),
            frame_sequencer_counter: 0,
            frame_sequencer_step: 0,
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
            0xFF10 => {
                0x80 | (self.ch1.sweep_period << 4)
                    | ((self.ch1.sweep_negate as u8) << 3)
                    | self.ch1.sweep_shift
            }
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
            0xFF26 => {
                0x70 | ((self.enabled as u8) << 7)
                    | ((self.ch3.enabled as u8) << 2)
                    | ((self.ch2.enabled as u8) << 1)
                    | (self.ch1.enabled as u8)
            }
            0xFF1A => 0x7F | ((self.ch3.dac_enabled as u8) << 7),
            0xFF1B => 0xFF,
            0xFF1C => 0x9F | (self.ch3.volume_code << 5),
            0xFF1D => 0xFF,
            0xFF1E => 0xBF | ((self.ch3.length_enable as u8) << 6),
            0xFF30..=0xFF3F => self.ch3.wave_ram[(addr - 0xFF30) as usize],
            _ => 0xFF,
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        if !self.enabled && addr != 0xFF26 && !(0xFF30..=0xFF3F).contains(&addr) {
            return; // APU disabled, ignore writes except power and wave RAM
        }

        match addr {
            0xFF10 => {
                self.ch1.sweep_period = (value >> 4) & 0x07;
                self.ch1.sweep_negate = value & 0x08 != 0;
                self.ch1.sweep_shift = value & 0x07;
            }
            0xFF11 => {
                self.ch1.duty_cycle = (value >> 6) & 0x03;
                self.ch1.length_load = value & 0x3F;
            }
            0xFF12 => {
                self.ch1.initial_volume = (value >> 4) & 0x0F;
                self.ch1.env_add_mode = value & 0x08 != 0;
                self.ch1.env_period = value & 0x07;
                self.ch1.dac_enabled = value & 0xF8 != 0;
                if !self.ch1.dac_enabled {
                    self.ch1.enabled = false;
                }
            }
            0xFF13 => {
                self.ch1.freq_low = value;
            }
            0xFF14 => {
                self.ch1.length_enable = value & 0x40 != 0;
                self.ch1.freq_high = value & 0x07;
                if value & 0x80 != 0 && self.ch1.dac_enabled {
                    self.ch1.enabled = true;
                    self.ch1.current_volume = self.ch1.initial_volume;
                    self.ch1.env_timer = self.ch1.env_period;

                    if self.ch1.length_timer == 0 {
                        self.ch1.length_timer = 64;
                    }

                    let freq = (self.ch1.freq_high as u16) << 8 | self.ch1.freq_low as u16;
                    self.ch1.freq_timer = (2048 - freq) * 4;
                    self.ch1.duty_position = 0;

                    self.ch1.shadow_freq = freq;

                    self.ch1.sweep_timer = if self.ch1.sweep_period > 0 {
                        self.ch1.sweep_period
                    } else {
                        8
                    };

                    if self.ch1.sweep_period != 0 || self.ch1.sweep_shift != 0 {
                        self.ch1.sweep_enabled = true;
                    }

                    if self.ch1.sweep_shift != 0 {
                        let new_freq =
                            self.ch1.shadow_freq + (self.ch1.shadow_freq >> self.ch1.sweep_shift);
                        if new_freq > 2047 {
                            self.ch1.enabled = false;
                        }
                    }
                }
            }
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
                    self.ch2.env_timer = self.ch2.env_period;

                    if self.ch2.length_timer == 0 {
                        self.ch2.length_timer = 64;
                    }

                    let freq = (self.ch2.freq_high as u16) << 8 | self.ch2.freq_low as u16;
                    self.ch2.freq_timer = (2048 - freq) * 4;
                    self.ch2.duty_position = 0;
                }
            }
            0xFF1A => {
                self.ch3.dac_enabled = (value & 0x80) != 0;
            }
            0xFF1B => {
                self.ch3.length_load = value as u16;
            }
            0xFF1C => {
                self.ch3.volume_code = (value >> 5) & 0x03;
            }
            0xFF1D => {
                self.ch3.freq_low = value;
            }
            0xFF1E => {
                self.ch3.length_enable = value & 0x40 != 0;
                self.ch3.freq_high = value & 0x07;

                if value & 0x80 != 0 {
                    if self.ch3.dac_enabled {
                        self.ch3.enabled = true;
                    }

                    if self.ch3.length_timer == 0 {
                        self.ch3.length_timer = 256 - self.ch3.length_load;
                    }

                    let freq = (self.ch3.freq_high as u16) << 8 | self.ch3.freq_low as u16;
                    self.ch3.freq_timer = (2048 - freq) * 2;
                    self.ch3.position = 0;
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
            0xFF30..=0xFF3F => self.ch3.wave_ram[(addr - 0xFF30) as usize] = value,
            _ => {}
        }
    }

    pub fn step(&mut self, cycles: u8) {
        let t_cycles = cycles * 4;
        let should_advance_frame_sequencer_step = 4_194_304 / 512;

        for _ in 0..t_cycles {
            self.frame_sequencer_counter += 1;
            self.t_clock += 1;

            if self.frame_sequencer_counter == should_advance_frame_sequencer_step {
                self.frame_sequencer_counter = 0;

                if self.frame_sequencer_step % 2 == 0 {
                    self.ch1.clock_length();
                    self.ch2.clock_length();
                    self.ch3.clock_length();
                }
                if self.frame_sequencer_step == 2 || self.frame_sequencer_step == 6 {
                    self.ch1.clock_sweep();
                }
                if self.frame_sequencer_step == 7 {
                    self.ch1.clock_envelope();
                    self.ch2.clock_envelope();
                }

                self.frame_sequencer_step = (self.frame_sequencer_step + 1) & 7;
            }

            if let Some(amp_delta) = self.ch1.tick() {
                if self.nr51 & 0x10 != 0 {
                    self.blip_left.add_delta(self.t_clock, amp_delta);
                }
                if self.nr51 & 0x01 != 0 {
                    self.blip_right.add_delta(self.t_clock, amp_delta);
                }
            }

            if let Some(amp_delta) = self.ch2.tick() {
                if self.nr51 & 0x20 != 0 {
                    self.blip_left.add_delta(self.t_clock, amp_delta);
                }
                if self.nr51 & 0x02 != 0 {
                    self.blip_right.add_delta(self.t_clock, amp_delta);
                }
            }

            if let Some(amp_delta) = self.ch3.tick() {
                if self.nr51 & 0x40 != 0 {
                    self.blip_left.add_delta(self.t_clock, amp_delta);
                }
                if self.nr51 & 0x04 != 0 {
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
    length_timer: u8,
    initial_volume: u8,
    env_add_mode: bool,
    env_period: u8,
    env_timer: u8,
    freq_low: u8,
    freq_high: u8,
    length_enable: bool,

    enabled: bool,
    dac_enabled: bool,
    freq_timer: u16,
    duty_position: u8,
    current_volume: u8,
    last_amp: i32,

    sweep_period: u8,
    sweep_shift: u8,
    sweep_negate: bool,
    sweep_timer: u8,
    sweep_enabled: bool,
    shadow_freq: u16, // internal copy of frequency used for sweep calculations
}

impl SquareChannel {
    fn new() -> Self {
        Self {
            duty_cycle: 0,
            length_load: 0,
            length_timer: 0,
            initial_volume: 0,
            env_add_mode: false,
            env_period: 0,
            env_timer: 0,
            freq_low: 0,
            freq_high: 0,
            length_enable: false,

            enabled: false,
            dac_enabled: false,
            freq_timer: 0,
            duty_position: 0,
            current_volume: 0,
            last_amp: 0,

            sweep_period: 0,
            sweep_shift: 0,
            sweep_negate: false,
            sweep_timer: 0,
            sweep_enabled: false,
            shadow_freq: 0,
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

    fn clock_length(&mut self) {
        if self.length_enable && self.length_timer > 0 {
            self.length_timer -= 1;

            if self.length_timer == 0 {
                self.enabled = false;
            }
        }
    }

    fn clock_envelope(&mut self) {
        if self.env_period == 0 {
            return;
        }

        if self.env_timer > 0 {
            self.env_timer -= 1;
        }

        if self.env_timer == 0 {
            self.env_timer = self.env_period;

            if self.env_add_mode {
                if self.current_volume < 15 {
                    self.current_volume += 1;
                }
            }

            if !self.env_add_mode {
                if self.current_volume > 0 {
                    self.current_volume -= 1;
                }
            }
        }
    }

    fn clock_sweep(&mut self) {
        if self.sweep_enabled && self.sweep_period > 0 {
            if self.sweep_timer > 0 {
                self.sweep_timer -= 1;
            }

            if self.sweep_timer == 0 {
                self.sweep_timer = self.sweep_period;

                let new_freq = if self.sweep_negate {
                    self.shadow_freq - (self.shadow_freq >> self.sweep_shift)
                } else {
                    self.shadow_freq + (self.shadow_freq >> self.sweep_shift)
                };

                if new_freq > 2047 {
                    self.enabled = false;
                } else if self.sweep_shift != 0 {
                    self.shadow_freq = new_freq;
                    self.freq_low = (new_freq & 0xFF) as u8;
                    self.freq_high = ((new_freq >> 8) & 0x07) as u8;
                }
            }
        }
    }
}

struct WaveChannel {
    dac_enabled: bool,
    length_load: u16,
    length_timer: u16,
    length_enable: bool,
    volume_code: u8,
    freq_low: u8,
    freq_high: u8,
    enabled: bool,
    freq_timer: u16,
    position: u8,
    sample_buffer: u8,
    wave_ram: [u8; 16],
    last_amp: i32,
}

impl WaveChannel {
    pub fn new() -> Self {
        Self {
            dac_enabled: false,
            length_load: 0,
            length_timer: 0,
            length_enable: false,
            volume_code: 0,
            freq_low: 0,
            freq_high: 0,
            enabled: false,
            freq_timer: 0,
            position: 0,
            sample_buffer: 0,
            wave_ram: [0; 16],
            last_amp: 0,
        }
    }

    fn tick(&mut self) -> Option<i32> {
        if self.freq_timer > 0 {
            self.freq_timer -= 1;
        }

        if self.freq_timer == 0 {
            let freq = (self.freq_high as u16) << 8 | self.freq_low as u16;
            self.freq_timer = (2048 - freq) * 2;
            self.position = (self.position + 1) & 31;

            let nibble = self.wave_ram[self.position as usize / 2];
            if self.position % 2 == 0 {
                self.sample_buffer = nibble >> 4;
            } else {
                self.sample_buffer = nibble & 0x0F;
            }
        }

        let amplitude: i32 = if self.enabled && self.dac_enabled {
            let volume_shift = match self.volume_code {
                0 => 4, // mute (shift right 4 = effectively 0)
                1 => 0, // 100%
                2 => 1, // 50%
                3 => 2, // 25%
                _ => 4,
            };
            (self.sample_buffer >> volume_shift) as i32 * 256
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

    fn clock_length(&mut self) {
        if self.length_enable && self.length_timer > 0 {
            self.length_timer -= 1;

            if self.length_timer == 0 {
                self.enabled = false;
            }
        }
    }
}
