use macroquad::prelude::Conf;

pub fn window_conf() -> Conf {
    Conf {
        window_title: "Gameboy Emulator".to_owned(),
        window_width: 800,
        window_height: 600,
        window_resizable: false,
        ..Default::default()
    }
}
