//! The look of the interface: the shell's own type and the colours of the table, on egui.

use super::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke, TextStyle};
use crate::theme::*;

/// How much larger or smaller than the picture's own scale the interface is drawn, by the
/// player's choice of its size.
pub fn size_ratio(size: u8) -> f32 {
    match size {
        0 => 0.75,
        2 => 1.25,
        _ => 1.0,
    }
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
        // Every text is ink (`override_text_color`), so the fields, lists and numbers a
        // player edits are light: dark wood behind ink could not be read.
        (&mut v.widgets.inactive, c(PARCHMENT_SHADE), c(INK)),
        (&mut v.widgets.hovered, c(BRASS_LIGHT), c(INK)),
        (&mut v.widgets.active, c(BRASS), c(INK)),
        (&mut v.widgets.open, c(BRASS_LIGHT), c(INK)),
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
    fn the_interface_is_a_step_smaller_or_larger_by_choice() {
        assert_eq!([size_ratio(0), size_ratio(1), size_ratio(2)], [0.75, 1.0, 1.25]);
    }
}
