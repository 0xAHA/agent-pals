/// BGRA, premultiplied, physical pixels — exactly what UpdateLayeredWindow wants.
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub scale: u32,
    pub px: Vec<u8>,
}

impl Canvas {
    pub fn new(lw: u32, lh: u32, scale: u32) -> Self {
        let mut c = Canvas { w: 0, h: 0, scale: 1, px: Vec::new() };
        c.resize(lw, lh, scale);
        c
    }

    pub fn resize(&mut self, lw: u32, lh: u32, scale: u32) {
        self.scale = scale.max(1);
        self.w = lw * self.scale;
        self.h = lh * self.scale;
        self.px = vec![0; (self.w * self.h * 4) as usize];
    }

    pub fn clear(&mut self) {
        self.px.fill(0);
    }

    /// Plot one logical pixel (a scale x scale block).
    pub fn dot(&mut self, lx: i32, ly: i32, rgb: [u8; 3], a: u8) {
        let s = self.scale as i32;
        let (x0, y0) = (lx * s, ly * s);
        let (w, h) = (self.w as i32, self.h as i32);
        if lx < 0 || ly < 0 || x0 >= w || y0 >= h {
            return;
        }
        let pa = a as u32;
        let px = [
            (rgb[2] as u32 * pa / 255) as u8,
            (rgb[1] as u32 * pa / 255) as u8,
            (rgb[0] as u32 * pa / 255) as u8,
            a,
        ];
        for y in y0..(y0 + s).min(h) {
            let row = (y * w) as usize;
            for x in x0..(x0 + s).min(w) {
                let i = (row + x as usize) * 4;
                self.px[i..i + 4].copy_from_slice(&px);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Bubble {
    Bang,
    Dots(u8),
    Zzz,
    Check,
}

/// Logical height reserved above the sprites for bubbles.
pub const BUBBLE_H: u32 = 11;

const BANG: [u8; 5] = [0b00100, 0b00100, 0b00100, 0b00000, 0b00100];
const ZED: [u8; 5] = [0b11111, 0b00010, 0b00100, 0b01000, 0b11111];
const CHECK: [u8; 5] = [0b00000, 0b00001, 0b00010, 0b10100, 0b01000];

pub fn draw_bubble(c: &mut Canvas, x: i32, y: i32, b: Bubble) {
    let ink = [40, 40, 56];
    let (bg, fg) = match b {
        Bubble::Bang => ([255, 214, 64], [200, 30, 30]),
        _ => ([250, 250, 250], ink),
    };
    let glyph = match b {
        Bubble::Bang => BANG,
        Bubble::Zzz => ZED,
        Bubble::Check => CHECK,
        Bubble::Dots(n) => {
            let row = match n {
                1 => 0b10000,
                2 => 0b10100,
                _ => 0b10101,
            };
            [0, 0, row, 0, 0]
        }
    };
    // 9x9 with an outline ring and clipped corners, plus a 1px tail.
    for by in 0..9 {
        for bx in 0..9 {
            let corner = (bx == 0 || bx == 8) && (by == 0 || by == 8);
            if corner {
                continue;
            }
            let edge = bx == 0 || bx == 8 || by == 0 || by == 8;
            c.dot(x + bx, y + by, if edge { ink } else { bg }, 255);
        }
    }
    c.dot(x + 2, y + 9, ink, 255);
    for (r, bits) in glyph.iter().enumerate() {
        for col in 0..5 {
            if bits & (1 << (4 - col)) != 0 {
                c.dot(x + 2 + col, y + 2 + r as i32, fg, 255);
            }
        }
    }
}
