//! The party down the right edge: a face, a name, a level, hit points and armour class for
//! every hero at the table, the player's own one framed in brass, the one whose turn it is
//! in a fight lit. Always there while playing, so nobody has to open a sheet to see who is
//! hurt. Drawn with egui; the map is never under it.

use crate::coop::{Session, player_name};
use crate::i18n::{Lang, T};
use crate::menu::Screen;
use crate::play::{Game, my_hero};
use crate::store::Settings;
use crate::theme::*;
use crate::ui::egui::{self, Align, Color32, Layout, Pos2, Rect, RichText, Stroke, vec2};
use crate::ui::kit;
use crate::ui::theme::c;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, EguiTextureHandle};
use std::collections::HashMap;

pub struct PartyHudPlugin;

impl Plugin for PartyHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(EguiPrimaryContextPass, column.in_set(crate::ui::UiSet::Screens).run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>));
    }
}

/// A hero's face as egui knows it: the texture of the first frame of its figure, by what shows on it.
#[derive(Default)]
struct Faces(HashMap<String, (egui::TextureId, Vec2)>);

/// The width of the column, in units of the interface.
const WIDTH: f32 = crate::view::COLUMN * crate::screen::UI_PIXEL;
/// Where on a figure the head and shoulders are, in its pixels.
const FACE: (f32, f32, f32, f32) = (4.0, 7.0, 28.0, 31.0);

#[allow(clippy::too_many_arguments)]
fn column(mut egui: EguiContexts, game: Res<Game>, session: Res<Session>, settings: Res<Settings>, lang: Res<Lang>, mut images: ResMut<Assets<Image>>, mut faces: Local<Faces>) {
    let world = &game.world;
    let me = player_name(&settings);
    let mine = my_hero(world, &me, matches!(*session, Session::Solo));
    // In a fight the wounds are the fight's until it ends.
    let hp_of = |i: usize, hero: &rules::Character| world.fight.as_ref().and_then(|f| f.combatants.iter().find(|c| c.id == format!("hero_{i}"))).map_or(hero.hp, |c| c.hp);
    let acting = world.fight.as_ref().map(|f| f.actor().id.clone());
    // Each face is made once and handed to egui.
    let mut wanted = Vec::new();
    for hero in &world.party {
        let key = crate::doll::key(hero);
        if !faces.0.contains_key(&key) {
            let frame = crate::doll::frames(hero).swap_remove(0);
            let size = Vec2::new(frame.w as f32, frame.h as f32);
            wanted.push((key, images.add(frame.into_image()), size));
        }
    }
    for (key, handle, size) in wanted {
        let id = egui.add_image(EguiTextureHandle::Strong(handle));
        faces.0.insert(key, (id, size));
    }
    let Ok(ctx) = egui.ctx_mut() else { return };
    if !kit::ready(ctx) {
        return;
    }
    let t = |text: T| text.text(*lang);
    let screen = ctx.content_rect();
    let strip = crate::play::HUD_HEIGHT * crate::screen::UI_PIXEL;
    let rect = Rect::from_min_max(Pos2::new(screen.right() - WIDTH, screen.top()), Pos2::new(screen.right(), screen.bottom() - strip));
    egui::Area::new(egui::Id::new("party")).order(egui::Order::Background).fixed_pos(rect.min).interactable(false).show(ctx, |ui| {
        ui.painter().rect_filled(rect, 0.0, c(WOOD_DEEP));
        ui.painter().line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(1.0, c(BRASS)));
        ui.set_min_size(rect.size());
        ui.set_max_width(WIDTH);
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.add_space(4.0);
            ui.spacing_mut().item_spacing.y = 1.0;
            for (i, hero) in world.party.iter().enumerate() {
                let away = world.is_away(i);
                let own = Some(i) == mine;
                let turn = acting.as_deref() == Some(format!("hero_{i}").as_str());
                let hp = hp_of(i, hero);
                let edge = if turn { BRASS_LIGHT } else if own { BRASS } else { WOOD_DARK };
                let frame = egui::Frame::new().stroke(Stroke::new(1.0, c(edge))).inner_margin(egui::Margin::same(2)).outer_margin(egui::Margin::symmetric(3, 2));
                frame.show(ui, |ui| {
                    ui.set_width(WIDTH - 14.0);
                    ui.with_layout(Layout::top_down(Align::Center), |ui| {
                        if let Some((texture, size)) = faces.0.get(&crate::doll::key(hero)) {
                            let uv = Rect::from_min_max(Pos2::new(FACE.0 / size.x, FACE.1 / size.y), Pos2::new(FACE.2 / size.x, FACE.3 / size.y));
                            let dim = if hero.dead || away { 0.45 } else { 1.0 };
                            let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), egui::Sense::hover());
                            ui.painter().rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(0x40, 0x4c, 0x33, 0x99));
                            ui.painter().image(*texture, r, uv, Color32::WHITE.gamma_multiply(dim));
                        }
                        let name: String = hero.name.chars().take(10).collect();
                        ui.label(RichText::new(format!("{name} {}", hero.level)).color(c(if own { BRASS_LIGHT } else { PARCHMENT })));
                        // Hit points as a bar, yellow and then red as they run low.
                        let share = (hp.max(0) as f32 / hero.max_hp.max(1) as f32).clamp(0.0, 1.0);
                        let bar = if share > 0.5 { [0x5c, 0xb0, 0x4a, 0xff] } else if share > 0.25 { [0xe0, 0xb0, 0x3a, 0xff] } else { WAX };
                        let (r, _) = ui.allocate_exact_size(vec2(64.0, 4.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, 0.0, c(INK));
                        ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(64.0 * share, 4.0)), 0.0, c(bar));
                        let state = if hero.dead {
                            t(T::HudFallen).to_string()
                        } else if away {
                            t(T::HudAway).to_string()
                        } else {
                            format!("{}/{} · {} {}", hp.max(0), hero.max_hp, t(T::ArmorClass), hero.ac())
                        };
                        ui.label(RichText::new(state).color(c(TALLOW)));
                        if world.tired() && !hero.dead {
                            ui.label(RichText::new(t(T::Tired)).color(c(WAX)));
                        }
                    });
                });
            }
            // Those who travel with the party, smaller: a name and a bar.
            for (n, companion) in world.companions.iter().enumerate() {
                let fighter = world.fight.as_ref().and_then(|f| f.combatants.iter().find(|c| c.id == format!("ally_{n}")));
                let hp = fighter.map_or(companion.hp, |c| c.hp);
                let turn = acting.as_deref() == Some(format!("ally_{n}").as_str());
                let frame = egui::Frame::new().stroke(Stroke::new(1.0, c(if turn { BRASS_LIGHT } else { WOOD_DARK }))).inner_margin(egui::Margin::same(2)).outer_margin(egui::Margin::symmetric(3, 2));
                frame.show(ui, |ui| {
                    ui.set_width(WIDTH - 14.0);
                    ui.with_layout(Layout::top_down(Align::Center), |ui| {
                        let name: String = companion.creature.name.chars().take(12).collect();
                        ui.label(RichText::new(name).color(c(PARCHMENT)));
                        let share = (hp.max(0) as f32 / companion.max_hp.max(1) as f32).clamp(0.0, 1.0);
                        let (r, _) = ui.allocate_exact_size(vec2(64.0, 3.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, 0.0, c(INK));
                        ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(64.0 * share, 3.0)), 0.0, c([0x5c, 0xb0, 0x4a, 0xff]));
                        ui.label(RichText::new(format!("{}/{}", hp.max(0), companion.max_hp)).color(c(TALLOW)));
                    });
                });
            }
        });
    });
}
