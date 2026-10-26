use eframe::egui;

const SIZE: usize = 32;

const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b001, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

const DASH: [u8; 5] = [0, 0, 0, 0b111, 0];

pub struct Rgba {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn tray_level(level: Option<u8>, charging: bool, low: u8) -> tray_icon::Icon {
    let image = build(level, charging, low, SIZE as u32);
    tray_icon::Icon::from_rgba(image.pixels, image.width, image.height)
        .expect("generated tray icon is valid RGBA")
}

pub fn egui_level(level: Option<u8>, charging: bool, low: u8) -> egui::IconData {
    let image = build(level, charging, low, 64);
    egui::IconData {
        rgba: image.pixels,
        width: image.width,
        height: image.height,
    }
}

pub fn preview_level(level: Option<u8>, charging: bool, low: u8) -> egui::ColorImage {
    let image = build(level, charging, low, SIZE as u32);
    egui::ColorImage::from_rgba_unmultiplied(
        [image.width as usize, image.height as usize],
        &image.pixels,
    )
}

pub fn build(level: Option<u8>, charging: bool, low: u8, size: u32) -> Rgba {
    let scale = (size / SIZE as u32).max(1) as usize;
    let mut canvas = Canvas::new(SIZE, scale);

    let accent = match (level, charging) {
        (_, true) => Rgb::new(0x55, 0xDA, 0xFF),
        (Some(value), false) if value <= low => Rgb::new(0xFF, 0x6B, 0x6B),
        (Some(value), false) if value <= 60 => Rgb::new(0xF6, 0xC4, 0x53),
        (Some(_), false) => Rgb::new(0xBC, 0xF2, 0x55),
        (None, false) => Rgb::new(0x77, 0x87, 0x99),
    };

    canvas.rounded_rect(0, 0, 31, 31, 7, accent);
    canvas.rounded_rect(2, 2, 29, 29, 5, Rgb::new(0x0B, 0x10, 0x16));

    let text = match level {
        Some(value) => format!("{value}"),
        None => "--".to_owned(),
    };

    let (pixel, gap, text_y) = if text.len() >= 3 {
        (3, 0, 8)
    } else {
        (4, 2, 6)
    };
    let glyph_width = 3 * pixel;
    let width = text.len() * glyph_width + text.len().saturating_sub(1) * gap;
    let x = (SIZE as i32 - width as i32) / 2;
    let color = Rgb::new(0xF4, 0xF8, 0xFA);

    for (index, character) in text.chars().enumerate() {
        let offset = x + (index * (glyph_width + gap)) as i32;
        let pattern = character
            .to_digit(10)
            .map(|digit| DIGITS[digit as usize])
            .unwrap_or(DASH);
        for (row, bits) in pattern.iter().enumerate() {
            for column in 0..3 {
                if bits & (1 << (2 - column)) != 0 {
                    canvas.rect(
                        offset + (column * pixel) as i32,
                        text_y + (row * pixel) as i32,
                        offset + ((column + 1) * pixel) as i32 - 1,
                        text_y + ((row + 1) * pixel) as i32 - 1,
                        color,
                    );
                }
            }
        }
    }

    canvas.into_rgba(size as usize)
}

#[derive(Clone, Copy)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

impl Rgb {
    const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

struct Canvas {
    width: usize,
    height: usize,
    scale: usize,
    pixels: Vec<[u8; 4]>,
}

impl Canvas {
    fn new(width: usize, scale: usize) -> Self {
        Self {
            width,
            height: width,
            scale,
            pixels: vec![[0, 0, 0, 0]; width * width],
        }
    }

    fn set(&mut self, x: i32, y: i32, color: Rgb) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        self.pixels[y as usize * self.width + x as usize] = [color.r, color.g, color.b, 255];
    }

    fn rect(&mut self, left: i32, top: i32, right: i32, bottom: i32, color: Rgb) {
        for y in top..=bottom {
            for x in left..=right {
                self.set(x, y, color);
            }
        }
    }

    fn rounded_rect(
        &mut self,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        radius: i32,
        color: Rgb,
    ) {
        let radius_squared = radius * radius;
        for y in top..=bottom {
            for x in left..=right {
                let cx = if x < left + radius {
                    left + radius
                } else if x > right - radius {
                    right - radius
                } else {
                    x
                };
                let cy = if y < top + radius {
                    top + radius
                } else if y > bottom - radius {
                    bottom - radius
                } else {
                    y
                };
                let dx = x - cx;
                let dy = y - cy;
                if dx * dx + dy * dy <= radius_squared {
                    self.set(x, y, color);
                }
            }
        }
    }

    fn into_rgba(self, output_size: usize) -> Rgba {
        let mut pixels = vec![0u8; output_size * output_size * 4];
        for y in 0..output_size {
            for x in 0..output_size {
                let source_x = (x / self.scale).min(self.width - 1);
                let source_y = (y / self.scale).min(self.height - 1);
                let color = self.pixels[source_y * self.width + source_x];
                let index = (y * output_size + x) * 4;
                pixels[index..index + 4].copy_from_slice(&color);
            }
        }
        Rgba {
            pixels,
            width: output_size as u32,
            height: output_size as u32,
        }
    }
}
