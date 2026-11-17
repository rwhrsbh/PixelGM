//! The bag drawn with egui: the hero with what it wears, the grid of the bag under it, the
//! companions or a shop beside it, and the things kept at hand in the strip at the bottom.
//! Nothing here changes the world: a click is written to `Clicks` for `press` and `outside`.

use super::*;
use crate::ui::egui::{self, Align, Color32, Layout, Pos2, Rect, RichText, Sense, Stroke, UiBuilder, vec2};
use crate::ui::kit::{self, button, hint, warning};
use crate::ui::theme::c;
use bevy_egui::{EguiContexts, EguiTextureHandle};

/// The size of a cell of the hero's gear, in units.
const GEAR_CELL: f32 = 20.0;
const PAD: f32 = 10.0;
/// The widest a bag or a shop is: a grid of cells of 32 units and the margins.
const PANEL_MAX: f32 = BAG_W as f32 * 32.0 + 2.0 * PAD;

/// The size of a cell of a grid in a panel, in units: as large as the room under the head
/// leaves, never more than two of the pictures' pixels to one of the interface's.
pub fn cell_for(width: f32, height: f32, rows: f32) -> f32 {
    (height / rows).min(width / BAG_W as f32).clamp(8.0, 32.0).floor()
}
/// How wide the companions' panel is.
const SIDE_W: f32 = 184.0;
/// How tall the row of the hero and its gear is.
const GEAR_ROW: f32 = 4.0 * GEAR_CELL;

/// What was clicked in the bag this frame, and whether a thing in hand was thrown outside it.
#[derive(Resource, Default)]
pub struct Clicks {
    /// The place, and whether it was the right button.
    pub hits: Vec<(Target, bool)>,
    pub outside: bool,
}

/// The pictures of things as egui knows them, by what shows on them, with their size in pixels.
#[derive(Resource, Default)]
pub struct Pictures(HashMap<String, (egui::TextureId, Vec2)>);

/// A thing's picture for egui: made once and kept.
pub fn picture_id(pictures: &mut Pictures, egui: &mut EguiContexts, images: &mut Assets<Image>, item: &Item, world: &World) -> (egui::TextureId, Vec2) {
    let key = icon_key(item);
    if let Some(found) = pictures.0.get(&key) {
        return *found;
    }
    let pixels = picture(item, world);
    let size = Vec2::new(pixels.w as f32, pixels.h as f32);
    let id = egui.add_image(EguiTextureHandle::Strong(images.add(pixels.into_image())));
    pictures.0.insert(key, (id, size));
    (id, size)
}

/// Where the bag stands, over the right of the map beside the party's column, and where the
/// panel beside it does (the shop, as wide as the bag, or the companions).
pub fn panels(screen: Rect, wide_side: bool) -> (Rect, Rect) {
    let party = crate::view::COLUMN * UI_PIXEL;
    let strip = crate::play::HUD_HEIGHT * UI_PIXEL;
    let map = Rect::from_min_max(screen.min, Pos2::new(screen.right() - party, screen.bottom() - strip));
    let panel_w = PANEL_MAX.min(map.width() * 0.5 - 8.0).max(200.0);
    let bag = Rect::from_min_size(Pos2::new(map.right() - panel_w - 4.0, map.top() + 4.0), vec2(panel_w, map.height() - 8.0));
    let width = if wide_side { panel_w } else { SIDE_W };
    let side = Rect::from_min_size(Pos2::new((bag.left() - width - 6.0).max(map.left() + 2.0), bag.top()), vec2(width, bag.height()));
    (bag, side)
}

/// Where each piece of gear is in the row of the hero, from the row's left, and its size in cells.
fn slot_box(slot: Slot) -> (f32, u8, u8) {
    match slot {
        Slot::Main => (0.0, 2, 4),
        Slot::Body => (48.0, 2, 3),
        Slot::Shield => (156.0, 2, 2),
        Slot::Spare => (204.0, 2, 4),
    }
}
const DOLL_LEFT: f32 = 96.0;
const LOOSE_LEFT: f32 = 252.0;

/// A picture fitted into a place, its shape kept and never larger than twice its pixels.
fn fitted(place: Rect, size: Vec2) -> Rect {
    let scale = (place.width() / size.x.max(1.0)).min(place.height() / size.y.max(1.0)).min(UI_PIXEL);
    Rect::from_center_size(place.center(), vec2(size.x * scale, size.y * scale))
}

const WHOLE: Rect = Rect { min: Pos2::ZERO, max: Pos2::new(1.0, 1.0) };

/// What a thing is, in the colours the lines were written for: on a dark card.
pub fn lines_ui(ui: &mut egui::Ui, lines: &[(String, Rgba)]) {
    egui::Frame::new().fill(Color32::from_rgba_unmultiplied(0x10, 0x0a, 0x08, 0xf0)).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
        ui.set_max_width(260.0);
        for (text, tint) in lines {
            ui.add(egui::Label::new(RichText::new(text).color(c(*tint))).wrap());
        }
    });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn draw(
    mut egui: EguiContexts,
    (mut phase, game, session, settings, lang, shopping): (ResMut<Phase>, Res<Game>, Res<Session>, Res<Settings>, Res<Lang>, Res<crate::shop_ui::Shopping>),
    (mut state, mut clicks, mut pictures, mut images): (ResMut<BagState>, ResMut<Clicks>, ResMut<Pictures>, ResMut<Assets<Image>>),
    mut sound: MessageWriter<Play>,
) {
    if *phase != Phase::Bag {
        return;
    }
    let world = &game.world;
    let lang = *lang;
    let ru = lang == Lang::Ru;
    let t = |text: T| text.text(lang);
    let mine = my_heroes(world, &session, &settings);
    if !mine.contains(&state.hero) {
        state.hero = mine.first().copied().unwrap_or(0);
    }
    let h = state.hero;
    let hero = if mine.contains(&h) { world.party.get(h) } else { None };

    // Every picture this frame shows, handed to egui before anything is laid out.
    let mut pic = |item: &Item| picture_id(&mut pictures, &mut egui, &mut images, item, world);
    let placed: Vec<(usize, (u8, u8), (u8, u8), (egui::TextureId, Vec2))> = bag(world, h).iter().enumerate().filter_map(|(i, item)| item.at.map(|at| (i, at, item.size(), pic(item)))).collect();
    let worn_item = |slot: Slot| -> Option<Item> {
        if slot == Slot::Shield && world.held_light(h).is_some() {
            return world.held_light(h).cloned();
        }
        let index = hero.and_then(|hero| worn(hero, slot))?;
        // A thing of the master's own keeps its own picture and name while worn.
        let own = world.story.bags.get(h).and_then(|b| b.worn_as.iter().find(|i| i.gear.as_deref() == Some(index.as_str()))).cloned();
        Some(own.unwrap_or(Item { name: gear_name(&index, false), gear: Some(index), count: 1, ..default() }))
    };
    let worn_pics: Vec<(Slot, Option<Item>, Option<(egui::TextureId, Vec2)>)> = Slot::ALL
        .iter()
        .map(|slot| {
            let item = worn_item(*slot);
            let shown = item.as_ref().map(&mut pic);
            (*slot, item, shown)
        })
        .collect();
    let held_item = match &state.held {
        Some(Held::Item(i)) => bag(world, h).get(*i).cloned(),
        Some(Held::Worn(_, slot)) => worn_item(*slot),
        None => None,
    };
    let held_pic = held_item.as_ref().map(|item| (pic(item), item.size()));
    #[allow(clippy::type_complexity)]
    let with: Vec<(usize, String, Option<(egui::TextureId, Vec2)>, Option<(egui::TextureId, Vec2)>, Vec<((egui::TextureId, Vec2), u32, Vec<(String, Rgba)>)>, u32)> = world
        .companions_here()
        .into_iter()
        .map(|n| {
            let companion = &world.companions[n];
            let pack = companion.pack.iter().map(|i| (pic(i), i.count, item_lines(i, lang))).collect();
            (n, companion.creature.name.clone(), companion.weapon.as_ref().map(&mut pic), companion.armor.as_ref().map(&mut pic), pack, companion.pack_used())
        })
        .collect();
    let doll = hero.map(|hero| {
        let light = world.held_light(h).is_some();
        let key = format!("doll|{}|{light}", crate::doll::key(hero));
        if let Some(found) = pictures.0.get(&key) {
            return *found;
        }
        let frame = crate::doll::frames_holding(hero, light).swap_remove(0);
        let size = Vec2::new(frame.w as f32, frame.h as f32);
        let id = egui.add_image(EguiTextureHandle::Strong(images.add(frame.into_image())));
        pictures.0.insert(key, (id, size));
        (id, size)
    });

    let Ok(ctx) = egui.ctx_mut() else { return };
    let Some(mut ui) = kit::screen_ui(ctx, "bag") else {
        return;
    };
    let screen = ui.max_rect();
    let shop = shopping.0.is_some();
    let (rect, side) = panels(screen, shop);
    let mut hits: Vec<(Target, bool)> = Vec::new();
    let mut under: Option<Target> = None;
    let mut close = false;
    state.over = None;
    let held = state.held.clone();
    let held_slot = match &held {
        Some(Held::Worn(_, slot)) => Some(*slot),
        _ => None,
    };
    let held_index = match &held {
        Some(Held::Item(i)) => Some(*i),
        _ => None,
    };

    kit::plate(ui.painter(), rect, kit::Look::Parchment, true);
    let inner = rect.shrink(PAD);
    // The head: whose bag, how to use it, armour, wounds and the purse.
    let mut head = ui.new_child(UiBuilder::new().id_salt("bag_head").max_rect(inner).layout(Layout::top_down(Align::Min)));
    head.horizontal(|row| {
        kit::title(row, hero.map_or(String::new(), |hero| hero.name.clone()), crate::ui::theme::SMALL_TITLE);
        row.with_layout(Layout::right_to_left(Align::Center), |row| {
            if row.add(button(t(T::CloseBag)).key("I")).clicked() {
                close = true;
            }
            if mine.len() > 1 {
                if row.add(button(">")).on_hover_text(t(T::OtherHero)).clicked() {
                    hits.push((Target::NextHero, false));
                }
                if row.add(button("<")).on_hover_text(t(T::OtherHero)).clicked() {
                    hits.push((Target::PrevHero, false));
                }
            }
        });
    });
    // Armour, wounds and the purse under the name.
    if let Some(hero) = hero {
        head.horizontal(|row| {
            row.label(format!("{} {} · {} {}/{}", t(T::ArmorClass), hero.ac(), t(T::HitPoints), hero.hp, hero.max_hp));
            let gold = world.story.bags.get(h).map_or(0, |b| b.gold);
            if row.add(button(format!("{} {gold}", t(T::GoldCount)))).on_hover_text(t(T::GoldDropHint)).clicked() {
                hits.push((Target::Gold, false));
            }
        });
    }
    hint(&mut head, t(T::BagAbout));
    let top = head.min_rect().bottom() + 6.0;
    // The footer at the bottom first, so that the grid knows how much room is left.
    let mut foot = ui.new_child(UiBuilder::new().id_salt("bag_foot").max_rect(inner).layout(Layout::bottom_up(Align::Min)));
    hint(&mut foot, t(T::BagQuickHint));
    hint(&mut foot, t(T::BagUseHint));
    let room = foot.min_rect().top() - 6.0 - top - 6.0;
    // The hero's row is two and a half cells of the grid tall, and shrinks with it.
    #[allow(non_snake_case)]
    let GRID_CELL = cell_for(inner.width(), room, BAG_H as f32 + GEAR_ROW / 32.0);
    let k = GRID_CELL / 32.0;
    let (gear_cell, gear_row) = (GEAR_CELL * k, GEAR_ROW * k);

    // The row of the hero: what it wears and holds, each in a place of its own, and the hero.
    let row_left = inner.left();
    for (slot, item, shown) in &worn_pics {
        let (left, w, hh) = slot_box(*slot);
        let place = Rect::from_min_size(Pos2::new(row_left + left * k, top), vec2(w as f32 * gear_cell, hh as f32 * gear_cell));
        let response = ui.interact(place, egui::Id::new(("bag_slot", *slot as u8)), Sense::click());
        let lit = response.hovered() && held.is_some();
        ui.painter().rect_filled(place, 0.0, c(WOOD_DARK));
        ui.painter().rect_stroke(place, 0.0, Stroke::new(1.0, c(if lit { BRASS_LIGHT } else { BRASS_DARK })), egui::StrokeKind::Inside);
        if let Some((id, size)) = shown {
            let tint = if held_slot == Some(*slot) { Color32::WHITE.gamma_multiply(0.35) } else { Color32::WHITE };
            ui.painter().image(*id, fitted(place.shrink(2.0), *size), WHOLE, tint);
        }
        if response.hovered() {
            under = Some(Target::Slot(*slot));
        }
        if held.is_none() {
            let mut lines = vec![(slot_label(*slot).text(lang).to_string(), FADED)];
            match item {
                Some(item) => lines.extend(item_lines(item, lang)),
                None => lines.push((t(T::BagEmptySlot).to_string(), FADED)),
            }
            response.clone().on_hover_ui(|ui| lines_ui(ui, &lines));
        }
        if response.clicked() {
            hits.push((Target::Slot(*slot), false));
        }
    }
    if let Some((id, size)) = doll {
        let place = Rect::from_min_size(Pos2::new(row_left + DOLL_LEFT * k, top), vec2(54.0 * k, gear_row));
        ui.painter().rect_filled(place, 0.0, Color32::from_rgba_unmultiplied(0x33, 0x26, 0x1a, 0x40));
        ui.painter().image(id, fitted(place, size), WHOLE, Color32::WHITE);
    }
    // What found no room in the bag: a button each, to take it in hand.
    let loose: Vec<(usize, String, Vec<(String, Rgba)>)> = bag(world, h).iter().enumerate().filter(|(_, i)| i.at.is_none()).map(|(n, i)| (n, item_name(i, ru), item_lines(i, lang))).collect();
    if !loose.is_empty() {
        let place = Rect::from_min_max(Pos2::new(row_left + LOOSE_LEFT * k, top), Pos2::new(inner.right(), top + gear_row));
        let mut list = ui.new_child(UiBuilder::new().id_salt("bag_loose").max_rect(place).layout(Layout::top_down(Align::Min)));
        warning(&mut list, t(T::BagNoRoom));
        for (i, name, lines) in loose.iter().take(3) {
            let short: String = name.chars().take(12).collect();
            let response = list.add(button(short)).on_hover_ui(|ui| lines_ui(ui, lines));
            if response.clicked() {
                hits.push((Target::Loose(*i), false));
            }
        }
    }

    // The bag's grid, and the things in it each at its own size.
    let grid = Rect::from_min_size(Pos2::new(inner.left(), top + gear_row + 6.0), vec2(BAG_W as f32 * GRID_CELL, BAG_H as f32 * GRID_CELL));
    let response = ui.interact(grid, egui::Id::new("bag_grid"), Sense::click());
    let painter = ui.painter().clone();
    painter.rect_filled(grid.expand(2.0), 0.0, c(WOOD_DEEP));
    let cell_rect =
        |x: u8, y: u8, w: u8, hh: u8| Rect::from_min_size(Pos2::new(grid.left() + x as f32 * GRID_CELL, grid.top() + y as f32 * GRID_CELL), vec2(w as f32 * GRID_CELL, hh as f32 * GRID_CELL));
    for y in 0..BAG_H {
        for x in 0..BAG_W {
            let shade = if (x + y) % 2 == 0 { WOOD_DARK } else { [0x3c, 0x24, 0x17, 0xff] };
            painter.rect_filled(cell_rect(x, y, 1, 1).shrink(0.5), 0.0, c(shade));
        }
    }
    for (i, (x, y), (w, hh), (id, _)) in &placed {
        let place = cell_rect(*x, *y, *w, *hh);
        painter.rect_filled(place, 0.0, Color32::from_rgba_unmultiplied(0x40, 0x52, 0x8c, 0x73));
        painter.rect_stroke(place, 0.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(0x73, 0x8c, 0xd9, 0xb3)), egui::StrokeKind::Inside);
        let tint = if held_index == Some(*i) { Color32::WHITE.gamma_multiply(0.35) } else { Color32::WHITE };
        painter.image(*id, place, WHOLE, tint);
    }
    let cell_under =
        response.hover_pos().map(|p| (((p.x - grid.left()) / GRID_CELL).floor().clamp(0.0, BAG_W as f32 - 1.0) as u8, ((p.y - grid.top()) / GRID_CELL).floor().clamp(0.0, BAG_H as f32 - 1.0) as u8));
    if let Some(cell) = cell_under {
        state.over = Some(cell);
        under = Some(Target::Cell(cell.0, cell.1));
        match &held {
            // Where the held thing would go: green to lie, yellow to swap, red where it cannot.
            Some(held) => {
                let size = held_size(world, h, held);
                let at = corner_for(cell, size);
                let overlapped = world.story.bags.get(h).map_or(0, |b| b.overlapped(size, at, held_index).len());
                let tone = match overlapped {
                    0 => Color32::from_rgba_unmultiplied(0x4c, 0xe6, 0x59, 0x59),
                    1 => Color32::from_rgba_unmultiplied(0xf2, 0xd9, 0x4c, 0x59),
                    _ => Color32::from_rgba_unmultiplied(0xf2, 0x40, 0x33, 0x66),
                };
                let place = cell_rect(at.0, at.1, size.0, size.1);
                painter.rect_filled(place, 0.0, tone);
                painter.rect_stroke(place, 0.0, Stroke::new(1.0, tone.gamma_multiply(2.0)), egui::StrokeKind::Inside);
            }
            None => {
                if let Some(item) = world.story.bags.get(h).and_then(|b| b.item_at(cell)).and_then(|i| bag(world, h).get(i)) {
                    let lines = item_lines(item, lang);
                    response.clone().on_hover_ui_at_pointer(|ui| lines_ui(ui, &lines));
                }
            }
        }
    }
    if response.clicked()
        && let Some((x, y)) = cell_under
    {
        hits.push((Target::Cell(x, y), false));
    }

    // Those who travel with the party, beside the bag; a shop takes their place.
    if !shop && !with.is_empty() {
        kit::plate(ui.painter(), side, kit::Look::Parchment, true);
        let mut col = ui.new_child(UiBuilder::new().id_salt("bag_companions").max_rect(side.shrink(PAD)).layout(Layout::top_down(Align::Min)));
        kit::title(&mut col, t(T::Companions), crate::ui::theme::SMALL_TITLE);
        hint(&mut col, t(T::CompanionsAbout));
        for (n, name, weapon, armor, pack, used) in &with {
            col.add_space(4.0);
            col.label(RichText::new(name.chars().take(18).collect::<String>()).color(c(WAX_DARK)));
            col.horizontal(|row| {
                for (hand, shown, about) in [(true, weapon, T::CompanionHand), (false, armor, T::CompanionBack)] {
                    let (place, response) = row.allocate_exact_size(vec2(GRID_CELL, GRID_CELL), Sense::click());
                    row.painter().rect_filled(place, 0.0, c(WOOD_DARK));
                    row.painter().rect_stroke(place, 0.0, Stroke::new(1.0, c(if response.hovered() { BRASS_LIGHT } else { BRASS_DARK })), egui::StrokeKind::Inside);
                    if let Some((id, size)) = shown {
                        row.painter().image(*id, fitted(place.shrink(2.0), *size), WHOLE, Color32::WHITE);
                    }
                    if response.on_hover_text(t(about)).clicked() {
                        hits.push((Target::Ally(*n, hand), false));
                    }
                }
            });
            // The pack: what it holds, and how much of its cells is taken.
            col.label(RichText::new(format!("{} {used}/{}", t(T::PackWord), world::model::PACK_CELLS)).color(c(FADED)));
            let across = 5;
            let small = 28.0;
            let rows = pack.len().div_ceil(across).max(1);
            let (place, response) = col.allocate_exact_size(vec2(across as f32 * (small + 1.0) + 2.0, rows as f32 * (small + 1.0) + 2.0), Sense::click());
            col.painter().rect_filled(place, 0.0, c(WOOD_DARK));
            if response.clicked() {
                hits.push((Target::AllyPack(*n), false));
            }
            for (k, ((id, size), count, lines)) in pack.iter().enumerate() {
                let cell = Rect::from_min_size(place.min + vec2(1.0 + (k % across) as f32 * (small + 1.0), 1.0 + (k / across) as f32 * (small + 1.0)), vec2(small, small));
                let response = col.interact(cell, egui::Id::new(("ally_item", *n, k)), Sense::click());
                col.painter().rect_filled(cell, 0.0, c(WOOD_DEEP));
                col.painter().image(*id, fitted(cell, *size), WHOLE, Color32::WHITE);
                if *count > 1 {
                    col.painter().text(cell.right_bottom(), egui::Align2::RIGHT_BOTTOM, count.to_string(), egui::FontId::proportional(9.0), c(PARCHMENT));
                }
                if response.on_hover_ui(|ui| lines_ui(ui, lines)).clicked() {
                    hits.push((Target::AllyItem(*n, k), false));
                }
            }
        }
    }

    // The thing in hand follows the cursor, centred on it as in the old games.
    let pointer = ctx.pointer_latest_pos();
    if let (Some(((id, _), (w, hh))), Some(at)) = (held_pic, pointer) {
        let size = vec2(w as f32 * GRID_CELL, hh as f32 * GRID_CELL);
        ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("bag_held"))).image(id, Rect::from_center_size(at, size), WHOLE, Color32::WHITE);
    }
    // A right click lets go of what is held, or uses what is under the cursor; a click left
    // of the bag with a thing in hand throws it on the ground.
    if ctx.input(|i| i.pointer.secondary_clicked()) {
        hits.push((under.unwrap_or(Target::Nothing), true));
    }
    let side_taken = shop || !with.is_empty();
    let left_edge = if side_taken { side.left() } else { rect.left() };
    if held.is_some() && ctx.input(|i| i.pointer.primary_clicked()) && pointer.is_some_and(|p| p.x < left_edge) {
        clicks.outside = true;
    }
    clicks.hits.extend(hits);
    if close {
        *phase = Phase::Walking;
        sound.write(Play(Sfx::Back));
    }
}

/// The things kept at hand, in the strip at the bottom: a picture, how many, and the key.
#[allow(clippy::too_many_arguments)]
pub fn quick_bar(mut egui: EguiContexts, game: Res<Game>, session: Res<Session>, settings: Res<Settings>, lang: Res<Lang>, mut pictures: ResMut<Pictures>, mut images: ResMut<Assets<Image>>) {
    let world = &game.world;
    let me = player_name(&settings);
    let hero = crate::play::my_hero(world, &me, matches!(*session, Session::Solo)).unwrap_or(0);
    let slots: Vec<Option<(Item, (egui::TextureId, Vec2))>> = (0..world::bag::QUICK_SLOTS)
        .map(|slot| world.quick_item(hero, slot).and_then(|i| bag(world, hero).get(i)).map(|item| (item.clone(), picture_id(&mut pictures, &mut egui, &mut images, item, world))))
        .collect();
    let Ok(ctx) = egui.ctx_mut() else { return };
    if !kit::ready(ctx) {
        return;
    }
    let screen = ctx.content_rect();
    let strip = crate::play::HUD_HEIGHT * UI_PIXEL;
    let side = strip - 4.0;
    egui::Area::new(egui::Id::new("quick_bar")).order(egui::Order::Background).fixed_pos(Pos2::new(screen.left() + 4.0, screen.bottom() - strip + 2.0)).show(ctx, |ui| {
        ui.horizontal(|row| {
            row.spacing_mut().item_spacing.x = 3.0;
            for (n, slot) in slots.iter().enumerate() {
                let (place, response) = row.allocate_exact_size(vec2(side, side), Sense::hover());
                row.painter().rect_filled(place, 0.0, c(WOOD_DEEP));
                row.painter().rect_stroke(place, 0.0, Stroke::new(1.0, c(if slot.is_some() { BRASS } else { WOOD_DARK })), egui::StrokeKind::Inside);
                row.painter().text(place.left_top() + vec2(2.0, 1.0), egui::Align2::LEFT_TOP, format!("F{}", n + 1), egui::FontId::proportional(8.0), c(WOOD_LIGHT));
                if let Some((item, (id, size))) = slot {
                    row.painter().image(*id, fitted(place.shrink(3.0), *size), WHOLE, Color32::WHITE);
                    if item.count > 1 {
                        row.painter().text(place.right_bottom(), egui::Align2::RIGHT_BOTTOM, item.count.to_string(), egui::FontId::proportional(9.0), c(PARCHMENT));
                    }
                    response.on_hover_ui(|ui| lines_ui(ui, &item_lines(item, *lang)));
                } else {
                    response.on_hover_text(T::BagQuickHint.text(*lang));
                }
            }
        });
    });
}
