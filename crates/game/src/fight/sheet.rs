//! The fight's sheets, drawn with egui: who acts when along the top of the map, and under it
//! the fight's log, whose turn it is and what the hero can do now. Every button that cannot
//! be pressed says why under the cursor. A press is sent as a `FightPress` to `on_activate`.

use super::*;
use crate::ui::egui::{self, Key, Pos2, Rect, RichText, Stroke, vec2};
use crate::ui::kit::{self, button, hint};
use crate::ui::theme::c;
use bevy_egui::EguiContexts;

/// A button of the fight was pressed, with what was typed in the field of one's own action.
#[derive(Event, Clone)]
pub struct FightPress {
    pub action: FightAction,
    pub typed: String,
}

/// What is typed in the field of one's own action.
#[derive(Resource, Default)]
pub struct StuntField(pub String);

/// The strip of keys at the bottom of the screen, the party's column and the top left column,
/// in units.
const HUD: f32 = HUD_HEIGHT * crate::screen::UI_PIXEL;
const PARTY: f32 = crate::view::COLUMN * crate::screen::UI_PIXEL;
const LEFT_COLUMN: f32 = 300.0;
const CELL: f32 = 50.0;
/// How tall the fight's sheet under the map is at least and at most, in units.
const FIGHT_LEAST: f32 = 64.0;
const FIGHT_MOST: f32 = 120.0;

#[allow(clippy::too_many_arguments)]
pub fn sheet(
    mut egui: EguiContexts,
    mut commands: Commands,
    (phase, game, view, lang, session, settings): (Res<Phase>, Res<Game>, Res<FightView>, Res<Lang>, Res<Session>, Res<Settings>),
    table: Res<crate::coop::Table>,
    mut slot: ResMut<crate::view::StoryPanel>,
    mut field: ResMut<StuntField>,
    mut was_writing: Local<bool>,
    mut needed: Local<f32>,
) {
    if *phase != Phase::Fighting {
        *was_writing = false;
        return;
    }
    let world = &game.world;
    let roster_timer = table.roster.turn_timer;
    let lang = *lang;
    let ru = lang == Lang::Ru;
    let t = |text: T| text.text(lang);
    let Ok(ctx) = egui.ctx_mut() else { return };
    let Some(mut ui) = kit::screen_ui(ctx, "fight_sheet") else { return };
    let screen = ui.max_rect();
    // As tall as what it holds needs, and never more than a strip: in a fight the map is
    // what the player looks at.
    let height = needed.clamp(FIGHT_LEAST, FIGHT_MOST);
    if slot.0 != height {
        slot.0 = height;
    }
    let map = Rect::from_min_max(screen.min, Pos2::new(screen.right() - PARTY, screen.bottom() - HUD - height));
    let rect = Rect::from_min_max(Pos2::new(screen.left(), map.bottom()), Pos2::new(map.right(), screen.bottom() - HUD));
    let fight = world.fight.as_ref();

    // Who acts when, along the top of the map right of the column of where and when.
    if let (None, Some(fight)) = (&view.over, fight) {
        let left = map.left() + LEFT_COLUMN.min(map.width() * 0.5) + 8.0;
        let room = Rect::from_min_max(Pos2::new(left, map.top() + 4.0), Pos2::new(map.right() - 4.0, map.top() + 4.0 + CELL * 2.0));
        let mut row = ui.new_child(egui::UiBuilder::new().id_salt("fight_strip").max_rect(room).layout(egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true)));
        row.spacing_mut().item_spacing = vec2(2.0, 2.0);
        let casual = world.table.difficulty == world::Difficulty::Casual;
        let order = (0..fight.order.len()).map(|step| &fight.combatants[fight.order[(fight.turn + step) % fight.order.len()]]).filter(|c| !c.dead);
        for (n, fighter) in order.take(10).enumerate() {
            let ours = fighter.side == Side::Party;
            let edge = if n == 0 { BRASS_LIGHT } else if ours { MOSS } else { WAX };
            egui::Frame::new().fill(egui::Color32::from_rgba_unmultiplied(0x10, 0x0a, 0x08, 0xd8)).stroke(Stroke::new(1.0, c(edge))).inner_margin(egui::Margin::same(1)).show(&mut row, |cell| {
                cell.set_width(CELL - 4.0);
                cell.vertical_centered(|cell| {
                    cell.spacing_mut().item_spacing.y = 1.0;
                    let short: String = fighter.name.chars().take(8).collect();
                    cell.label(RichText::new(short).color(c(if n == 0 { BRASS_LIGHT } else { PARCHMENT })));
                    if casual || ours {
                        cell.label(RichText::new(format!("{}/{}", fighter.hp.max(0), fighter.max_hp)).color(c(TALLOW)));
                    }
                    let share = (fighter.hp.max(0) as f32 / fighter.max_hp.max(1) as f32).clamp(0.0, 1.0);
                    let (r, _) = cell.allocate_exact_size(vec2(32.0, 3.0), egui::Sense::hover());
                    cell.painter().rect_filled(r, 0.0, c(INK));
                    cell.painter().rect_filled(Rect::from_min_size(r.min, vec2(32.0 * share, 3.0)), 0.0, c(if ours { [0x5c, 0xb0, 0x4a, 0xff] } else { WAX }));
                });
            });
        }
    }

    let mut pressed: Vec<FightAction> = Vec::new();
    let enter = !ctx.egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(Key::Enter));
    let log = |ui: &mut egui::Ui| {
        for line in &view.lines {
            ui.add(egui::Label::new(RichText::new(line).color(c(INK))).wrap());
        }
    };

    // The fight is over: what it came to, and what next.
    if let Some(summary) = &view.over {
        let casual = world.table.difficulty == world::Difficulty::Casual;
        let pressed = std::cell::RefCell::new(&mut pressed);
        let (_, wants) = kit::sheet_at(
            &mut ui,
            "fight_over",
            rect,
            t(if summary.won { T::Victory } else { T::Defeat }),
            |ui| {
                log(ui);
                if summary.won {
                    ui.label(format!("{} {}", t(T::XpEach), summary.xp_each));
                    for (name, up) in &summary.level_ups {
                        ui.label(RichText::new(format!("{name}: {} {}! +{} {}", t(T::Level), up.level, up.hp_gained, t(T::HitPoints))).color(c(MOSS)));
                    }
                } else if casual {
                    ui.label(t(T::DefeatCasual));
                } else {
                    ui.label(t(T::DefeatAsk));
                }
            },
            |footer| {
                if summary.won || casual || session.is_client() {
                    if kit::right(footer, |r| r.add(button(t(T::Onward)).primary().key("Enter"))).clicked() || enter {
                        pressed.borrow_mut().push(FightAction::Onward);
                    }
                } else {
                    // What becomes of a beaten party is the host's call; on the hardest
                    // setting there is no going back to an earlier save.
                    if world.table.difficulty != world::Difficulty::Hardcore && footer.add(button(t(T::LoadGame))).clicked() {
                        pressed.borrow_mut().push(FightAction::Load);
                    }
                    if kit::right(footer, |r| r.add(button(t(T::MasterDecides)).primary())).clicked() {
                        pressed.borrow_mut().push(FightAction::MasterDecides);
                    }
                }
            },
        );
        *needed = wants;
    } else if let Some(fight) = fight {
        let actor = fight.actor();
        let playing = !view.queue.is_empty();
        let can_act = actor.side == Side::Party && !playing && my_turn(world, &session, &settings);
        let turn = format!("{} {} · {} {}/{} · {} {}", t(T::FightTurn), actor.name, t(T::HitPoints), actor.hp.max(0), actor.max_hp, t(T::Movement), actor.movement_left);
        let writing = can_act && !view.asking && view.mode == Mode::Writing;
        let entered = writing && !*was_writing;
        *was_writing = writing;
        let spent = t(T::ActionSpent);
        // At a shared table, this player's own fighter: in others' turns a blow can be
        // prepared for when its turn comes.
        let me = crate::coop::player_name(&settings);
        let own = crate::play::my_hero(world, &me, false).and_then(|i| fight.combatants.iter().find(|c| c.id == format!("hero_{i}") && !c.dead));
        let can_prepare = !matches!(*session, Session::Solo) && !can_act && !playing && !view.asking && own.is_some();
        let prepared = view.prepared;
        let prepared_name = prepared.and_then(|w| actor.strikes.get(w)).map(|s| tr(ru, &s.name).to_string());
        let pressed = std::cell::RefCell::new(&mut pressed);
        let push = |action: FightAction| pressed.borrow_mut().push(action);
        let (_, wants) = kit::sheet_at(
            &mut ui,
            "fight",
            rect,
            "",
            |ui| {
                log(ui);
                ui.horizontal(|row| {
                    row.label(RichText::new(turn).color(c(WAX_DARK)));
                    // The time a player has for the turn, at a shared table that sets one.
                    let timer = if session.is_host() { settings.turn_timer } else { roster_timer };
                    if timer > 0 && actor.side == Side::Party && !playing {
                        let left = (timer as f32 - view.turn_age).max(0.0).ceil() as u32;
                        row.label(RichText::new(format!("· {} {left} {}", t(T::TurnLeft), t(T::Seconds))).color(c(if left <= 10 { WAX } else { WAX_DARK })));
                    }
                });
                if view.asking {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(9.0).color(c(BRASS_DARK)));
                        ui.label(t(T::TestRunning));
                    });
                } else if playing {
                    hint(ui, t(T::FightPlaying));
                } else if !can_act {
                    hint(ui, t(T::OthersTurn));
                    if can_prepare && let Some(own) = own {
                        ui.horizontal_wrapped(|row| {
                            hint(row, t(T::PrepareHint));
                            for (n, strike) in own.strikes.iter().enumerate().filter(|(_, s)| s.uses_left != Some(0)) {
                                let mut b = button(tr(ru, &strike.name));
                                if prepared == Some(n) {
                                    b = b.primary();
                                }
                                if row.add(b).clicked() {
                                    push(FightAction::Prepare(if prepared == Some(n) { None } else { Some(n) }));
                                }
                            }
                        });
                    }
                } else {
                    match &view.mode {
                        // The keys are written on the buttons themselves.
                        Mode::Menu => {}
                        Mode::Spells => {
                            hint(ui, t(T::SpellsAbout));
                            ui.horizontal_wrapped(|ui| {
                                for (index, slot) in castable(world, fight) {
                                    let name = srd().spells.get(&index).map_or(index.as_str(), |s| tr(ru, &s.name)).to_string();
                                    let label = if slot == 0 { name } else { format!("{name} ({slot})") };
                                    if ui.add(button(label)).clicked() {
                                        push(FightAction::Spell(index, slot));
                                    }
                                }
                                for (index, slot) in rituals(world, fight) {
                                    let name = srd().spells.get(&index).map_or(index.as_str(), |s| tr(ru, &s.name)).to_string();
                                    let label = if slot == 0 { format!("{name} *") } else { format!("{name} ({slot}) *") };
                                    if ui.add(button(label)).clicked() {
                                        push(FightAction::Ritual(index, slot));
                                    }
                                }
                            });
                        }
                        Mode::Items => {
                            hint(ui, t(T::ItemsAbout));
                            ui.horizontal_wrapped(|ui| {
                                for (i, name, heal, count) in usable(world, fight) {
                                    let many = if count > 1 { format!(" ×{count}") } else { String::new() };
                                    if ui.add(button(format!("{name}{many} (+{heal})"))).clicked() {
                                        push(FightAction::Item(i));
                                    }
                                }
                            });
                        }
                        Mode::Moving => {
                            hint(ui, t(T::MoveHint));
                        }
                        Mode::Aiming(aim) => {
                            let none = strike_of(world, fight, aim).is_none_or(|s| targets(fight, &s).is_empty());
                            hint(ui, t(if none { T::NoTargets } else { T::AimHint }));
                        }
                        Mode::Writing => {
                            ui.label(t(T::StuntHint));
                            let edit = ui.add(egui::TextEdit::singleline(&mut field.0).desired_width(f32::INFINITY).char_limit(300).hint_text(t(T::SayHint)));
                            if entered {
                                edit.request_focus();
                            }
                            if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                push(FightAction::SendStunt);
                            }
                        }
                    }
                }
            },
            |footer| {
                if !can_act || view.asking {
                    return;
                }
                match &view.mode {
                    Mode::Menu => {
                        // The blow prepared in another's turn, one press away.
                        if let (Some(name), true) = (&prepared_name, actor.action_left) {
                            if footer.add(button(format!("{} {name}", t(T::DoPrepared))).primary().key("Enter")).clicked() || footer.input(|i| i.key_pressed(Key::Enter)) {
                                push(FightAction::Prepared);
                            }
                        }
                        if footer.add(button(t(T::Move)).key("1").enabled(actor.movement_left > 0, t(T::NoStepsLeft))).clicked() {
                            push(FightAction::Move);
                        }
                        for (n, strike) in actor.strikes.iter().enumerate() {
                            // A feat that is used up is off the menu until the next long rest.
                            if strike.uses_left == Some(0) {
                                continue;
                            }
                            let name = tr(ru, &strike.name).to_string();
                            let label = match strike.uses_left {
                                Some(left) => format!("{name} ({left})"),
                                None => name,
                            };
                            let key = ["2", "3"].get(n).copied().unwrap_or("");
                            if footer.add(button(label).key(key).enabled(actor.action_left, spent)).clicked() {
                                push(FightAction::Weapon(n));
                            }
                        }
                        if !(castable(world, fight).is_empty() && rituals(world, fight).is_empty()) && footer.add(button(t(T::Spells)).enabled(actor.action_left, spent)).clicked() {
                            push(FightAction::Spells);
                        }
                        if !usable(world, fight).is_empty() && footer.add(button(t(T::FightItems)).key("4").enabled(actor.action_left, spent)).clicked() {
                            push(FightAction::Items);
                        }
                        if footer.add(button(t(T::OwnAction)).enabled(actor.action_left, spent)).on_hover_text(t(T::StuntHint)).clicked() {
                            push(FightAction::Stunt);
                        }
                        if footer.add(button(t(T::Dodge)).key("5").enabled(actor.action_left, spent)).on_hover_text(t(T::DodgeWhy)).clicked() {
                            push(FightAction::Dodge);
                        }
                        if footer.add(button(t(T::Dash)).enabled(actor.action_left, spent)).on_hover_text(t(T::DashWhy)).clicked() {
                            push(FightAction::Dash);
                        }
                        if footer.add(button(t(T::Disengage)).enabled(actor.action_left, spent)).on_hover_text(t(T::DisengageWhy)).clicked() {
                            push(FightAction::Disengage);
                        }
                        if footer.add(button(t(T::EndTurn)).primary().key("6")).clicked() {
                            push(FightAction::EndTurn);
                        }
                    }
                    Mode::Writing => {
                        if footer.add(button(t(T::Cancel)).key("Esc")).clicked() {
                            push(FightAction::Cancel);
                        }
                        if footer.add(button(t(T::Say)).primary().key("Enter")).clicked() {
                            push(FightAction::SendStunt);
                        }
                    }
                    _ => {
                        if footer.add(button(t(T::Cancel)).key("Esc")).clicked() {
                            push(FightAction::Cancel);
                        }
                    }
                }
            },
        );
        *needed = wants;
    }
    for action in pressed {
        let typed = if action == FightAction::SendStunt { std::mem::take(&mut field.0) } else { String::new() };
        commands.trigger(FightPress { action, typed });
    }
}
