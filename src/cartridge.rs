use std::fmt;
use std::fs;
use std::path::Path;

const ROM_SIZE: u32 = 0x7FFF;

pub struct Cartridge {
    title: String,
    ctype: &'static str,
    rom_size: &'static str,
    ram_size: &'static str,
    rom_version: String,
    data: Vec<u8>,
    checksum: u8,
}

impl Cartridge {
    pub fn new() -> Self {
        Self {
            title: "".to_string(),
            ctype: "UNKNOWN",
            rom_size: "UNKNOWN",
            ram_size: "UNKNOWN",
            rom_version: "".to_string(),
            data: vec![0; ROM_SIZE as usize],
            checksum: 0,
        }
    }

    // TODO: add the different MBC's here.
    pub fn load(&mut self, path: &Path) -> Result<(), &str> {
        self.data = fs::read(path).unwrap();
        println!("{:?} loaded.", path);
        self.get_title();
        self.get_cartridge_type();
        self.get_rom_size();
        self.get_ram_size();
        self.get_version();
        self.calculate_and_check_checksum();
        Ok(())
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        self.data[addr as usize]
    }

    pub fn write_byte(&mut self, addr: u16, value: u8) {
        self.data[addr as usize] = value;
    }

    // title of the game in upper case ascii
    fn get_title(&mut self) {
        for addr in 0x134..=0x143 {
            self.title.push(self.read_byte(addr) as char);
        }
    }

    // Specifices which Memory Bank Controller is used in the cartridge and what other external
    // hardware is available
    fn get_cartridge_type(&mut self) {
        self.ctype = match self.data[0x147] {
            0x00 => "ROM ONLY",
            0x01 => "MBC1",
            0x02 => "MBC1+RAM",
            0x03 => "MBC1+RAM+BATTERY",
            0x05 => "MBC2",
            0x06 => "MBC2+BATTERY",
            0x08 => "ROM+RAM",
            0x09 => "ROM+RAM+BATTERY",
            0x0B => "MMM01",
            0x0C => "MMM01+RAM",
            0x0D => "MMM01+RAM+BATTERY",
            0x0F => "MBC3+TIMER+BATTERY",
            0x10 => "MBC3+TIMER+RAM+BATTERY",
            0x11 => "MBC3",
            0x12 => "MBC3+RAM",
            0x13 => "MBC3+RAM+BATTERY",
            0x19 => "MBC5",
            0x1A => "MBC5+RAM",
            0x1B => "MBC5+RAM+BATTERY",
            0x1C => "MBC5+RUMBLE",
            0x1D => "MBC5+RUMBLE+RAM",
            0x1E => "MBC5+RUMBLE+RAM+BATTERY",
            0x20 => "MBC6",
            0x22 => "MBC7+SENSOR+RUMBLE+RAM+BATTERY",
            0xFC => "POCKET CAMERA",
            0xFD => "BANDAI TAMA5",
            0xFE => "HuC3",
            0xFF => "HuC1+RAM+BATTERY",
            _ => "",
        }
    }

    // Rom size of the cartridge
    fn get_rom_size(&mut self) {
        self.rom_size = match self.data[0x148] {
            0x00 => "32 KByte",
            0x01 => "64 KByte",
            0x02 => "128 KByte",
            0x03 => "256 KByte",
            0x04 => "512 KByte",
            0x05 => "1 MByte",
            0x06 => "2 MByte",
            0x07 => "4 MByte",
            0x08 => "8 MByte",
            0x52 => "1.1 MByte",
            0x53 => "1.2 MByte",
            0x54 => "1.5 MByte",
            _ => "0",
        }
    }

    // Size of external ram in cartridge if present
    fn get_ram_size(&mut self) {
        self.ram_size = match self.data[0x149] {
            0x00 => "0",
            0x01 => "-",
            0x02 => "8 KB",
            0x03 => "32 KB",
            0x04 => "128 KB",
            0x05 => "64 KB",
            _ => "0",
        }
    }

    // Specifies version number of the game
    fn get_version(&mut self) {
        self.rom_version = self.data[0x14C].to_string();
    }

    // Calculate checksum based on header bytes 0x0134 - 0x014C
    // if byte at 0x014D does not match lower 8 bits of x, boot rom lock up
    fn calculate_and_check_checksum(&mut self) {
        let mut x: u8 = 0;
        for i in 0x0134..=0x014C {
            x = x.wrapping_sub(self.data[i]).wrapping_sub(1);
        }
        self.checksum = x;
    }
}

impl Default for Cartridge {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Cartridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Title: {}\nType: {}\nROM Size: {}\nRam Size: {}\nVersion: {}\nChecksum: {:#X} {}",
            self.title,
            self.ctype,
            self.rom_size,
            self.ram_size,
            self.rom_version,
            self.checksum,
            if self.checksum == self.data[0x14D] {
                "PASSED"
            } else {
                "FAILED"
            }
        )
    }
}
