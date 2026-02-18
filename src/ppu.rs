pub const VRAM_SIZE: u16 = 0x1FFF;

const VBLANK: u8 = 1;
const HBLANK: u8 = 0;
const OAM_SCAN: u8 = 2;
const PIXEL_TRANSFER: u8 = 3;

const MAX_OAM_ENTRIES: usize = 40;

pub struct Ppu {
    // stores graphic tiles
    pub video_ram: Vec<u8>,
    pub frame_buffer: Vec<u32>,
    mode: u8,
    // OAM stores data that tells the gameboy
    // which tiles to use to construct moving objects on the screen
    pub oam: Vec<u8>,
    pub ly: u8,
    pub ly_cycles: u16,
    pub stat: u8,
    pub lyc: u8,
    pub lcdc: u8,
    scy: u8,
    scx: u8,
    bgp: u8,
    obp0: u8,
    obp1: u8,
    wy: u8,
    wx: u8,
    window_line_counter: u8,
    scanline_sprites: Vec<usize>,
}

impl Ppu {
    pub fn new() -> Self {
        Self {
            video_ram: vec![0x00; VRAM_SIZE as usize + 1],
            frame_buffer: vec![0x00FFFFFFu32; 160 * 144],
            mode: 0,
            oam: vec![0xFF; 160], // 160 bytes for OAM
            ly: 0,
            ly_cycles: 0,
            lyc: 0x00,
            stat: 0x85,
            lcdc: 0x91,
            scy: 0,
            scx: 0,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
            wy: 0,
            wx: 0,
            window_line_counter: 0,
            scanline_sprites: Vec::new(),
        }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => {
                if self.mode == PIXEL_TRANSFER && self.lcdc & 0x80 != 0 {
                    return 0xFF;
                }
                self.video_ram[(addr - 0x8000) as usize]
            }
            0xFE00..=0xFE9F => {
                if (self.mode == OAM_SCAN || self.mode == PIXEL_TRANSFER) && self.lcdc & 0x80 != 0 {
                    return 0xFF;
                }

                self.oam[(addr - 0xFE00) as usize]
            }

            0xFF40 => self.lcdc,
            0xFF41 => 0x80 | self.stat | self.mode,
            0xFF42 => self.scy,
            0xFF43 => self.scx,
            0xFF44 => self.ly,
            0xFF45 => self.lyc,
            0xFF47 => self.bgp,
            0xFF48 => self.obp0,
            0xFF49 => self.obp1,
            0xFF4A => self.wy,
            0xFF4B => self.wx,
            _ => panic!("PPU read error at address: {:#04X}", addr),
        }
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000..=0x9FFF => {
                if self.mode == PIXEL_TRANSFER && self.lcdc & 0x80 != 0 {
                    return;
                }
                self.video_ram[(addr - 0x8000) as usize] = value;
            }

            0xFE00..=0xFE9F => {
                if (self.mode == OAM_SCAN || self.mode == PIXEL_TRANSFER) && self.lcdc & 0x80 != 0 {
                    return;
                }
                self.oam[(addr - 0xFE00) as usize] = value;
            }
            0xFF40 => self.lcdc = value,
            0xFF41 => self.stat = value & 0x7C, // only bits 2-6 are writable
            0xFF42 => self.scy = value,
            0xFF43 => self.scx = value,
            0xFF45 => self.lyc = value,
            0xFF47 => self.bgp = value,
            0xFF48 => self.obp0 = value, // lower 2 bits are ignored
            0xFF49 => self.obp1 = value, // lower 2 bits are ignored
            0xFF4A => self.wy = value,
            0xFF4B => self.wx = value,
            _ => panic!("PPU write error at address: {:#04X}", addr),
        }
    }

    pub fn update_ly(&mut self, cycles: u8) -> u8 {
        let mut bitmask: u8 = 0;

        self.ly_cycles += cycles as u16;

        let mode_before = self.mode;

        if self.lcdc & 0x80 == 0 {
            self.ly = 0;
            self.ly_cycles = 0;
            self.mode = 0;
            self.stat &= !0x04; // clear coincidence flag
            return bitmask;
        }

        if self.ly >= 144 {
            self.mode = VBLANK;
            if mode_before != self.mode && self.stat & 0x10 != 0 {
                bitmask |= 0x02;
            }
        } else if self.ly_cycles < 80 {
            self.mode = OAM_SCAN; // OAM scan
            if mode_before != self.mode && self.stat & 0x20 != 0 {
                bitmask |= 0x02;
            }
        } else if self.ly_cycles < 252 {
            self.mode = PIXEL_TRANSFER; // Pixel transfer
        } else {
            self.mode = HBLANK; // HBlank
            if mode_before != self.mode && self.stat & 0x08 != 0 {
                bitmask |= 0x02;
            }
        };

        while self.ly_cycles >= 456 {
            self.ly_cycles -= 456;
            if self.ly < 144 {
                self.render_scanline();
            }
            self.ly = self.ly.wrapping_add(1);
            if self.ly > 153 {
                self.ly = 0;
                self.window_line_counter = 0;
            }

            if self.ly == 144 {
                bitmask |= 0x01;
            }

            // STAT coincidence flag and interrupt
            if self.ly == self.lyc {
                self.stat |= 0x04;
                if self.stat & 0x40 != 0 {
                    bitmask |= 0x02;
                }
            } else {
                self.stat &= !0x04;
            }
        }

        bitmask
    }

    fn get_tile_color_id(&self, map_x: u16, map_y: u16, use_high_tile_map: bool) -> u8 {
        let tile_x = map_x / 8;
        let tile_y = map_y / 8;

        let tile_index = if use_high_tile_map {
            0x9C00 + tile_y * 32 + tile_x
        } else {
            0x9800 + tile_y * 32 + tile_x
        };

        let tile_data = self.video_ram[tile_index as usize - 0x8000];

        let pixel_row = map_y % 8;
        let tile_addr = if self.lcdc & 0x10 != 0 {
            0x8000 + (tile_data as u16) * 16 + pixel_row * 2
        } else {
            (0x9000u16)
                .wrapping_add(((tile_data as i8 as i16) * 16) as u16)
                .wrapping_add(pixel_row * 2)
        };

        let byte_low = self.video_ram[tile_addr as usize - 0x8000];
        let byte_high = self.video_ram[(tile_addr + 1) as usize - 0x8000];

        let bit_index = 7 - (map_x % 8);
        ((byte_high >> bit_index) & 1) << 1 | ((byte_low >> bit_index) & 1)
    }

    fn apply_palette(palette: u8, color_id: u8) -> u32 {
        let shade = (palette >> (color_id * 2)) & 0x03;
        let pixel_color = match shade {
            0 => 255u32, // White
            1 => 170,    // Light gray
            2 => 85,     // Dark gray
            3 => 0,      // Black
            _ => unreachable!(),
        };
        0xFF000000 | pixel_color << 16 | pixel_color << 8 | pixel_color
    }

    pub fn render_scanline(&mut self) {
        let window_width = 160;

        let mut window_drawn = false;
        let mut bg_color_ids = [0u8; 160];

        self.oam_scan();


        for (x, bg_color_id) in bg_color_ids.iter_mut().enumerate().take(window_width) {
            let bg_map_y: u16 = (self.scy as u16 + self.ly as u16) % 256;
            let bg_map_x: u16 = (self.scx as u16 + x as u16) % 256;

            if self.lcdc & 0x01 == 0 {
                // Background display is disabled, fill with white
                self.frame_buffer[self.ly as usize * window_width + x] = 0xFFFFFFFF;
                continue;
            }

            let color_id = self.get_tile_color_id(bg_map_x, bg_map_y, self.lcdc & 0x08 != 0);
            self.frame_buffer[self.ly as usize * window_width + x] =
                Self::apply_palette(self.bgp, color_id);
            *bg_color_id = color_id;

            let should_draw_at_position = self.lcdc & 0x20 != 0
                && self.ly >= self.wy
                && x >= self.wx.saturating_sub(7) as usize;
            if should_draw_at_position {
                window_drawn = true;

                let win_x: u16 = (x - (self.wx.saturating_sub(7) as usize)) as u16;
                let win_y: u16 = self.window_line_counter as u16;

                let color_id = self.get_tile_color_id(win_x, win_y, self.lcdc & 0x40 != 0);
                *bg_color_id = color_id;
                self.frame_buffer[self.ly as usize * window_width + x] =
                    Self::apply_palette(self.bgp, color_id);
            }

            self.scanline_sprites.iter().for_each(|&sprite| {
                let sprite_x: i16 = self.oam[sprite * 4 + 1] as i16 - 8;
                let sprite_y: i16 = self.oam[sprite * 4] as i16 - 16;
                let sprite_tile: u8 = self.oam[sprite * 4 + 2];
                let sprite_attr: u8 = self.oam[sprite * 4 + 3];

                let sprite_height = if self.lcdc & 0x04 != 0 { 16 } else { 8 };

                let pixel_column_within_span_of_sprite =
                    x as i16 >= sprite_x && (x as i16) < sprite_x + 8;

                if pixel_column_within_span_of_sprite {
                    let tile_row = if sprite_attr & 0x40 != 0 {
                        sprite_height - 1 - (self.ly as i16 - sprite_y)
                    } else {
                        self.ly as i16 - sprite_y
                    } as u16;
                    let tile = if sprite_height == 16 {
                        if tile_row < 8 {
                            sprite_tile & 0xFE
                        } else {
                            sprite_tile | 0x01
                        }
                    } else {
                        sprite_tile
                    };
                    let row_in_tile = tile_row % 8;
                    let tile_addr = 0x8000 + (tile as u16) * 16 + row_in_tile * 2;

                    let byte_low = self.video_ram[tile_addr as usize - 0x8000];
                    let byte_high = self.video_ram[(tile_addr + 1) as usize - 0x8000];
                    let bit_index = if sprite_attr & 0x20 != 0 {
                        (x as i16 - sprite_x) as u8
                    } else {
                        7 - (x as i16 - sprite_x) as u8
                    };
                    let color_id =
                        ((byte_high >> bit_index) & 1) << 1 | ((byte_low >> bit_index) & 1);

                    if color_id != 0 {
                        if sprite_attr & 0x80 != 0 && *bg_color_id != 0 {
                            return;
                        }

                        let palette = if sprite_attr & 0x10 != 0 {
                            self.obp1
                        } else {
                            self.obp0
                        };
                        self.frame_buffer[self.ly as usize * window_width + x] =
                            Self::apply_palette(palette, color_id);
                    }
                }
            });
        }

        if window_drawn {
            self.window_line_counter += 1;
        }
    }

    fn oam_scan(&mut self) {
        self.scanline_sprites.clear();

        if self.lcdc & 0x02 == 0 {
            return; // Sprites are disabled
        }

        let sprite_height = if self.lcdc & 0x04 != 0 { 16 } else { 8 };

        for i in 0..MAX_OAM_ENTRIES {
            let sprite_y = self.oam[i * 4] as i16 - 16;

            if sprite_y <= self.ly as i16 && sprite_y + sprite_height > self.ly as i16 {
                self.scanline_sprites.push(i);
                if self.scanline_sprites.len() >= 10 {
                    break; // Maximum 10 sprites per scanline
                }
            }
        }
    }
}
