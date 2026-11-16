//! The interface's own controls, on egui, in the look of the table: wooden and brass
//! plates with a light edge and a shade, parchment sheets. Everything a screen is made of
//! comes from here, so that no screen decides sizes or colours of its own.

use super::egui::{self, Align, Color32, CursorIcon, Key, Layout, Pos2, Rect, Response, Sense, Stroke, TextStyle, Ui, UiBuilder, Vec2, WidgetInfo, WidgetText, WidgetType, vec2};
use super::theme::{BODY, c, title_family};
use crate::theme::*;

/// Whether the keyboard is steering, as `ui::steering` last found: the focus is drawn only then.
pub fn steering(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(egui::Id::new("steering"))).unwrap_or(false)
}

pub fn set_steering(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("steering"), on));
}

/// The colours of a plate: the line round it, its lit edge, its face and its shaded edge.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Look {
    Wood,
    Brass,
    Pressed,
    Off,
    Parchment,
    Field,
}

impl Look {
    fn colours(self) -> (Rgba, Rgba, Rgba, Rgba) {
        match self {
            Look::Wood => (INK, WOOD_LIGHT, WOOD, WOOD_DARK),
            Look::Brass => (INK, BRASS_LIGHT, BRASS, BRASS_DARK),
            Look::Pressed => (INK, WOOD_DEEP, WOOD_DARK, WOOD),
            Look::Off => (WOOD_DEEP, WOOD_DARK, WOOD_DARK, WOOD_DARK),
            Look::Parchment => (INK, PARCHMENT_LIGHT, PARCHMENT, PARCHMENT_SHADE),
            Look::Field => (INK, PARCHMENT_SHADE, PARCHMENT_LIGHT, PARCHMENT_LIGHT),
        }
    }

    /// The colour of words written on it.
    pub fn ink(self) -> Color32 {
        c(match self {
            Look::Wood => PARCHMENT,
            Look::Brass => INK,
            Look::Pressed => PARCHMENT_SHADE,
            Look::Off => WOOD_LIGHT,
            Look::Parchment | Look::Field => INK,
        })
    }
}

/// Paints a plate as the shell's pictures are drawn: a line of ink round it, the corners
/// cut, the top and left edges lit, the bottom and right shaded, and a shadow under it.
pub fn plate(painter: &egui::Painter, rect: Rect, look: Look, shadow: bool) {
    let (outline, light, fill, shade) = look.colours();
    let unit = 1.0;
    if shadow {
        painter.rect_filled(rect.translate(vec2(unit, unit)), 0.0, Color32::from_black_alpha(0x70));
    }
    // The ink line, with each corner pixel left out.
    painter.rect_filled(rect.shrink2(vec2(unit, 0.0)), 0.0, c(outline));
    painter.rect_filled(rect.shrink2(vec2(0.0, unit)), 0.0, c(outline));
    let inner = rect.shrink(unit);
    painter.rect_filled(inner, 0.0, c(fill));
    let (l, t, r, b) = (inner.left(), inner.top(), inner.right(), inner.bottom());
    painter.rect_filled(Rect::from_min_max(Pos2::new(l, t), Pos2::new(r, t + unit)), 0.0, c(light));
    painter.rect_filled(Rect::from_min_max(Pos2::new(l, t), Pos2::new(l + unit, b)), 0.0, c(light));
    painter.rect_filled(Rect::from_min_max(Pos2::new(l, b - unit), Pos2::new(r, b)), 0.0, c(shade));
    painter.rect_filled(Rect::from_min_max(Pos2::new(r - unit, t), Pos2::new(r, b)), 0.0, c(shade));
    // The corners inside the line are ink again, as on the pictures.
    for corner in [Pos2::new(l, t), Pos2::new(r - unit, t), Pos2::new(l, b - unit), Pos2::new(r - unit, b - unit)] {
        painter.rect_filled(Rect::from_min_size(corner, Vec2::splat(unit)), 0.0, c(outline));
    }
}

/// What a button is for, which decides its colour at rest.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Kind {
    #[default]
    Plain,
    /// The one thing the screen is for.
    Primary,
}

/// A button. It acts when pressed with the mouse, or with Enter or Space while it has the
/// keyboard's focus. One that cannot be pressed says why under the cursor.
#[must_use = "add it with ui.add(…)"]
pub struct Button<'a> {
    text: WidgetText,
    kind: Kind,
    enabled: bool,
    why_not: Option<&'a str>,
    key: Option<&'a str>,
    /// Take the whole width offered, words wrapped onto at most three rows.
    wide: bool,
    /// Its words in the middle, as in a menu.
    centred: bool,
}

pub fn button<'a>(text: impl Into<WidgetText>) -> Button<'a> {
    Button { text: text.into(), kind: Kind::Plain, enabled: true, why_not: None, key: None, wide: false, centred: false }
}

impl<'a> Button<'a> {
    pub fn primary(mut self) -> Self {
        self.kind = Kind::Primary;
        self
    }

    /// Switched off when `enabled` is false, with the reason to show.
    pub fn enabled(mut self, enabled: bool, why_not: &'a str) -> Self {
        self.enabled = enabled;
        self.why_not = Some(why_not).filter(|_| !enabled);
        self
    }

    /// The key that presses it, written small at its right.
    pub fn key(mut self, key: &'a str) -> Self {
        self.key = Some(key);
        self
    }

    pub fn wide(mut self) -> Self {
        self.wide = true;
        self
    }

    /// As wide as offered, the words in the middle.
    pub fn menu(mut self) -> Self {
        self.wide = true;
        self.centred = true;
        self
    }
}

/// How far the words stand from the plate's edges, and the least height of a control.
const PAD: Vec2 = vec2(6.0, 3.0);
pub const ROW: f32 = 16.0;
/// How wide the value of a setting is at least, so the arrows of a sheet stand in one column.
const VALUE_COLUMN: f32 = 130.0;
/// A label written on a control is at most this many rows; longer is cut with "…" and
/// shown whole under the cursor.
const MOST_ROWS: usize = 3;

impl egui::Widget for Button<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let Button { text, kind, enabled, why_not, key, wide, centred } = self;
        let wrap_width = if wide { ui.available_width() - PAD.x * 2.0 } else { f32::INFINITY };
        let mut job = std::sync::Arc::unwrap_or_clone(text.clone().into_layout_job(ui.style(), egui::FontSelection::Style(TextStyle::Button), Align::LEFT));
        job.wrap.max_width = wrap_width;
        job.wrap.max_rows = if wide { MOST_ROWS } else { 1 };
        job.wrap.break_anywhere = false;
        // Laid out from its left edge; centred, where it is, by where it is drawn.
        job.halign = Align::LEFT;
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let cut = galley.elided;
        let hint = key.map(|k| ui.fonts_mut(|f| f.layout_no_wrap(k.to_string(), egui::FontId::proportional(BODY), c(FADED))));
        let hint_w = hint.as_ref().map_or(0.0, |h| h.size().x + PAD.x);
        let mut size = galley.size() + PAD * 2.0 + vec2(hint_w, 0.0);
        size.y = size.y.max(ROW);
        if wide {
            size.x = ui.available_width();
        }
        let sense = if enabled { Sense::click() } else { Sense::hover() };
        let (rect, mut response) = ui.allocate_exact_size(size, sense);
        let label = text.text().to_string();
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, &label));
        if ui.is_rect_visible(rect) {
            let down = response.is_pointer_button_down_on();
            let shown_focus = response.has_focus() && steering(ui.ctx());
            let lit = response.hovered() || shown_focus;
            let look = match (enabled, down, lit, kind) {
                (false, ..) => Look::Off,
                (_, true, ..) => Look::Pressed,
                (_, _, true, _) | (_, _, _, Kind::Primary) => Look::Brass,
                _ => Look::Wood,
            };
            plate(ui.painter(), rect, look, enabled && !down);
            // A pressed plate sits lower, and its words with it.
            let sink = if down { 1.0 } else { 0.0 };
            let left = if centred { rect.center().x - galley.size().x / 2.0 } else { rect.left() + PAD.x };
            let at = Pos2::new(left, rect.center().y - galley.size().y / 2.0);
            ui.painter().galley(at + vec2(0.0, sink), galley, look.ink());
            if let Some(hint) = hint {
                let at = Pos2::new(rect.right() - PAD.x - hint.size().x, rect.center().y - hint.size().y / 2.0 + sink);
                ui.painter().galley(at, hint, if look == Look::Brass { c(BRASS_DARK) } else { c(PARCHMENT_DARK) });
            }
            // The keyboard's focus is drawn whether or not the mouse is near.
            if shown_focus {
                ui.painter().rect_stroke(rect.expand(1.0), 0.0, Stroke::new(1.0, c(BRASS_LIGHT)), egui::StrokeKind::Outside);
            }
        }
        if enabled {
            response = response.on_hover_cursor(CursorIcon::PointingHand);
        }
        if let Some(why) = why_not {
            response = response.on_hover_text(why);
        } else if cut {
            response = response.on_hover_text(label);
        }
        response
    }
}

/// A setting with a few values: its name at the left, `◀ value ▶` at the right. The arrows,
/// a click on the value, ←/→ while it has the focus, and the wheel over it move the value
/// either way. The value's place is as wide as its longest value, so nothing moves when it
/// changes. Gives whether it changed.
pub fn choice(ui: &mut Ui, name: &str, values: &[&str], index: &mut usize) -> Response {
    let count = values.len().max(1);
    let widest = values.iter().map(|v| ui.fonts_mut(|f| f.layout_no_wrap(v.to_string(), egui::FontId::proportional(BODY), Color32::WHITE).size().x)).fold(0.0f32, f32::max);
    let arrow = ROW;
    // One column for every setting on a sheet, unless a value is longer.
    let value_w = (widest + PAD.x * 2.0).max(VALUE_COLUMN);
    let width = ui.available_width();
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, ROW), Sense::click());
    let row = rect;
    let right = row.right();
    let next_rect = Rect::from_min_size(Pos2::new(right - arrow, row.top()), Vec2::splat(arrow));
    let value_rect = Rect::from_min_size(Pos2::new(next_rect.left() - 2.0 - value_w, row.top()), vec2(value_w, ROW));
    let prev_rect = Rect::from_min_size(Pos2::new(value_rect.left() - 2.0 - arrow, row.top()), Vec2::splat(arrow));
    let mut step = 0i32;
    if response.clicked()
        && let Some(at) = response.interact_pointer_pos()
    {
        step = if prev_rect.contains(at) { -1 } else { 1 };
    }
    if response.has_focus() {
        ui.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowLeft) {
                step = -1;
            }
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowRight) {
                step = 1;
            }
        });
    }
    if response.hovered() {
        let wheel = ui.input(|i| i.smooth_scroll_delta.y);
        if wheel != 0.0 {
            step = if wheel > 0.0 { 1 } else { -1 };
        }
    }
    if step != 0 {
        *index = ((*index as i32 + step).rem_euclid(count as i32)) as usize;
        response.mark_changed();
    }
    let shown = values.get(*index).copied().unwrap_or("");
    let focused = response.has_focus();
    response.widget_info(|| WidgetInfo::selected(WidgetType::ComboBox, true, focused, format!("{name}: {shown}")));
    if ui.is_rect_visible(row) {
        let p = ui.painter();
        let shown_focus = response.has_focus() && steering(ui.ctx());
        let lit = response.hovered() || shown_focus;
        if lit {
            p.rect_filled(row, 0.0, c(PARCHMENT_SHADE));
        }
        p.text(Pos2::new(row.left() + 2.0, row.center().y), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(BODY), c(INK));
        let pointer = response.hover_pos();
        for (r, sign) in [(prev_rect, "◀"), (next_rect, "▶")] {
            let hot = pointer.is_some_and(|at| r.contains(at));
            plate(p, r, if hot { Look::Brass } else { Look::Wood }, true);
            let look = if hot { Look::Brass } else { Look::Wood };
            p.text(r.center(), egui::Align2::CENTER_CENTER, sign, egui::FontId::proportional(BODY), look.ink());
        }
        p.text(value_rect.center(), egui::Align2::CENTER_CENTER, shown, egui::FontId::proportional(BODY), c(INK));
        if shown_focus {
            p.rect_stroke(row.expand(1.0), 0.0, Stroke::new(1.0, c(BRASS_DARK)), egui::StrokeKind::Outside);
        }
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A setting that is on or off: `[✔]` and its name. Gives whether it changed.
pub fn toggle(ui: &mut Ui, name: &str, on: &mut bool) -> Response {
    let width = ui.available_width();
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, ROW), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, *on, name));
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let shown_focus = response.has_focus() && steering(ui.ctx());
        if response.hovered() || shown_focus {
            p.rect_filled(rect, 0.0, c(PARCHMENT_SHADE));
        }
        p.text(Pos2::new(rect.left() + 2.0, rect.center().y), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(BODY), c(INK));
        let box_rect = Rect::from_min_size(Pos2::new(rect.right() - ROW, rect.top()), Vec2::splat(ROW));
        plate(p, box_rect, Look::Field, false);
        if *on {
            p.text(box_rect.center(), egui::Align2::CENTER_CENTER, "✔", egui::FontId::proportional(BODY), c(MOSS));
        }
        if shown_focus {
            p.rect_stroke(rect.expand(1.0), 0.0, Stroke::new(1.0, c(BRASS_DARK)), egui::StrokeKind::Outside);
        }
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Words in the faded ink of hints.
pub fn hint(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(text.into()).color(c(FADED))).wrap())
}

/// Words in red, for what is wrong.
pub fn warning(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.add(egui::Label::new(egui::RichText::new(text.into()).color(c(WAX))).wrap())
}

/// A title in the titles' face.
pub fn title(ui: &mut Ui, text: impl Into<String>, size: f32) -> Response {
    ui.label(egui::RichText::new(text.into()).font(egui::FontId::new(size, title_family())).color(c(INK)))
}

/// How a sheet sits on the screen.
pub struct Sheet<'a> {
    pub id: &'a str,
    pub title: &'a str,
    /// The widest and tallest it may be, in units; a smaller screen makes it smaller.
    pub max: Vec2,
    /// Room left from the top of the screen, for a title of the game above the sheet.
    pub top: f32,
    /// Always as tall as its most, not as its contents: for a sheet in steps, whose buttons
    /// must stand in one place from step to step.
    pub fixed: bool,
}

/// The margin round a sheet, and inside it.
const SHEET_GAP: f32 = 8.0;
const SHEET_PAD: f32 = 10.0;

/// A parchment sheet: its title at the top, `footer` pinned to the bottom, and `body`
/// between them, scrolled when it is longer than there is room for. The buttons of the
/// footer therefore stay where they are whatever the body holds. The sheet is as tall as
/// its contents need, up to its most.
pub fn sheet<R>(ui: &mut Ui, sheet: Sheet, body: impl FnOnce(&mut Ui) -> R, mut footer: impl FnMut(&mut Ui)) -> R {
    sheet_with(ui, sheet, body, Some(&mut footer))
}

/// A sheet with nothing pinned to its bottom: a menu, a list that is all buttons.
pub fn bare_sheet<R>(ui: &mut Ui, sheet: Sheet, body: impl FnOnce(&mut Ui) -> R) -> R {
    sheet_with(ui, sheet, body, None)
}

fn sheet_with<R>(ui: &mut Ui, sheet: Sheet, body: impl FnOnce(&mut Ui) -> R, footer: Option<&mut dyn FnMut(&mut Ui)>) -> R {
    let screen = ui.max_rect();
    let id = ui.id().with(sheet.id);
    let room = vec2(screen.width() - SHEET_GAP * 2.0, screen.height() - SHEET_GAP * 2.0 - sheet.top);
    let width = sheet.max.x.min(room.x).max(1.0);
    // The height the contents asked for the last time they were laid out.
    let wanted: Option<f32> = ui.data(|d| d.get_temp(id));
    let height = if sheet.fixed { sheet.max.y } else { wanted.unwrap_or(sheet.max.y) }.min(sheet.max.y).min(room.y).max(1.0);
    let rect = Rect::from_min_size(Pos2::new(screen.center().x - width / 2.0, screen.top() + SHEET_GAP + sheet.top), vec2(width, height));
    plate(ui.painter(), rect, Look::Parchment, true);
    let inner = rect.shrink(SHEET_PAD);
    let mut child = ui.new_child(UiBuilder::new().id_salt(sheet.id).max_rect(inner).layout(Layout::top_down(Align::Min)));
    let mut used = 0.0;
    if !sheet.title.is_empty() {
        let r = title(&mut child, sheet.title, super::theme::SMALL_TITLE);
        used += r.rect.height() + child.spacing().item_spacing.y + 2.0;
        child.add_space(2.0);
    }
    // The footer is laid out first, at the bottom, so the body knows what room is left.
    let footer_h = match footer {
        Some(footer) => {
            let mut f = child.new_child(UiBuilder::new().id_salt("footer").max_rect(inner).layout(Layout::bottom_up(Align::Min)));
            f.horizontal_wrapped(|row| {
                row.spacing_mut().item_spacing.x = 6.0;
                footer(row)
            });
            f.min_rect().height()
        }
        None => 0.0,
    };
    let gap = if footer_h > 0.0 { child.spacing().item_spacing.y + 2.0 } else { 0.0 };
    let body_room = (inner.height() - used - footer_h - gap).max(0.0);
    let mut content_h = 0.0;
    let out = egui::ScrollArea::vertical()
        .id_salt("body")
        .max_height(body_room)
        .auto_shrink([false, true])
        .show(&mut child, |ui| {
            let out = body(ui);
            content_h = ui.min_rect().height();
            out
        })
        .inner;
    let needed = SHEET_PAD * 2.0 + used + content_h + gap + footer_h;
    if !sheet.fixed && wanted.is_none_or(|w| (w - needed).abs() > 0.5) {
        ui.data_mut(|d| d.insert_temp(id, needed));
        ui.ctx().request_discard("a sheet measured its contents");
    }
    out
}

/// The whole screen as a Ui to lay a screen out in, under everything else egui draws.
/// None until the game's type is in place: egui takes new fonts from the next frame on.
pub fn screen_ui(ctx: &egui::Context, id: &'static str) -> Option<Ui> {
    if !ready(ctx) {
        return None;
    }
    Some(Ui::new(ctx.clone(), egui::Id::new(id), UiBuilder::new().layer_id(egui::LayerId::background()).max_rect(ctx.content_rect())))
}

/// Whether the game's own type has reached egui.
pub fn ready(ctx: &egui::Context) -> bool {
    ctx.fonts(|f| f.definitions().families.contains_key(&title_family()))
}

/// Whether Escape (or the pad's B, which is turned into it) was pressed this frame and
/// nothing else took it.
pub fn back_pressed(ui: &Ui) -> bool {
    ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn a_button_is_pressed_by_a_click_and_a_switched_off_one_is_not() {
        let mut pressed = 0;
        let mut off_pressed = 0;
        let mut harness = Harness::new_ui(|ui| {
            super::super::theme::install(ui.ctx());
            if ui.add(button("Go")).clicked() {
                pressed += 1;
            }
            if ui.add(button("Stay").enabled(false, "not now")).clicked() {
                off_pressed += 1;
            }
        });
        harness.get_by_label("Go").click();
        harness.run();
        harness.get_by_label("Stay").click();
        harness.run();
        drop(harness);
        assert_eq!((pressed, off_pressed), (1, 0));
    }

    #[test]
    fn a_choice_moves_both_ways_and_comes_round() {
        let mut index = 0usize;
        {
            let mut harness = Harness::new_ui(|ui| {
                super::super::theme::install(ui.ctx());
                choice(ui, "Difficulty", &["casual", "honest", "hardcore"], &mut index);
            });
            harness.get_by_label("Difficulty: casual").click();
            harness.run();
            // ←, with the focus on it, goes back; from the first value it comes round to the last.
            harness.get_by_label("Difficulty: honest").focus();
            harness.key_press(egui::Key::ArrowLeft);
            harness.run();
            harness.key_press(egui::Key::ArrowLeft);
            harness.run();
        }
        assert_eq!(index, 2);
    }

    #[test]
    fn a_toggle_flips_and_says_so() {
        let mut on = false;
        {
            let mut harness = Harness::new_ui(|ui| {
                super::super::theme::install(ui.ctx());
                toggle(ui, "Live world", &mut on);
            });
            harness.get_by_label("Live world").click();
            harness.run();
        }
        assert!(on);
    }
}
