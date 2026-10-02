//! Colours and type of the shell: a game master's table lit by candles.

use bevy::prelude::*;
use bevy::text::FontSmoothing;

pub type Rgba = [u8; 4];

pub const CLEAR: Rgba = [0, 0, 0, 0];
pub const INK: Rgba = [0x1b, 0x14, 0x12, 0xff];
pub const WOOD_DEEP: Rgba = [0x24, 0x15, 0x0f, 0xff];
pub const WOOD_DARK: Rgba = [0x35, 0x1f, 0x14, 0xff];
pub const WOOD: Rgba = [0x4c, 0x2e, 0x1b, 0xff];
pub const WOOD_LIGHT: Rgba = [0x69, 0x41, 0x25, 0xff];
pub const BRASS_DARK: Rgba = [0x8a, 0x5a, 0x1e, 0xff];
pub const BRASS: Rgba = [0xc8, 0x96, 0x3c, 0xff];
pub const BRASS_LIGHT: Rgba = [0xf2, 0xcf, 0x6b, 0xff];
pub const PARCHMENT_DARK: Rgba = [0xa3, 0x82, 0x4e, 0xff];
pub const PARCHMENT_SHADE: Rgba = [0xcc, 0xb1, 0x7a, 0xff];
pub const PARCHMENT: Rgba = [0xe6, 0xd3, 0xa3, 0xff];
pub const PARCHMENT_LIGHT: Rgba = [0xf4, 0xe8, 0xc4, 0xff];
pub const WAX: Rgba = [0xa8, 0x32, 0x2a, 0xff];
pub const WAX_DARK: Rgba = [0x6e, 0x1c, 0x1c, 0xff];
pub const MOSS: Rgba = [0x3f, 0x66, 0x2a, 0xff];
pub const FLAME_CORE: Rgba = [0xff, 0xf4, 0xc2, 0xff];
pub const FLAME: Rgba = [0xff, 0xd0, 0x5a, 0xff];
pub const FLAME_EDGE: Rgba = [0xf0, 0x8a, 0x2c, 0xff];
pub const TALLOW: Rgba = [0xe9, 0xe0, 0xc8, 0xff];
pub const TALLOW_SHADE: Rgba = [0xb8, 0xa9, 0x8a, 0xff];

pub fn color(c: Rgba) -> Color {
    Color::srgba_u8(c[0], c[1], c[2], c[3])
}

/// Both fonts are drawn on an 8-unit grid: at size 8 one font pixel is one canvas pixel.
/// Sizes must stay multiples of 8.
pub const TEXT_PX: f32 = 8.0;

#[derive(Resource)]
pub struct Fonts {
    /// Tiny5: proportional, 5 pixels tall. Everything that has to be read.
    pub body: Handle<Font>,
    /// Press Start 2P: 8×8 cells. Titles only.
    pub title: Handle<Font>,
}

// Built when the resource is first asked for, because the first screen is spawned before
// any startup system runs.
impl FromWorld for Fonts {
    fn from_world(world: &mut World) -> Self {
        let mut fonts = world.resource_mut::<Assets<Font>>();
        let body = fonts.add(Font::from_bytes(include_bytes!("../assets/fonts/Tiny5-Regular.ttf").to_vec()));
        let title = fonts.add(Font::from_bytes(include_bytes!("../assets/fonts/PressStart2P-Regular.ttf").to_vec()));
        Fonts { body, title }
    }
}

fn pixel_font(font: &Handle<Font>, size: f32) -> TextFont {
    TextFont { font: font.clone().into(), font_size: FontSize::Px(size), font_smoothing: FontSmoothing::None, ..default() }
}

impl Fonts {
    pub fn body(&self) -> TextFont {
        pixel_font(&self.body, TEXT_PX)
    }

    pub fn title(&self, scale: u8) -> TextFont {
        pixel_font(&self.title, TEXT_PX * scale as f32)
    }
}
