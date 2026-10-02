//! Every picture of the shell is drawn here in code, pixel by pixel. Sprites written as rows
//! of characters use the same format the game master will later draw the world in.

use crate::theme::*;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::widget::NodeImageMode;

#[derive(Clone)]
pub struct Pixels {
    pub w: u32,
    pub h: u32,
    data: Vec<Rgba>,
}

impl Pixels {
    pub fn new(w: u32, h: u32, fill: Rgba) -> Self {
        Self { w, h, data: vec![fill; (w * h) as usize] }
    }

    /// One character per pixel; characters missing from the legend are transparent.
    pub fn from_rows(rows: &[&str], legend: &[(char, Rgba)]) -> Self {
        let w = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as u32;
        let mut p = Self::new(w, rows.len() as u32, CLEAR);
        for (y, row) in rows.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                if let Some((_, c)) = legend.iter().find(|(l, _)| *l == ch) {
                    p.set(x as i32, y as i32, *c);
                }
            }
        }
        p
    }

    pub fn get(&self, x: i32, y: i32) -> Rgba {
        self.get_or(x, y, CLEAR)
    }

    /// The pixel, or `outside` past the edge.
    pub fn get_or(&self, x: i32, y: i32, outside: Rgba) -> Rgba {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 { outside } else { self.data[(y as u32 * self.w + x as u32) as usize] }
    }

    /// Lays another picture over this one at a point; clear pixels of the other let this show.
    pub fn stamp(&mut self, other: &Pixels, at_x: i32, at_y: i32, flip: bool) {
        for y in 0..other.h as i32 {
            for x in 0..other.w as i32 {
                let source = if flip { other.w as i32 - 1 - x } else { x };
                let c = other.get(source, y);
                if c[3] != 0 {
                    self.set(at_x + x, at_y + y, c);
                }
            }
        }
    }

    /// The same picture mirrored left to right.
    pub fn flipped(&self) -> Pixels {
        let mut out = Pixels::new(self.w, self.h, CLEAR);
        out.stamp(self, 0, 0, true);
        out
    }

    pub fn set(&mut self, x: i32, y: i32, c: Rgba) {
        if x >= 0 && y >= 0 && x < self.w as i32 && y < self.h as i32 {
            self.data[(y as u32 * self.w + x as u32) as usize] = c;
        }
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgba) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }

    pub fn into_image(self) -> Image {
        Image::new(
            Extent3d { width: self.w, height: self.h, depth_or_array_layers: 1 },
            TextureDimension::D2,
            self.data.into_iter().flatten().collect(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
    }
}

/// Repeatable noise: the same coordinates always give the same value, so the table looks
/// the same on every launch.
fn hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA6B) ^ seed.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

const BAYER: [[u32; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

fn darken(c: Rgba, steps: u32) -> Rgba {
    let f = 0.8f32.powi(steps as i32);
    [(c[0] as f32 * f) as u8, (c[1] as f32 * f) as u8, (c[2] as f32 * f) as u8, c[3]]
}

/// The table top: planks with grain, butt joints, and corners falling into shadow.
pub fn table(w: u32, h: u32) -> Pixels {
    const PLANK: i32 = 27;
    let mut p = Pixels::new(w, h, WOOD);
    for y in 0..h as i32 {
        let plank = y / PLANK;
        let within = y % PLANK;
        let base = if hash(plank, 0, 1) % 3 == 0 { WOOD_DARK } else { WOOD };
        let streak_len = 14 + (hash(plank, within, 2) % 44) as i32;
        let streak_shift = (hash(plank, within, 3) % 97) as i32;
        let grain_row = hash(plank, within, 4) % 3 == 0;
        let joint = (hash(plank, 0, 5) % w) as i32;
        for x in 0..w as i32 {
            let mut c = base;
            if grain_row {
                c = match hash((x + streak_shift) / streak_len, y, 6) % 5 {
                    0 => WOOD_DARK,
                    1 if base == WOOD => WOOD_LIGHT,
                    _ => base,
                };
            }
            if within == 0 || x == joint {
                c = WOOD_DEEP;
            } else if within == 1 && hash(x / 9, y, 7) % 4 == 0 {
                c = WOOD_LIGHT;
            }
            // Distance from the centre, in a shape that follows the screen's proportions.
            let dx = (x as f32 / w as f32 - 0.5) * 2.0;
            let dy = (y as f32 / h as f32 - 0.5) * 2.0;
            let edge = ((dx * dx + dy * dy).sqrt() - 0.45).max(0.0) * 5.5;
            let whole = edge.floor();
            let dithered = (edge - whole) * 16.0 > BAYER[(y & 3) as usize][(x & 3) as usize] as f32;
            p.set(x, y, darken(c, whole as u32 + dithered as u32));
        }
    }
    p
}

/// A bevelled plate with clipped corners, drawn small so it can be stretched as a nine-slice.
/// `shadow` adds a dark row underneath, inside the image bounds.
fn plate(size: i32, outline: Rgba, light: Rgba, fill: Rgba, shade: Rgba, shadow: bool) -> Pixels {
    let mut p = Pixels::new(size as u32, size as u32, CLEAR);
    let bottom = if shadow { size - 2 } else { size - 1 };
    if shadow {
        p.rect(2, bottom, size - 3, 1, [0, 0, 0, 0x70]);
        p.rect(3, bottom + 1, size - 4, 1, [0, 0, 0, 0x70]);
        p.rect(size - 1, 3, 1, bottom - 3, [0, 0, 0, 0x70]);
    }
    let right = if shadow { size - 2 } else { size - 1 };
    p.rect(0, 0, right + 1, bottom + 1, outline);
    p.rect(1, 1, right - 1, bottom - 1, fill);
    p.rect(1, 1, right - 1, 1, light);
    p.rect(1, 1, 1, bottom - 1, light);
    p.rect(1, bottom - 1, right - 1, 1, shade);
    p.rect(right - 1, 1, 1, bottom - 1, shade);
    for (x, y) in [(0, 0), (right, 0), (0, bottom), (right, bottom)] {
        p.set(x, y, CLEAR);
    }
    for (x, y) in [(1, 1), (right - 1, 1), (1, bottom - 1), (right - 1, bottom - 1)] {
        p.set(x, y, outline);
    }
    p
}

const CANDLE_LEGEND: &[(char, Rgba)] = &[
    ('k', INK),
    ('t', TALLOW),
    ('s', TALLOW_SHADE),
    ('c', FLAME_CORE),
    ('f', FLAME),
    ('e', FLAME_EDGE),
    ('b', BRASS),
    ('l', BRASS_LIGHT),
    ('d', BRASS_DARK),
];

const FLAMES: [[&str; 10]; 4] = [
    ["....e.....", "....e.....", "...efe....", "...efe....", "..effce...", "..efcce...", "..efccfe..", "...efce...", "....ke....", "....k....."],
    [".....e....", "....ee....", "....fe....", "...effe...", "..efcfe...", "..efcce...", "..efccfe..", "...ecfe...", "....ke....", "....k....."],
    ["..........", "....e.....", "....fe....", "...efe....", "...efce...", "..efccfe..", "..efccfe..", "...efce...", "....ke....", "....k....."],
    ["...e......", "...ee.....", "...efe....", "..effe....", "..efcfe...", "..efcce...", "..efccfe..", "...efce...", "....ke....", "....k....."],
];

const CANDLE_BODY: [&str; 15] = [
    "..kkkkkk..",
    ".kttttttk.",
    ".kttttssk.",
    ".kttttttk.",
    "kktttttsk.",
    "ktkttttsk.",
    "ktktttssk.",
    ".kkttttsk.",
    ".kttttssk.",
    ".ktttttsk.",
    ".kttttssk.",
    "kkkkkkkkkk",
    "klbbbbbbdk",
    ".kbbbbddk.",
    "..kkkkkk..",
];

pub const CANDLE_SIZE: UVec2 = UVec2::new(10, 25);

fn candle(frame: usize) -> Pixels {
    let rows: Vec<&str> = FLAMES[frame].iter().chain(CANDLE_BODY.iter()).copied().collect();
    Pixels::from_rows(&rows, CANDLE_LEGEND)
}

/// A pool of candlelight: rings of thinning dither rather than a smooth gradient.
fn glow(size: i32) -> Pixels {
    let mut p = Pixels::new(size as u32, size as u32, CLEAR);
    let r = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 + 0.5 - r).powi(2) + (y as f32 + 0.5 - r).powi(2)).sqrt() / r;
            let strength = (1.0 - d).max(0.0).powf(1.6) * 16.0;
            if strength > BAYER[(y & 3) as usize][(x & 3) as usize] as f32 {
                p.set(x, y, [FLAME[0], FLAME[1], FLAME[2], 0x38]);
            }
        }
    }
    p
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonLook {
    Idle,
    Hot,
    Down,
    Disabled,
}

#[derive(Resource)]
pub struct Art {
    pub table: Handle<Image>,
    pub panel: Handle<Image>,
    pub field: Handle<Image>,
    buttons: [Handle<Image>; 4],
    pub candle: [Handle<Image>; 4],
    pub glow: Handle<Image>,
}

pub const PANEL_BORDER: f32 = 6.0;
pub const BUTTON_BORDER: f32 = 4.0;

impl Art {
    pub fn button(&self, look: ButtonLook) -> Handle<Image> {
        self.buttons[look as usize].clone()
    }
}

/// An image stretched as a nine-slice: corners keep their size, the middle fills the node.
pub fn sliced(image: Handle<Image>, border: f32) -> ImageNode {
    ImageNode {
        image,
        image_mode: NodeImageMode::Sliced(TextureSlicer {
            border: BorderRect::all(border),
            center_scale_mode: SliceScaleMode::Stretch,
            sides_scale_mode: SliceScaleMode::Stretch,
            max_corner_scale: 1.0,
        }),
        ..default()
    }
}

impl FromWorld for Art {
    fn from_world(world: &mut World) -> Self {
        let canvas = crate::screen::UI;
        let mut images = world.resource_mut::<Assets<Image>>();
        let mut add = |p: Pixels| images.add(p.into_image());
        Art {
        table: add(table(canvas.x, canvas.y)),
        panel: add(plate(16, INK, PARCHMENT_LIGHT, PARCHMENT, PARCHMENT_SHADE, true)),
        field: add(plate(12, INK, PARCHMENT_SHADE, PARCHMENT_LIGHT, PARCHMENT_LIGHT, false)),
        buttons: [
            add(plate(12, INK, WOOD_LIGHT, WOOD, WOOD_DARK, true)),
            add(plate(12, INK, BRASS_LIGHT, BRASS, BRASS_DARK, true)),
            add(plate(12, INK, WOOD_DEEP, WOOD_DARK, WOOD, false)),
            add(plate(12, WOOD_DEEP, WOOD_DARK, WOOD_DARK, WOOD_DARK, false)),
        ],
        candle: [add(candle(0)), add(candle(1)), add(candle(2)), add(candle(3))],
        glow: add(glow(96)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_map_through_the_legend_and_unknown_characters_stay_clear() {
        let p = Pixels::from_rows(&["k.", "?t"], &[('k', INK), ('t', TALLOW)]);
        assert_eq!((p.w, p.h), (2, 2));
        assert_eq!([p.get(0, 0), p.get(1, 0), p.get(0, 1), p.get(1, 1)], [INK, CLEAR, CLEAR, TALLOW]);
    }

    #[test]
    fn candle_frames_all_have_the_declared_size() {
        for frame in 0..4 {
            let c = candle(frame);
            assert_eq!(UVec2::new(c.w, c.h), CANDLE_SIZE);
        }
    }

    #[test]
    fn drawing_outside_the_picture_is_ignored() {
        let mut p = Pixels::new(2, 2, CLEAR);
        p.rect(-5, -5, 20, 20, INK);
        p.set(9, 9, WAX);
        assert_eq!(p.get(1, 1), INK);
        assert_eq!(p.get(9, 9), CLEAR);
    }

    #[test]
    fn table_is_the_same_on_every_build() {
        let a = table(64, 54);
        let b = table(64, 54);
        assert!((0..54).all(|y| (0..64).all(|x| a.get(x, y) == b.get(x, y))));
    }
}
