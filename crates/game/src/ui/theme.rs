//! The look of the interface: the shell's own type and the colours of the table, on egui.

use super::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke, TextStyle};
use crate::theme::*;

/// Physical pixels to a unit of the interface, for the screen the player chose (0 lets the
/// window's height decide, then Full HD, 2K and 4K) and the size (0 smaller, 1 as is, 2
/// larger). A window smaller than the screen chosen gets a smaller unit, so that nothing
/// is cut off: everything is laid out in at least 800 by 450 units.
pub fn zoom(window_height: u32, screen: u8, size: u8) -> f32 {
    let screen = match screen {
        1..=3 => screen,
        _ if window_height < 1300 => 1,
        _ if window_height < 1900 => 2,
        _ => 3,
    };
    let base = match screen {
        1 | 2 => 2.0,
        _ => 3.0,
    };
    let unit = (base + size.min(2) as f32 - 1.0).max(1.0);
    let fits = window_height.max(1) as f32 / 450.0;
    unit.min(fits)
}

pub fn c(rgba: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3])
}

/// The family of the titles; the rest of the text is the body face.
pub fn title_family() -> FontFamily {
    FontFamily::Name("title".into())
}

/// Sizes of type, in units. The body face is drawn a pixel to a unit at nine; the titles'
/// at eight.
pub const BODY: f32 = 9.0;
pub const BIG: f32 = 18.0;
pub const TITLE: f32 = 16.0;
pub const SMALL_TITLE: f32 = 8.0;

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("monocraft".into(), FontData::from_static(include_bytes!("../../assets/fonts/Monocraft.ttf")).into());
    fonts.font_data.insert("press_start".into(), FontData::from_static(include_bytes!("../../assets/fonts/PressStart2P-Regular.ttf")).into());
    // The game's face first; egui's own behind it, for any sign the game's does not have.
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(family).or_default().insert(0, "monocraft".into());
    }
    fonts.families.insert(title_family(), vec!["press_start".into(), "monocraft".into()]);
    ctx.set_fonts(fonts);
    ctx.global_style_mut(style);
}

fn style(style: &mut egui::Style) {
    use TextStyle::*;
    style.text_styles = [
        (Small, FontId::proportional(BODY)),
        (Body, FontId::proportional(BODY)),
        (Monospace, FontId::monospace(BODY)),
        (Button, FontId::proportional(BODY)),
        (Heading, FontId::new(TITLE, title_family())),
    ]
    .into();
    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(4.0, 3.0);
    s.button_padding = egui::vec2(6.0, 3.0);
    s.interact_size = egui::vec2(20.0, 16.0);
    s.window_margin = Margin::same(8);
    s.menu_margin = Margin::same(4);
    s.indent = 10.0;
    s.scroll.bar_width = 6.0;
    s.scroll.floating = false;
    s.icon_width = 9.0;
    s.icon_spacing = 4.0;
    style.animation_time = 0.08;
    let v = &mut style.visuals;
    *v = egui::Visuals::light();
    v.override_text_color = Some(c(INK));
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = c(PARCHMENT);
    v.window_stroke = Stroke::new(1.0, c(INK));
    v.window_corner_radius = CornerRadius::ZERO;
    v.window_shadow = egui::Shadow::NONE;
    v.popup_shadow = egui::Shadow::NONE;
    v.menu_corner_radius = CornerRadius::ZERO;
    v.extreme_bg_color = c(PARCHMENT_LIGHT);
    v.faint_bg_color = c(PARCHMENT_SHADE);
    v.code_bg_color = c(PARCHMENT_SHADE);
    v.hyperlink_color = c(WAX_DARK);
    v.warn_fg_color = c(BRASS_DARK);
    v.error_fg_color = c(WAX);
    v.selection.bg_fill = c(BRASS_LIGHT);
    v.selection.stroke = Stroke::new(1.0, c(INK));
    v.text_cursor.stroke = Stroke::new(1.0, c(INK));
    for (w, fill, text) in [
        (&mut v.widgets.noninteractive, c(PARCHMENT), c(INK)),
        (&mut v.widgets.inactive, c(WOOD), c(PARCHMENT)),
        (&mut v.widgets.hovered, c(BRASS), c(INK)),
        (&mut v.widgets.active, c(BRASS_DARK), c(PARCHMENT_SHADE)),
        (&mut v.widgets.open, c(WOOD_DARK), c(PARCHMENT)),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0, c(INK));
        w.fg_stroke = Stroke::new(1.0, text);
        w.corner_radius = CornerRadius::ZERO;
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, c(PARCHMENT_DARK));
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, c(INK));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unit_follows_the_screen_chosen_and_never_cuts_anything_off() {
        // Left to the window: Full HD and 2K are two pixels a unit, 4K three.
        assert_eq!([zoom(1080, 0, 1), zoom(1440, 0, 1), zoom(2160, 0, 1)], [2.0, 2.0, 3.0]);
        // Chosen by hand, and a step smaller or larger.
        assert_eq!([zoom(2160, 1, 1), zoom(2160, 3, 0), zoom(2160, 3, 2)], [2.0, 2.0, 4.0]);
        // A window lower than the screen chosen: everything still fits in 450 units.
        assert_eq!(zoom(768, 1, 1), 768.0 / 450.0);
        assert_eq!(zoom(720, 0, 2), 1.6);
        // Never below one pixel a unit on a screen that has room for it.
        assert_eq!(zoom(1080, 1, 0), 1.0);
    }
}
