//! The sheets of the play screen, drawn with egui: the story under the map, a thing looked
//! at, the field for one's own words, the master's seat, and the sheets in the middle of the
//! screen (pause, journal, the heroes). They show what `sheet_for` says and nothing of their
//! own; a press is sent as a `PlayPress` to the same handlers as before.

use super::*;
use crate::ui::egui::{self, Key, Pos2, Rect, Sense, Ui, vec2};
use crate::ui::kit::{self, Sheet as Paper, button, hint, warning};
use crate::ui::theme::c;
use bevy_egui::EguiContexts;

/// A button of the play screen was pressed, with what was typed in the field it belongs to.
#[derive(Event, Clone)]
pub struct PlayPress {
    pub action: PlayAction,
    pub typed: String,
}

/// What is typed in the play screen's fields, and how far the telling has been written out.
#[derive(Resource, Default)]
pub struct Fields {
    say: String,
    judge: String,
    redo: String,
    panel: String,
    /// The draft whose words the judge's field was filled from.
    judged: String,
    /// The telling being written out, and how many of its letters are shown.
    telling: String,
    letters: f32,
}

/// The strip of keys at the bottom of the screen and the column of the party, in units.
const HUD: f32 = HUD_HEIGHT * crate::screen::UI_PIXEL;
const PARTY: f32 = crate::view::COLUMN * crate::screen::UI_PIXEL;
/// How tall the story's panel is, at each of its three sizes.
pub const STORY_HEIGHTS: [f32; 3] = [96.0, 168.0, 260.0];
/// The least a sheet under the map is, whatever it holds.
const STORY_LEAST: f32 = 64.0;

#[allow(clippy::too_many_arguments)]
pub fn overlay(
    mut egui: EguiContexts,
    mut commands: Commands,
    (phase, game, gm, lang, session, table, panel): (Res<Phase>, Res<Game>, Res<Gm>, Res<Lang>, Res<Session>, Res<Table>, Res<Panel>),
    (mut settings, store, mut folded, use_for, link): (ResMut<Settings>, Res<Store>, ResMut<Folded>, Res<UseFor>, Res<crate::coop::SteamLink>),
    (time, mut slot, arrival): (Res<Time>, ResMut<crate::view::StoryPanel>, Res<Arrival>),
    mut translator: ResMut<Translator>,
    mut fields: ResMut<Fields>,
    mut was: Local<Option<std::mem::Discriminant<Phase>>>,
) {
    let me = player_name(&settings);
    let solo = matches!(*session, Session::Solo);
    let lang = *lang;
    let mut extra = Extra {
        seat: seat_state(&game.world, &session, &me),
        judging: table.judging.clone(),
        panel: PanelView::default(),
        lost: hero_lost(&game.world, &me, solo),
        offer: my_offer(&game.world, &me, solo).map(|offer| offer.feats.iter().map(|f| crate::hero::feat_line(f, lang)).collect()).unwrap_or_default(),
        advancing: my_hero(&game.world, &me, solo).and_then(|i| game.world.party.get(i)).and_then(|hero| rules::srd().class(hero.advancing())).map(|class| tr(lang == Lang::Ru, &class.name).to_string()).unwrap_or_default(),
        folded: folded.0,
        going_on: table.going_on.clone(),
        use_for: use_for.0.clone().unwrap_or_default(),
    };
    if *phase == Phase::Master {
        let others = others_at_table(&game.world, &me);
        extra.panel = PanelView {
            typed: panel.typed.clone(),
            target: others.get(panel.target % others.len().max(1)).cloned().unwrap_or_default(),
            hostile: panel.hostile,
            note: table.master_note.clone(),
            cards: table.cards.iter().map(|card| card.name.clone()).collect(),
            presses: panel.presses,
        };
    }
    let sheet = translated(sheet_for(&phase, &game.world, gm.dispatcher.now(), lang, &extra), foreign(&game.world, lang), translator.bypass_change_detection());
    // A field opened afresh takes the keyboard.
    let entered = was.replace(std::mem::discriminant(&*phase)) != Some(std::mem::discriminant(&*phase));
    let Ok(ctx) = egui.ctx_mut() else { return };
    let Some(mut ui) = kit::screen_ui(ctx, "play_sheets") else { return };
    let t = |text: T| text.text(lang);
    let keys_label = if settings.active_provider().is_some() { T::KeysProvider } else { T::Keys };
    // Pressed in the body or in the footer of a sheet: both may add to it.
    let pressed: std::cell::RefCell<Vec<PlayAction>> = std::cell::RefCell::new(Vec::new());
    let mut size_pressed = false;
    let mut invite = false;
    let low = matches!(sheet, Sheet::Arrival(..) | Sheet::Looking { .. } | Sheet::Writing { .. } | Sheet::Asking(_) | Sheet::Reading(_) | Sheet::Folded | Sheet::Last(_) | Sheet::Rolling(_) | Sheet::Judging(_) | Sheet::Master(_));
    // Just walked in: the place's name, and its description written out, where the story is.
    let here = game.world.here();
    let description = translator.bypass_change_detection().read(foreign(&game.world, lang), &here.description);
    let letters = description.chars().count();
    let arriving = matches!(*phase, Phase::Walking | Phase::Arriving { .. }) && matches!(sheet, Sheet::None | Sheet::Last(_)) && arrival.age < letters as f32 / LETTERS_PER_SECOND + ARRIVAL_LINGER;
    let sheet = if arriving {
        let name = translator.bypass_change_detection().read(foreign(&game.world, lang), &here.name);
        Sheet::Arrival(name, description.chars().take((arrival.age * LETTERS_PER_SECOND) as usize).collect())
    } else {
        sheet
    };
    let screen = ui.max_rect();
    let panel_height = match &sheet {
        Sheet::Arrival(..) => 64.0,
        Sheet::None => 0.0,
        Sheet::Folded => 28.0,
        Sheet::Last(_) => 36.0,
        // The telling is as tall as it will be when all of it is written out, so the map does
        // not shrink with every line as the letters come: what is still to come is measured
        // beside what the sheet holds now.
        Sheet::Reading(turn) => {
            let width = screen.width() - PARTY - 24.0;
            let tall = |text: String| ui.fonts_mut(|f| f.layout(text, egui::FontId::proportional(crate::ui::theme::BODY), egui::Color32::WHITE, width)).size().y;
            let shown: String = if fields.telling == turn.narration { turn.narration.chars().take(fields.letters as usize).collect() } else { String::new() };
            let to_come = (tall(turn.narration.clone()) - tall(shown)).max(0.0);
            STORY_HEIGHTS[settings.narration.min(2) as usize].min(kit::last_needed(ctx).map_or(f32::MAX, |n| n + to_come).max(STORY_LEAST))
        }
        // The rest are no taller than what they hold: a thing with two actions leaves the map large.
        _ if low => STORY_HEIGHTS[settings.narration.min(2) as usize].min(kit::last_needed(ctx).unwrap_or(f32::MAX).max(STORY_LEAST)),
        _ => 0.0,
    };
    // The map is seen above the panel: the camera is told how much of the screen it takes.
    // In a fight the fight's sheet says how tall it is.
    if slot.0 != panel_height && *phase != Phase::Fighting {
        slot.0 = panel_height;
    }
    let story_rect = Rect::from_min_max(Pos2::new(screen.left(), screen.bottom() - HUD - panel_height), Pos2::new(screen.right() - PARTY, screen.bottom() - HUD));
    let no_typing = !ctx.egui_wants_keyboard_input();
    let digit = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6].iter().position(|k| ui.input(|i| i.key_pressed(*k)));
    match &sheet {
        Sheet::None => {}
        Sheet::Arrival(name, shown) => {
            kit::plate(ui.painter(), story_rect, kit::Look::Parchment, true);
            let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(story_rect.shrink(8.0)));
            kit::title(&mut inner, name.as_str(), crate::ui::theme::SMALL_TITLE);
            inner.add(egui::Label::new(egui::RichText::new(shown).color(c(crate::theme::INK))).wrap());
        }
        Sheet::Last(text) => {
            kit::plate(ui.painter(), story_rect, kit::Look::Parchment, true);
            let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(story_rect.shrink(8.0)));
            inner.add(egui::Label::new(egui::RichText::new(text).color(c(crate::theme::FADED))).truncate());
        }
        Sheet::Folded => {
            kit::sheet_at(&mut ui, "story_folded", story_rect, "", |ui| hint(ui, t(T::FoldedHint)), |footer| {
                if footer.add(button(t(T::Unfold)).key("H")).clicked() {
                    folded.0 = false;
                }
            });
            if no_typing && ui.input(|i| i.key_pressed(Key::Enter)) {
                folded.0 = false;
            }
        }
        Sheet::Reading(turn) => {
            if fields.telling != turn.narration {
                // A telling that went on from the one being read keeps what was written out.
                if !turn.narration.starts_with(fields.telling.as_str()) || fields.telling.is_empty() {
                    fields.letters = 0.0;
                }
                fields.telling = turn.narration.clone();
            }
            fields.letters += crate::play::LETTERS_PER_SECOND * time.delta_secs();
            let shown: String = turn.narration.chars().take(fields.letters as usize).collect();
            let mut all_shown = false;
            kit::sheet_at(
                &mut ui,
                "story",
                story_rect,
                "",
                |ui| {
                    // Clicked, it shows the rest at once; its ink is set, or egui would give it a button's.
                    if ui.add(egui::Label::new(egui::RichText::new(shown).color(c(crate::theme::INK))).wrap().sense(Sense::click())).clicked() {
                        all_shown = true;
                    }
                    for (n, option) in turn.options.iter().enumerate() {
                        // A choice offered to another player's hero is shown with that hero's
                        // name, and cannot be taken here.
                        let mine = crate::play::mine_to_take(option, &game.world, &me, solo);
                        let whose = option.for_hero.as_deref().map(|h| format!(" — {h}")).unwrap_or_default();
                        let far = option.near.as_deref().and_then(|id| far_from(&game.world, id));
                        let (label, action) = match far {
                            Some(name) => (format!("{}. {} ({} {name})", n + 1, option.label, t(T::WalkUpTo)), PlayAction::Approach(n)),
                            None => {
                                let chance = game.world.chance(option).map(|c| format!("  [~{c}%]")).unwrap_or_default();
                                (format!("{}. {}{chance}", n + 1, option.label), PlayAction::Choose(n))
                            }
                        };
                        let why = format!("{}{whose}", t(T::OfferForOther));
                        if ui.add(button(format!("{label}{whose}")).wide().enabled(mine, &why)).clicked() {
                            pressed.borrow_mut().push(action);
                        }
                    }
                },
                |footer| {
                    if !turn.options.is_empty() && footer.add(button(t(T::OwnAction))).clicked() {
                        pressed.borrow_mut().push(PlayAction::Own);
                    }
                    if footer.add(button(t(T::SheetSize))).clicked() {
                        size_pressed = true;
                    }
                    if footer.add(button(t(T::Fold)).key("H")).clicked() {
                        folded.0 = true;
                    }
                    hint(footer, t(T::StoryKeys));
                    if kit::right(footer, |r| r.add(button(t(T::Onward)).primary().key("Enter"))).clicked() {
                        pressed.borrow_mut().push(PlayAction::Close);
                    }
                },
            );
            if all_shown {
                fields.letters = f32::MAX;
            }
            if no_typing {
                if let Some(n) = digit.filter(|n| *n < turn.options.len() && crate::play::mine_to_take(&turn.options[*n], &game.world, &me, solo)) {
                    let far = turn.options[n].near.as_deref().and_then(|id| far_from(&game.world, id)).is_some();
                    pressed.borrow_mut().push(if far { PlayAction::Approach(n) } else { PlayAction::Choose(n) });
                } else if ui.input(|i| i.key_pressed(Key::Enter)) && ui.memory(|m| m.focused().is_none()) {
                    // Enter shows the rest of the telling first, then goes on.
                    if (fields.letters as usize) < turn.narration.chars().count() {
                        fields.letters = f32::MAX;
                    } else {
                        pressed.borrow_mut().push(PlayAction::Close);
                    }
                }
            }
        }
        Sheet::Asking(said) => {
            let quoted = if table.going_on.is_some() { said.clone() } else { format!("«{said}»") };
            kit::sheet_at(&mut ui, "asking", story_rect, "", |ui| {
                ui.label(quoted);
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(9.0).color(c(crate::theme::BRASS_DARK)));
                    ui.label(t(T::TestRunning));
                });
                hint(ui, t(T::AskingAbout));
            }, |_| {});
        }
        Sheet::Writing { table: at_table, about } => {
            kit::sheet_at(
                &mut ui,
                "writing",
                story_rect,
                "",
                |ui| {
                    if about.is_empty() {
                        ui.label(t(T::WhatDoYouDo));
                    } else {
                        ui.label(format!("{about} — {}", t(T::UseForWhat)));
                    }
                    let field = ui.add(egui::TextEdit::singleline(&mut fields.say).desired_width(f32::INFINITY).char_limit(300).hint_text(t(T::SayHint)));
                    if entered {
                        field.request_focus();
                    }
                    if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        pressed.borrow_mut().push(PlayAction::Send);
                    }
                    hint(ui, if *at_table { t(T::WritingHintTable) } else { t(T::WritingHint) });
                },
                |footer| {
                    if footer.add(button(t(T::Leave)).key("Esc")).clicked() {
                        pressed.borrow_mut().push(PlayAction::Close);
                    }
                    if *at_table && footer.add(button(t(T::TableTalk))).clicked() {
                        pressed.borrow_mut().push(PlayAction::Talk);
                    }
                    if kit::right(footer, |r| r.add(button(t(T::Say)).primary().key("Enter"))).clicked() {
                        pressed.borrow_mut().push(PlayAction::Send);
                    }
                },
            );
        }
        Sheet::Looking { name, description, actions, chest, shop } => {
            kit::sheet_at(
                &mut ui,
                "looking",
                story_rect,
                name,
                |ui| {
                    ui.add(egui::Label::new(description.as_str()).wrap());
                    if let Some(locked) = chest {
                        ui.horizontal_wrapped(|ui| {
                            if ui.add(button(t(T::ChestOpen))).clicked() {
                                pressed.borrow_mut().push(PlayAction::Chest(world::bag::Opening::Open));
                            }
                            if *locked {
                                if ui.add(button(t(T::ChestPick))).clicked() {
                                    pressed.borrow_mut().push(PlayAction::Chest(world::bag::Opening::Pick));
                                }
                                if ui.add(button(t(T::ChestForce))).clicked() {
                                    pressed.borrow_mut().push(PlayAction::Chest(world::bag::Opening::Force));
                                }
                            }
                        });
                    }
                    for (n, label) in actions.iter().enumerate() {
                        if ui.add(button(format!("{}. {label}", n + 1)).wide()).clicked() {
                            pressed.borrow_mut().push(PlayAction::Do(n));
                        }
                    }
                },
                |footer| {
                    if footer.add(button(t(T::Leave)).key("Esc")).clicked() {
                        pressed.borrow_mut().push(PlayAction::Close);
                    }
                    if *shop && footer.add(button(t(T::Trade))).clicked() {
                        pressed.borrow_mut().push(PlayAction::Trade);
                    }
                    if footer.add(button(t(T::OwnAction))).clicked() {
                        pressed.borrow_mut().push(PlayAction::Own);
                    }
                    hint(footer, t(T::LookKeys));
                },
            );
            if no_typing && let Some(n) = digit.filter(|n| *n < actions.len()) {
                pressed.borrow_mut().push(PlayAction::Do(n));
            }
        }
        Sheet::Rolling(what) => {
            kit::sheet_at(&mut ui, "rolling", story_rect, t(T::CheckTitle), |ui| ui.label(what.as_str()), |footer| {
                if kit::right(footer, |r| r.add(button(t(T::RollDie)).primary().key("Enter"))).clicked() {
                    pressed.borrow_mut().push(PlayAction::Roll);
                }
            });
            if no_typing && ui.input(|i| i.key_pressed(Key::Enter)) && ui.memory(|m| m.focused().is_none()) {
                pressed.borrow_mut().push(PlayAction::Roll);
            }
        }
        Sheet::Judging(view) => {
            if fields.judged != view.narration {
                fields.judged = view.narration.clone();
                fields.judge = view.narration.clone();
            }
            kit::sheet_at(
                &mut ui,
                "judging",
                story_rect,
                t(T::DraftTitle),
                |ui| {
                    hint(ui, t(T::JudgeAbout));
                    if !view.said.is_empty() {
                        hint(ui, format!("«{}»", view.said));
                    }
                    ui.add(egui::TextEdit::multiline(&mut fields.judge).desired_rows(4).desired_width(f32::INFINITY).char_limit(2000));
                    for effect in &view.effects {
                        hint(ui, format!("• {effect}"));
                    }
                    for option in &view.options {
                        ui.label(format!("→ {option}"));
                    }
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut fields.redo).desired_width(220.0).char_limit(300).hint_text(t(T::RedoHint)));
                        if ui.add(button(t(T::Redo))).clicked() {
                            pressed.borrow_mut().push(PlayAction::Redo);
                        }
                    });
                },
                |footer| {
                    if footer.add(button(t(T::Reject))).clicked() {
                        pressed.borrow_mut().push(PlayAction::Judge(false));
                    }
                    if kit::right(footer, |r| r.add(button(t(T::Accept)).primary())).clicked() {
                        pressed.borrow_mut().push(PlayAction::Judge(true));
                    }
                },
            );
        }
        Sheet::Master(view) => {
            kit::sheet_at(
                &mut ui,
                "master",
                story_rect,
                t(T::MasterTitle),
                |ui| {
                    let field = ui.add(egui::TextEdit::singleline(&mut fields.panel).desired_width(f32::INFINITY).char_limit(400).hint_text(t(T::MasterFieldHint)));
                    if entered {
                        field.request_focus();
                    }
                    let tool = |ui: &mut Ui, label: String, tool: Tool, pressed: &mut Vec<PlayAction>| {
                        if ui.add(button(label)).clicked() {
                            pressed.push(PlayAction::Tool(tool));
                        }
                    };
                    ui.horizontal_wrapped(|ui| {
                        tool(ui, t(T::ToModel).into(), Tool::ToModel, &mut pressed.borrow_mut());
                        tool(ui, t(T::Narrate).into(), Tool::Narrate, &mut pressed.borrow_mut());
                        let target = if view.target.is_empty() { "—" } else { view.target.as_str() };
                        tool(ui, format!("{} {target}", t(T::WhisperTo)), Tool::Whisper, &mut pressed.borrow_mut());
                        tool(ui, t(T::NextPlayer).into(), Tool::NextTarget, &mut pressed.borrow_mut());
                    });
                    ui.horizontal_wrapped(|ui| {
                        tool(ui, t(T::ToolCreature).into(), Tool::Creature, &mut pressed.borrow_mut());
                        tool(ui, t(if view.hostile { T::MoodHostile } else { T::MoodNeutral }).into(), Tool::Mood, &mut pressed.borrow_mut());
                        for (label, which) in [
                            (T::ToolRemove, Tool::Remove),
                            (T::ToolTile, Tool::Tile),
                            (T::ToolDamage, Tool::Damage),
                            (T::ToolHeal, Tool::Heal),
                            (T::ToolGive, Tool::Give),
                            (T::ToolFight, Tool::Fight),
                            (T::ToolRoll, Tool::Roll),
                            (T::ToolGold, Tool::Gold),
                            (T::ToolHours, Tool::Hours),
                            (T::ToolSky, Tool::Sky),
                            (T::ToolSay, Tool::Say),
                            (T::ToolBrush, Tool::Brush),
                        ] {
                            tool(ui, t(label).into(), which, &mut pressed.borrow_mut());
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        tool(ui, t(T::ToolInvent).into(), Tool::Invent, &mut pressed.borrow_mut());
                        for (n, card) in view.cards.iter().enumerate() {
                            tool(ui, card.clone(), Tool::Card(n), &mut pressed.borrow_mut());
                        }
                    });
                    if !view.note.is_empty() {
                        ui.label(egui::RichText::new(&view.note).color(c(crate::theme::WAX_DARK)));
                    }
                    hint(ui, t(T::MasterHint));
                },
                |footer| {
                    if footer.add(button(t(T::Leave)).key("Esc")).clicked() {
                        pressed.borrow_mut().push(PlayAction::Close);
                    }
                    if footer.add(button(t(T::SeatLeave))).clicked() {
                        pressed.borrow_mut().push(PlayAction::SeatLeave);
                    }
                },
            );
        }
        Sheet::Drawing(place) => {
            let paper = Paper { id: "drawing", title: t(T::Drawing), about: t(T::DrawingHint), max: vec2(300.0, 120.0), top: 120.0, fixed: false };
            kit::bare_sheet(&mut ui, paper, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(9.0).color(c(crate::theme::BRASS_DARK)));
                    kit::title(ui, place.as_str(), crate::ui::theme::SMALL_TITLE);
                });
            });
        }
        Sheet::Stuck(why) => {
            let paper = Paper { id: "stuck", title: t(T::StuckTitle), about: t(T::StuckAbout), max: vec2(320.0, 200.0), top: 100.0, fixed: false };
            kit::sheet(&mut ui, paper, |ui| warning(ui, why.as_str()), |footer| {
                for (label, action) in [(T::StayHere, PlayAction::Stay), (keys_label, PlayAction::Keys), (T::Sketch, PlayAction::Sketch), (T::Recheck, PlayAction::Retry)] {
                    if footer.add(button(t(label))).clicked() {
                        pressed.borrow_mut().push(action);
                    }
                }
            });
        }
        Sheet::Refused(why) => {
            let paper = Paper { id: "refused", title: t(T::RefusedTitle), about: t(T::RefusedAbout), max: vec2(320.0, 200.0), top: 100.0, fixed: false };
            kit::sheet(&mut ui, paper, |ui| warning(ui, why.as_str()), |footer| {
                for (label, action) in [(T::Leave, PlayAction::Close), (keys_label, PlayAction::Keys), (T::Recheck, PlayAction::AskAgain)] {
                    let mut b = button(t(label));
                    if action == PlayAction::AskAgain {
                        b = b.primary();
                    }
                    if footer.add(b).clicked() {
                        pressed.borrow_mut().push(action);
                    }
                }
            });
        }
        Sheet::Paused { seat, lost } => {
            let paper = Paper { id: "paused", title: t(T::Paused), about: t(T::PausedAbout), max: vec2(220.0, 360.0), top: 40.0, fixed: false };
            kit::bare_sheet(&mut ui, paper, |ui| {
                let mut first = None;
                let mut item = |ui: &mut Ui, label: T, enabled: bool, why: &str, action: Option<PlayAction>| {
                    let r = ui.add(button(t(label)).menu().enabled(enabled, why));
                    first.get_or_insert(r.clone());
                    if r.clicked() {
                        match action {
                            Some(action) => pressed.borrow_mut().push(action),
                            None => invite = true,
                        }
                    }
                };
                item(ui, T::Continue, true, "", Some(PlayAction::Resume));
                if *lost {
                    item(ui, T::NewHero, true, "", Some(PlayAction::NewHero));
                }
                if game.world.table.difficulty != world::Difficulty::Hardcore {
                    item(ui, T::SaveGame, true, "", Some(PlayAction::Save));
                }
                let no_party = game.world.party.is_empty();
                item(ui, T::ShortRest, !no_party, "", Some(PlayAction::ShortRest));
                item(ui, T::LongRest, !no_party, "", Some(PlayAction::Rest));
                match seat {
                    SeatState::Alone => {}
                    SeatState::Mine => item(ui, T::SeatLeave, true, "", Some(PlayAction::SeatLeave)),
                    SeatState::Free | SeatState::Taken => item(ui, T::SeatTake, *seat == SeatState::Free, t(T::SeatTaken), Some(PlayAction::SeatTake)),
                }
                item(ui, keys_label, true, "", Some(PlayAction::Keys));
                if crate::coop::steam_table(&session) {
                    item(ui, T::SteamInvite, true, "", None);
                }
                if !session.is_client() {
                    item(ui, T::RedrawPlace, game.world.fight.is_none(), t(T::RedrawInFight), Some(PlayAction::RedrawPlace));
                }
                item(ui, T::Report, true, "", Some(PlayAction::Report));
                item(ui, T::ToMainMenu, true, "", Some(PlayAction::MainMenu));
                if let Some(first) = first
                    && ui.memory(|m| m.focused().is_none())
                {
                    first.request_focus();
                }
            });
        }
        Sheet::Journal { quests, carried, lines } => {
            let paper = Paper { id: "journal", title: t(T::Journal), about: t(T::JournalAbout), max: vec2(520.0, 420.0), top: 20.0, fixed: true };
            kit::sheet(
                &mut ui,
                paper,
                |ui| {
                    for (n, line) in quests.iter().enumerate() {
                        let tone = if n == 0 { crate::theme::WAX_DARK } else { crate::theme::INK };
                        ui.add(egui::Label::new(egui::RichText::new(line).color(c(tone))).wrap());
                    }
                    let bag = if carried.is_empty() { t(T::CarryNothing).to_string() } else { format!("{} {carried}", t(T::Carried)) };
                    ui.add(egui::Label::new(egui::RichText::new(bag).color(c(crate::theme::WAX_DARK))).wrap());
                    ui.separator();
                    if lines.is_empty() {
                        hint(ui, t(T::JournalEmpty));
                    }
                    for (speaker, text) in lines {
                        let (prefix, tone) = if *speaker == Speaker::Player { ("» ", crate::theme::WAX_DARK) } else { ("", crate::theme::INK) };
                        ui.add(egui::Label::new(egui::RichText::new(format!("{prefix}{text}")).color(c(tone))).wrap());
                    }
                },
                |footer| {
                    if footer.add(button(t(T::Leave)).key("Esc")).clicked() {
                        pressed.borrow_mut().push(PlayAction::Close);
                    }
                },
            );
        }
        Sheet::Character { lines, points, offer, advancing } => {
            let paper = Paper { id: "character", title: t(T::CharacterTitle), about: t(T::CharacterAbout), max: vec2(520.0, 420.0), top: 20.0, fixed: true };
            kit::sheet(
                &mut ui,
                paper,
                |ui| {
                    if !offer.is_empty() {
                        ui.label(egui::RichText::new(t(T::FeatChoose)).color(c(crate::theme::WAX_DARK)));
                        for (n, line) in offer.iter().enumerate() {
                            if ui.add(button(line.as_str()).wide()).clicked() {
                                pressed.borrow_mut().push(PlayAction::Feat(n));
                            }
                        }
                        ui.separator();
                    }
                    if *points {
                        let ru = lang == Lang::Ru;
                        ui.horizontal_wrapped(|ui| {
                            for ability in Ability::ALL {
                                if ui.add(button(format!("+1 {}", tr(ru, ability.name())))).clicked() {
                                    pressed.borrow_mut().push(PlayAction::Improve(ability));
                                }
                            }
                        });
                        ui.separator();
                    }
                    for line in lines {
                        if let Some(heading) = line.strip_prefix("# ") {
                            ui.add_space(3.0);
                            ui.label(egui::RichText::new(heading).color(c(crate::theme::WAX_DARK)));
                        } else if let Some(note) = line.strip_prefix("~ ") {
                            hint(ui, note);
                        } else {
                            ui.add(egui::Label::new(line.as_str()).wrap());
                        }
                    }
                },
                |footer| {
                    if footer.add(button(t(T::Leave)).key("Esc")).clicked() {
                        pressed.borrow_mut().push(PlayAction::Close);
                    }
                    if !advancing.is_empty() && footer.add(button(format!("{} {advancing}", t(T::NextLevels)))).on_hover_text(t(T::NextLevelsHint)).clicked() {
                        pressed.borrow_mut().push(PlayAction::Advance);
                    }
                },
            );
        }
    }
    if size_pressed {
        settings.narration = (settings.narration + 1) % STORY_HEIGHTS.len() as u8;
        if let Err(e) = store.save_settings(&settings) {
            error!("could not save settings: {e}");
        }
    }
    if invite {
        crate::coop::invite_friends(&link, &session);
    }
    for action in pressed.into_inner() {
        let typed = match action {
            PlayAction::Send | PlayAction::Talk => std::mem::take(&mut fields.say),
            PlayAction::Judge(_) => fields.judge.clone(),
            PlayAction::Redo => std::mem::take(&mut fields.redo),
            PlayAction::Tool(_) => std::mem::take(&mut fields.panel),
            _ => String::new(),
        };
        commands.trigger(PlayPress { action, typed });
    }
}
