use blip_buf::BlipBuf;

const CLOCKS_PER_SECOND: u32 = 4_194_304;
const CLOCKS_PER_FRAME: u32 = CLOCKS_PER_SECOND / 512;
const OUTPUT_SAMPLE_COUNT: usize = 2000;

const DUTY_TABLE: [[i32; 8]; 4] = [
    [-1, -1, -1, -1, 1, -1, -1, -1], // 12.5%
    [-1, -1, -1, -1, 1, 1, -1, -1],  // 25%
    [-1, -1, 1, 1, 1, 1, -1, -1],    // 50%
    [1, 1, 1, 1, -1, -1, 1, 1],      // 75%
];

pub struct Apu {
    enabled: bool,
    nr50: u8,
    nr51: u8,
    volume_left: u8,
    volume_right: u8,
    ch1: SquareChannel,
    ch2: SquareChannel,
    ch3: WaveChannel,
    ch4: NoiseChannel,

    time: u32,
    prev_time: u32,
    next_frame_time: u32,
    frame_step: u8,
    output_period: u32,
}

fn create_blipbuf(sample_rate: u32) -> BlipBuf {
    let mut blipbuf = BlipBuf::new(OUTPUT_SAMPLE_COUNT as u32 + 1);
    blipbuf.set_rates(CLOCKS_PER_SECOND as f64, sample_rate as f64);
    blipbuf
}

impl Apu {
    pub fn new(sample_rate: u32) -> Self {
        let output_period =
            (OUTPUT_SAMPLE_COUNT as u64 * CLOCKS_PER_SECOND as u64) / sample_rate as u64;

        Self {
            enabled: false,
            nr50: 0x00,
            nr51: 0x00,
            volume_left: 0,
            volume_right: 0,
            ch1: SquareChannel::new(create_blipbuf(sample_rate), true),
            ch2: SquareChannel::new(create_blipbuf(sample_rate), false),
            ch3: WaveChannel::new(create_blipbuf(sample_rate)),
            ch4: NoiseChannel::new(create_blipbuf(sample_rate)),
            time: 0,
            prev_time: 0,
            next_frame_time: CLOCKS_PER_FRAME,
            frame_step: 0,
            output_period: output_period as u32,
        }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0xFF10 => {
                0x80 | (self.ch1.sweep_period << 4)
                    | ((self.ch1.sweep_negate as u8) << 3)
                    | self.ch1.sweep_shift
            }
            0xFF11 => (self.ch1.duty << 6) | 0x3F,
            0xFF12 => self.ch1.envelope.read(),
            0xFF13 => 0xFF,
            0xFF14 => 0xBF | ((self.ch1.length.enabled as u8) << 6),
            0xFF16 => (self.ch2.duty << 6) | 0x3F,
            0xFF17 => self.ch2.envelope.read(),
            0xFF18 => 0xFF,
            0xFF19 => 0xBF | ((self.ch2.length.enabled as u8) << 6),
            0xFF20 => 0xFF,
            0xFF21 => self.ch4.envelope.read(),
            0xFF22 => self.ch4.reg_ff22,
            0xFF23 => 0xBF | ((self.ch4.length.enabled as u8) << 6),
            0xFF24 => self.nr50,
            0xFF25 => self.nr51,
            0xFF26 => {
                0x70 | ((self.enabled as u8) << 7)
                    | ((self.ch4.active as u8) << 3)
                    | ((self.ch3.active as u8) << 2)
                    | ((self.ch2.active as u8) << 1)
                    | (self.ch1.active as u8)
            }
            0xFF1A => 0x7F | ((self.ch3.dac_enabled as u8) << 7),
            0xFF1B => 0xFF,
            0xFF1C => 0x9F | (self.ch3.volume_shift_code << 5),
            0xFF1D => 0xFF,
            0xFF1E => 0xBF | ((self.ch3.length.enabled as u8) << 6),
            0xFF30..=0xFF3F => {
                if !self.ch3.active {
                    self.ch3.wave_ram[(addr - 0xFF30) as usize]
                } else if self.ch3.sample_recently_accessed {
                    self.ch3.wave_ram[self.ch3.current_wave as usize >> 1]
                } else {
                    0xFF
                }
            }
            _ => 0xFF,
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        let is_length_reg = matches!(addr, 0xFF11 | 0xFF16 | 0xFF1B | 0xFF20);
        if !self.enabled && addr != 0xFF26 && !(0xFF30..=0xFF3F).contains(&addr) && !is_length_reg {
            return;
        }

        // Run channels up to current time before modifying state
        if self.enabled {
            self.run();
        }

        match addr {
            0xFF10 => {
                let old_negate = self.ch1.sweep_negate;
                self.ch1.sweep_period = (value >> 4) & 0x07;
                self.ch1.sweep_negate = value & 0x08 != 0;
                self.ch1.sweep_shift = value & 0x07;
                if old_negate && !self.ch1.sweep_negate && self.ch1.sweep_did_negate {
                    self.ch1.active = false;
                }
                self.ch1.sweep_did_negate = false;
            }
            0xFF11 => {
                self.ch1.duty = (value >> 6) & 0x03;
                self.ch1.length.set(value & 0x3F);
            }
            0xFF12 => {
                self.ch1.dac_enabled = value & 0xF8 != 0;
                self.ch1.active = self.ch1.active && self.ch1.dac_enabled;
                self.ch1.envelope.write_nrx2(value);
            }
            0xFF13 => {
                self.ch1.frequency = (self.ch1.frequency & 0x0700) | value as u16;
                self.ch1.calculate_period();
            }
            0xFF14 => {
                self.ch1.frequency = (self.ch1.frequency & 0x00FF) | (((value & 0x07) as u16) << 8);
                self.ch1.calculate_period();

                self.ch1.length.enable(value & 0x40 != 0, self.frame_step);
                self.ch1.active = self.ch1.active && self.ch1.length.is_active();

                if value & 0x80 != 0 {
                    if self.ch1.dac_enabled {
                        self.ch1.active = true;
                    }
                    self.ch1.length.trigger(self.frame_step);
                    self.ch1.envelope.trigger();

                    self.ch1.sweep_frequency = self.ch1.frequency;
                    self.ch1.sweep_delay = if self.ch1.sweep_period != 0 {
                        self.ch1.sweep_period
                    } else {
                        8
                    };
                    self.ch1.sweep_enabled = self.ch1.sweep_period > 0 || self.ch1.sweep_shift > 0;
                    self.ch1.sweep_did_negate = false;

                    if self.ch1.sweep_shift > 0 {
                        self.ch1.sweep_calculate_frequency();
                    }
                }
            }
            0xFF16 => {
                self.ch2.duty = (value >> 6) & 0x03;
                self.ch2.length.set(value & 0x3F);
            }
            0xFF17 => {
                self.ch2.dac_enabled = value & 0xF8 != 0;
                self.ch2.active = self.ch2.active && self.ch2.dac_enabled;
                self.ch2.envelope.write_nrx2(value);
            }
            0xFF18 => {
                self.ch2.frequency = (self.ch2.frequency & 0x0700) | value as u16;
                self.ch2.calculate_period();
            }
            0xFF19 => {
                self.ch2.frequency = (self.ch2.frequency & 0x00FF) | (((value & 0x07) as u16) << 8);
                self.ch2.calculate_period();

                self.ch2.length.enable(value & 0x40 != 0, self.frame_step);
                self.ch2.active = self.ch2.active && self.ch2.length.is_active();

                if value & 0x80 != 0 {
                    if self.ch2.dac_enabled {
                        self.ch2.active = true;
                    }
                    self.ch2.length.trigger(self.frame_step);
                    self.ch2.envelope.trigger();
                }
            }
            0xFF1A => {
                self.ch3.dac_enabled = (value & 0x80) != 0;
                self.ch3.active = self.ch3.active && self.ch3.dac_enabled;
            }
            0xFF1B => {
                self.ch3.length.set(value);
            }
            0xFF1C => {
                self.ch3.volume_shift_code = (value >> 5) & 0x03;
            }
            0xFF1D => {
                self.ch3.frequency = (self.ch3.frequency & 0x0700) | value as u16;
                self.ch3.calculate_period();
            }
            0xFF1E => {
                self.ch3.frequency = (self.ch3.frequency & 0x00FF) | (((value & 0x07) as u16) << 8);
                self.ch3.calculate_period();

                self.ch3.length.enable(value & 0x40 != 0, self.frame_step);
                self.ch3.active = self.ch3.active && self.ch3.length.is_active();

                if value & 0x80 != 0 {
                    self.ch3.length.trigger(self.frame_step);
                    self.ch3.current_wave = 0;
                    // Additional delay on trigger
                    self.ch3.delay = self.ch3.period + 4;

                    if self.ch3.dac_enabled {
                        self.ch3.active = true;
                    }
                }
            }
            0xFF20 => {
                self.ch4.length.set(value & 0x3F);
            }
            0xFF21 => {
                self.ch4.dac_enabled = value & 0xF8 != 0;
                self.ch4.active = self.ch4.active && self.ch4.dac_enabled;
                self.ch4.envelope.write_nrx2(value);
            }
            0xFF22 => {
                self.ch4.reg_ff22 = value;
                self.ch4.shift_width = if value & 8 != 0 { 6 } else { 14 };
                let freq_div = match value & 7 {
                    0 => 8u32,
                    n => n as u32 * 16,
                };
                self.ch4.period = freq_div << (value >> 4);
            }
            0xFF23 => {
                self.ch4.length.enable(value & 0x40 != 0, self.frame_step);
                self.ch4.active = self.ch4.active && self.ch4.length.is_active();

                if value & 0x80 != 0 {
                    self.ch4.length.trigger(self.frame_step);
                    self.ch4.state = 0xFF;
                    self.ch4.delay = 0;

                    if self.ch4.dac_enabled {
                        self.ch4.active = true;
                    }
                    self.ch4.envelope.trigger();
                }
            }
            0xFF24 => {
                self.nr50 = value;
                self.volume_left = value & 0x07;
                self.volume_right = (value >> 4) & 0x07;
            }
            0xFF25 => {
                self.nr51 = value;
            }
            0xFF26 => {
                let turn_on = value & 0x80 != 0;
                if self.enabled && !turn_on {
                    // Reset all registers to 0 when turning off
                    for i in 0xFF10..=0xFF25u16 {
                        self.write_byte(i, 0);
                    }
                    self.nr50 = 0;
                    self.nr51 = 0;
                }
                if !self.enabled && turn_on {
                    self.frame_step = 0;
                }
                self.enabled = turn_on;
            }
            0xFF30..=0xFF3F => {
                if !self.ch3.active {
                    self.ch3.wave_ram[(addr - 0xFF30) as usize] = value;
                } else if self.ch3.sample_recently_accessed {
                    self.ch3.wave_ram[self.ch3.current_wave as usize >> 1] = value;
                }
            }
            _ => {}
        }
    }

    /// Called by the CPU after each instruction with the number of M-cycles elapsed.
    pub fn step(&mut self, cycles: u8) {
        if !self.enabled {
            return;
        }

        let t_cycles = cycles as u32 * 4;
        self.time += t_cycles;

        if self.time >= self.output_period {
            self.do_output();
        }
    }

    /// Run channels and frame sequencer up to self.time.
    fn run(&mut self) {
        while self.next_frame_time <= self.time {
            self.ch1.run(self.prev_time, self.next_frame_time);
            self.ch2.run(self.prev_time, self.next_frame_time);
            self.ch3.run(self.prev_time, self.next_frame_time);
            self.ch4.run(self.prev_time, self.next_frame_time);

            if self.frame_step % 2 == 0 {
                self.ch1.step_length();
                self.ch2.step_length();
                self.ch3.step_length();
                self.ch4.step_length();
            }
            if self.frame_step % 4 == 2 {
                self.ch1.step_sweep();
            }
            if self.frame_step == 7 {
                self.ch1.envelope.step();
                self.ch2.envelope.step();
                self.ch4.envelope.step();
            }

            self.frame_step = (self.frame_step + 1) & 7;
            self.prev_time = self.next_frame_time;
            self.next_frame_time += CLOCKS_PER_FRAME;
        }

        if self.prev_time != self.time {
            self.ch1.run(self.prev_time, self.time);
            self.ch2.run(self.prev_time, self.time);
            self.ch3.run(self.prev_time, self.time);
            self.ch4.run(self.prev_time, self.time);
            self.prev_time = self.time;
        }
    }

    fn do_output(&mut self) {
        self.run();

        self.ch1.blip.end_frame(self.time);
        self.ch2.blip.end_frame(self.time);
        self.ch3.blip.end_frame(self.time);
        self.ch4.blip.end_frame(self.time);

        self.next_frame_time -= self.time;
        self.time = 0;
        self.prev_time = 0;
    }

    /// Called once per video frame. Reads downsampled audio and mixes channels with panning.
    pub fn end_frame(&mut self) -> Vec<f32> {
        // Make sure all pending audio is flushed
        if self.enabled {
            self.do_output();
        }

        let sample_count = self.ch1.blip.samples_avail() as usize;

        let left_vol = (self.volume_left as f32 / 7.0) * (1.0 / 15.0) * 0.25;
        let right_vol = (self.volume_right as f32 / 7.0) * (1.0 / 15.0) * 0.25;

        let mut buf_left = vec![0f32; sample_count];
        let mut buf_right = vec![0f32; sample_count];
        let mut buf = vec![0i16; sample_count];

        // Channel 1
        let count = self.ch1.blip.read_samples(&mut buf, false);
        for (i, &v) in buf[..count].iter().enumerate() {
            if self.nr51 & 0x10 != 0 {
                buf_left[i] += v as f32 * left_vol;
            }
            if self.nr51 & 0x01 != 0 {
                buf_right[i] += v as f32 * right_vol;
            }
        }

        // Channel 2
        let count = self.ch2.blip.read_samples(&mut buf, false);
        for (i, &v) in buf[..count].iter().enumerate() {
            if self.nr51 & 0x20 != 0 {
                buf_left[i] += v as f32 * left_vol;
            }
            if self.nr51 & 0x02 != 0 {
                buf_right[i] += v as f32 * right_vol;
            }
        }

        // Channel 3 (wave) — outputs at 4x amplitude for precision, divide back
        let count = self.ch3.blip.read_samples(&mut buf, false);
        for (i, &v) in buf[..count].iter().enumerate() {
            if self.nr51 & 0x40 != 0 {
                buf_left[i] += (v as f32 / 4.0) * left_vol;
            }
            if self.nr51 & 0x04 != 0 {
                buf_right[i] += (v as f32 / 4.0) * right_vol;
            }
        }

        // Channel 4 (noise)
        let count = self.ch4.blip.read_samples(&mut buf, false);
        for (i, &v) in buf[..count].iter().enumerate() {
            if self.nr51 & 0x80 != 0 {
                buf_left[i] += v as f32 * left_vol;
            }
            if self.nr51 & 0x08 != 0 {
                buf_right[i] += v as f32 * right_vol;
            }
        }

        // Interleave L/R
        let mut interleaved = Vec::with_capacity(sample_count * 2);
        for i in 0..sample_count {
            interleaved.push(buf_left[i]);
            interleaved.push(buf_right[i]);
        }
        interleaved
    }
}

// ─── Volume Envelope ──────────────────────────────────────────────────────────

struct VolumeEnvelope {
    period: u8,
    goes_up: bool,
    delay: u8,
    initial_volume: u8,
    volume: u8,
}

impl VolumeEnvelope {
    fn new() -> Self {
        Self {
            period: 0,
            goes_up: false,
            delay: 0,
            initial_volume: 0,
            volume: 0,
        }
    }

    fn read(&self) -> u8 {
        ((self.initial_volume & 0xF) << 4)
            | if self.goes_up { 0x08 } else { 0 }
            | (self.period & 0x07)
    }

    fn write_nrx2(&mut self, v: u8) {
        self.period = v & 0x07;
        self.goes_up = v & 0x08 != 0;
        self.initial_volume = v >> 4;
        self.volume = self.initial_volume;
    }

    fn trigger(&mut self) {
        self.delay = self.period;
        self.volume = self.initial_volume;
    }

    fn step(&mut self) {
        if self.delay > 1 {
            self.delay -= 1;
        } else if self.delay == 1 {
            self.delay = self.period;
            if self.goes_up && self.volume < 15 {
                self.volume += 1;
            } else if !self.goes_up && self.volume > 0 {
                self.volume -= 1;
            }
        }
    }
}

// ─── Length Counter ───────────────────────────────────────────────────────────

struct LengthCounter {
    enabled: bool,
    value: u16,
    max: u16,
}

impl LengthCounter {
    fn new(max: u16) -> Self {
        Self {
            enabled: false,
            value: 0,
            max,
        }
    }

    fn is_active(&self) -> bool {
        self.value > 0
    }

    fn extra_step(frame_step: u8) -> bool {
        // True when the *last* step was a length step (odd frame_step means
        // the next step is not a length step, so we just did one).
        frame_step % 2 == 1
    }

    fn enable(&mut self, enable: bool, frame_step: u8) {
        let was_enabled = self.enabled;
        self.enabled = enable;
        if !was_enabled && self.enabled && Self::extra_step(frame_step) {
            self.step();
        }
    }

    fn set(&mut self, minus_value: u8) {
        self.value = self.max - minus_value as u16;
    }

    fn trigger(&mut self, frame_step: u8) {
        if self.value == 0 {
            self.value = self.max;
            if Self::extra_step(frame_step) {
                self.step();
            }
        }
    }

    fn step(&mut self) {
        if self.enabled && self.value > 0 {
            self.value -= 1;
        }
    }
}

// ─── Square Channel ───────────────────────────────────────────────────────────

struct SquareChannel {
    active: bool,
    dac_enabled: bool,
    duty: u8,
    phase: u8,
    length: LengthCounter,
    frequency: u16,
    period: u32,
    last_amp: i32,
    delay: u32,
    has_sweep: bool,
    sweep_enabled: bool,
    sweep_frequency: u16,
    sweep_delay: u8,
    sweep_period: u8,
    sweep_shift: u8,
    sweep_negate: bool,
    sweep_did_negate: bool,
    envelope: VolumeEnvelope,
    blip: BlipBuf,
}

impl SquareChannel {
    fn new(blip: BlipBuf, has_sweep: bool) -> Self {
        Self {
            active: false,
            dac_enabled: false,
            duty: 0,
            phase: 0,
            length: LengthCounter::new(64),
            frequency: 0,
            period: 2048,
            last_amp: 0,
            delay: 0,
            has_sweep,
            sweep_enabled: false,
            sweep_frequency: 0,
            sweep_delay: 0,
            sweep_period: 0,
            sweep_shift: 0,
            sweep_negate: false,
            sweep_did_negate: false,
            envelope: VolumeEnvelope::new(),
            blip,
        }
    }

    fn calculate_period(&mut self) {
        if self.frequency > 2047 {
            self.period = 0;
        } else {
            self.period = (2048 - self.frequency as u32) * 4;
        }
    }

    fn run(&mut self, start_time: u32, end_time: u32) {
        if !self.active || self.period == 0 {
            if self.last_amp != 0 {
                self.blip.add_delta(start_time, -self.last_amp);
                self.last_amp = 0;
                self.delay = 0;
            }
            return;
        }

        let mut time = start_time + self.delay;
        let pattern = DUTY_TABLE[self.duty as usize];
        let vol = self.envelope.volume as i32;

        while time < end_time {
            let amp = vol * pattern[self.phase as usize];
            if amp != self.last_amp {
                self.blip.add_delta(time, amp - self.last_amp);
                self.last_amp = amp;
            }
            time += self.period;
            self.phase = (self.phase + 1) & 7;
        }

        self.delay = time - end_time;
    }

    fn step_length(&mut self) {
        self.length.step();
        self.active = self.active && self.length.is_active();
    }

    fn sweep_calculate_frequency(&mut self) -> u16 {
        let offset = self.sweep_frequency >> self.sweep_shift;
        let new_freq = if self.sweep_negate {
            self.sweep_did_negate = true;
            self.sweep_frequency.wrapping_sub(offset)
        } else {
            self.sweep_frequency.wrapping_add(offset)
        };
        if new_freq > 2047 {
            self.active = false;
        }
        new_freq
    }

    fn step_sweep(&mut self) {
        if !self.has_sweep {
            return;
        }

        if self.sweep_delay > 1 {
            self.sweep_delay -= 1;
        } else {
            if self.sweep_period == 0 {
                self.sweep_delay = 8;
            } else {
                self.sweep_delay = self.sweep_period;
                if self.sweep_enabled {
                    let new_freq = self.sweep_calculate_frequency();
                    if new_freq <= 2047 && self.sweep_shift != 0 {
                        self.sweep_frequency = new_freq;
                        self.frequency = new_freq;
                        self.calculate_period();
                    }
                    // Second overflow check
                    self.sweep_calculate_frequency();
                }
            }
        }
    }
}

// ─── Wave Channel ─────────────────────────────────────────────────────────────

struct WaveChannel {
    active: bool,
    dac_enabled: bool,
    length: LengthCounter,
    frequency: u16,
    period: u32,
    last_amp: i32,
    delay: u32,
    volume_shift_code: u8,
    wave_ram: [u8; 16],
    current_wave: u8,
    sample_recently_accessed: bool,
    blip: BlipBuf,
}

impl WaveChannel {
    fn new(blip: BlipBuf) -> Self {
        Self {
            active: false,
            dac_enabled: false,
            length: LengthCounter::new(256),
            frequency: 0,
            period: 2048,
            last_amp: 0,
            delay: 0,
            volume_shift_code: 0,
            wave_ram: [0; 16],
            current_wave: 0,
            sample_recently_accessed: false,
            blip,
        }
    }

    fn calculate_period(&mut self) {
        if self.frequency > 2048 {
            self.period = 0;
        } else {
            self.period = (2048 - self.frequency as u32) * 2;
        }
    }

    fn run(&mut self, start_time: u32, end_time: u32) {
        self.sample_recently_accessed = false;
        if !self.active || self.period == 0 {
            if self.last_amp != 0 {
                self.blip.add_delta(start_time, -self.last_amp);
                self.last_amp = 0;
                self.delay = 0;
            }
            return;
        }

        let mut time = start_time + self.delay;

        // Wave channel outputs at 4x amplitude to avoid precision loss on 25% volume shift.
        // This is accounted for during mixing in end_frame().
        let vol_shift = match self.volume_shift_code {
            0 => 4 + 2, // mute: shift a 4-bit sample * 4 to zero
            1 => 0,     // 100%
            2 => 1,     // 50%
            3 => 2,     // 25%
            _ => 4 + 2,
        };

        while time < end_time {
            let wave_byte = self.wave_ram[self.current_wave as usize >> 1];
            let sample = if self.current_wave % 2 == 0 {
                wave_byte >> 4
            } else {
                wave_byte & 0x0F
            };

            // Shift left by 2 so 25% doesn't lose precision
            let amp = ((sample << 2) >> vol_shift) as i32;

            if amp != self.last_amp {
                self.blip.add_delta(time, amp - self.last_amp);
                self.last_amp = amp;
            }

            if time >= end_time.saturating_sub(2) {
                self.sample_recently_accessed = true;
            }
            time += self.period;
            self.current_wave = (self.current_wave + 1) & 31;
        }

        self.delay = time - end_time;
    }

    fn step_length(&mut self) {
        self.length.step();
        self.active = self.active && self.length.is_active();
    }
}

// ─── Noise Channel ────────────────────────────────────────────────────────────

struct NoiseChannel {
    active: bool,
    dac_enabled: bool,
    reg_ff22: u8,
    length: LengthCounter,
    envelope: VolumeEnvelope,
    period: u32,
    shift_width: u8,
    state: u16,
    delay: u32,
    last_amp: i32,
    blip: BlipBuf,
}

impl NoiseChannel {
    fn new(blip: BlipBuf) -> Self {
        Self {
            active: false,
            dac_enabled: false,
            reg_ff22: 0,
            length: LengthCounter::new(64),
            envelope: VolumeEnvelope::new(),
            period: 2048,
            shift_width: 14,
            state: 1,
            delay: 0,
            last_amp: 0,
            blip,
        }
    }

    fn run(&mut self, start_time: u32, end_time: u32) {
        if !self.active {
            if self.last_amp != 0 {
                self.blip.add_delta(start_time, -self.last_amp);
                self.last_amp = 0;
                self.delay = 0;
            }
            return;
        }

        let mut time = start_time + self.delay;
        while time < end_time {
            let old_state = self.state;
            self.state <<= 1;
            let bit = ((old_state >> self.shift_width) ^ (self.state >> self.shift_width)) & 1;
            self.state |= bit;

            let amp = match (old_state >> self.shift_width) & 1 {
                0 => -(self.envelope.volume as i32),
                _ => self.envelope.volume as i32,
            };

            if self.last_amp != amp {
                self.blip.add_delta(time, amp - self.last_amp);
                self.last_amp = amp;
            }

            time += self.period;
        }
        self.delay = time - end_time;
    }

    fn step_length(&mut self) {
        self.length.step();
        self.active = self.active && self.length.is_active();
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

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
        assert_eq!(apu.read_byte(0xFF24), 0x00);
    }

    #[test]
    fn ch2_trigger_enables_and_produces_output() {
        let mut apu = Apu::new(44100);

        // Power on APU
        apu.write_byte(0xFF26, 0x80);
        // Enable CH2 on both outputs, master volume max
        apu.write_byte(0xFF25, 0x22);
        apu.write_byte(0xFF24, 0x77);
        // NR21: duty 50%, length 0
        apu.write_byte(0xFF16, 0x80);
        // NR22: volume 15, no envelope
        apu.write_byte(0xFF17, 0xF0);
        // NR23: freq low = 0x00
        apu.write_byte(0xFF18, 0x00);
        // NR24: trigger + freq high = 7
        apu.write_byte(0xFF19, 0x87);

        assert!(apu.ch2.active);
    }

    #[test]
    fn end_frame_produces_nonzero_samples() {
        let mut apu = Apu::new(44100);

        // Power on APU
        apu.write_byte(0xFF26, 0x80);
        // Enable CH2 on both outputs, master volume max
        apu.write_byte(0xFF25, 0x22);
        apu.write_byte(0xFF24, 0x77);
        // NR21: duty 50%, length 0
        apu.write_byte(0xFF16, 0x80);
        // NR22: volume 15, no envelope
        apu.write_byte(0xFF17, 0xF0);
        // NR23: freq low
        apu.write_byte(0xFF18, 0x00);
        // NR24: trigger + freq high = 6
        apu.write_byte(0xFF19, 0x86);

        // Run enough cycles for one frame
        for _ in 0..17556 {
            apu.step(1);
        }

        let samples = apu.end_frame();
        assert!(!samples.is_empty(), "end_frame should produce samples");
        assert!(
            samples.iter().any(|&s| s != 0.0),
            "end_frame should produce non-zero samples when a channel is active"
        );
    }

    #[test]
    fn blargg_01_register_roundtrip() {
        let masks: [(u16, u8); 22] = [
            (0xFF10, 0x80),
            (0xFF11, 0x3F),
            (0xFF12, 0x00),
            (0xFF13, 0xFF),
            (0xFF14, 0xBF),
            (0xFF15, 0xFF),
            (0xFF16, 0x3F),
            (0xFF17, 0x00),
            (0xFF18, 0xFF),
            (0xFF19, 0xBF),
            (0xFF1A, 0x7F),
            (0xFF1B, 0xFF),
            (0xFF1C, 0x9F),
            (0xFF1D, 0xFF),
            (0xFF1E, 0xBF),
            (0xFF1F, 0xFF),
            (0xFF20, 0xFF),
            (0xFF21, 0x00),
            (0xFF22, 0x00),
            (0xFF23, 0xBF),
            (0xFF24, 0x00),
            (0xFF25, 0x00),
        ];

        let mut apu = Apu::new(44100);
        apu.write_byte(0xFF26, 0x80); // power on

        for d in 0..=255u8 {
            for &(addr, mask) in &masks {
                apu.write_byte(addr, d);
                let expected = mask | d;
                let actual = apu.read_byte(addr);
                assert_eq!(
                    actual, expected,
                    "Register {:#06X} with d={:#04X}: expected {:#04X}, got {:#04X}",
                    addr, d, expected, actual
                );
                // Mute and disable wave between each (like the test does)
                apu.write_byte(0xFF25, 0x00);
                apu.write_byte(0xFF1A, 0x00);
            }
        }
    }

    #[test]
    fn blargg_01_wave_ram_roundtrip() {
        let mut apu = Apu::new(44100);
        apu.write_byte(0xFF26, 0x80);

        for d in 0..=255u8 {
            for addr in 0xFF30..=0xFF3Fu16 {
                apu.write_byte(addr, d);
                let actual = apu.read_byte(addr);
                assert_eq!(
                    actual, d,
                    "Wave RAM {:#06X} with d={:#04X}: expected {:#04X}, got {:#04X}",
                    addr, d, d, actual
                );
            }
        }
    }
}
