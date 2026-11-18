//! What stays on the map while playing, drawn with egui, each in a place of its own so that
//! nothing lies over anything else: at the top left where the party is and when, the goal
//! and the tasks, and table talk under them; at the top right what has just happened, each
//! line kept for as long as it takes to read; at the bottom the keys that do something now.

use super::*;
use crate::coop::ChatLine;
use crate::theme::{BRASS_LIGHT, FADED, PARCHMENT, PARCHMENT_SHADE, Rgba};
use crate::ui::egui::{self, Align2, Color32, Pos2, RichText, vec2};
use crate::ui::kit;
use crate::ui::theme::c;
use bevy_egui::EguiContexts;

/// Something that has just happened, in the corner for as long as it takes to read.
pub struct Notice {
    text: String,
    tint: Rgba,
    age: f32,
    life: f32,
}

/// How long a line stays: three seconds and one more for every fifteen letters, between four
/// and twenty. The cursor over it keeps it.
pub fn life_of(text: &str) -> f32 {
    (3.0 + text.chars().count() as f32 / 15.0).clamp(4.0, 20.0)
}

/// The lines in the corner, oldest first.
#[derive(Resource, Default)]
pub struct Notices(pub Vec<Notice>);

impl Notices {
    pub fn push(&mut self, text: String, tint: Rgba) {
        let life = life_of(&text);
        self.0.push(Notice { text, tint, age: 0.0, life });
    }
}

/// At most this many lines in the corner at once; the rest wait their turn.
const SHOWN_AT_ONCE: usize = 3;
/// How wide the columns over the map are, in units.
const COLUMN_WIDTH: f32 = 300.0;
const NOTICE_WIDTH: f32 = 260.0;

const GAIN: Rgba = [0x8c, 0xe6, 0x80, 0xff];
const LOSS: Rgba = [0xf2, 0x80, 0x66, 0xff];

#[allow(clippy::too_many_arguments)]
pub fn hud(
    mut egui: EguiContexts,
    (game, phase, lang, session, settings, time): (Res<Game>, Res<Phase>, Res<Lang>, Res<Session>, Res<Settings>, Res<Time>),
    (mut table, mut toast, mut arrival, mut shown_place, mut notices): (ResMut<Table>, ResMut<Toast>, ResMut<Arrival>, ResMut<ShownPlace>, ResMut<Notices>),
    (story, mut translator, shopping): (Res<crate::view::StoryPanel>, ResMut<Translator>, Res<crate::shop_ui::Shopping>),
    mut seen_bag: Local<Option<std::collections::BTreeMap<String, i64>>>,
    mut seen_goal: Local<Option<(String, Vec<(String, bool)>)>>,
) {
    let dt = time.delta_secs();
    let lang = *lang;
    let t = |text: T| text.text(lang);
    let world = &game.world;
    let here = world.here();
    let other_tongue = foreign(world, lang);
    let translator = translator.bypass_change_detection();
    let mut read = |text: &str| translator.read(other_tongue, text);
    let me = player_name(&settings);
    let solo = matches!(*session, Session::Solo);

    // The place's name is kept for whoever else shows it.
    let place = read(&here.name);
    if shown_place.0 != place {
        shown_place.0 = place.clone();
    }
    toast.left = (toast.left - dt).max(0.0);
    arrival.age += dt;

    // What the hero's bag gained and lost since the last look, a line each.
    let hero = my_hero(world, &me, solo).unwrap_or(0);
    let bag: std::collections::BTreeMap<String, i64> = world.story.bags.get(hero).map(|b| {
        let mut counts = std::collections::BTreeMap::new();
        for item in &b.inventory {
            *counts.entry(item.name.clone()).or_insert(0) += item.count as i64;
        }
        counts
    }).unwrap_or_default();
    if let Some(before) = seen_bag.as_ref() {
        let names: std::collections::BTreeSet<&String> = before.keys().chain(bag.keys()).collect();
        for name in names {
            let delta = bag.get(name).copied().unwrap_or(0) - before.get(name).copied().unwrap_or(0);
            if delta != 0 {
                let count = if delta.abs() > 1 { format!(" ×{}", delta.abs()) } else { String::new() };
                let (sign, tint) = if delta > 0 { ("+", GAIN) } else { ("−", LOSS) };
                notices.push(format!("{sign} {name}{count}"), tint);
            }
        }
    }
    *seen_bag = Some(bag);

    // The goal and the tasks, and what changed in them.
    let quest = &world.quest;
    let goal = match quest.done {
        _ if !quest.is_set() => String::new(),
        Some(true) => format!("{}: {}", t(T::QuestWon), quest.title),
        Some(false) => format!("{}: {}", t(T::QuestLost), quest.title),
        None => format!("{} {}", t(T::Goal), quest.goal),
    };
    let tasks: Vec<(String, bool)> = quest.tasks.iter().map(|task| (task.name.clone(), task.done)).collect();
    if let Some((before, tasks_before)) = seen_goal.as_ref() {
        if *before != goal && !goal.is_empty() {
            notices.push(if quest.done.is_some() { goal.clone() } else { format!("{} {}", t(T::NewGoal), quest.goal) }, BRASS_LIGHT);
        }
        for (name, done) in &tasks {
            match tasks_before.iter().find(|(n, _)| n == name) {
                None => notices.push(format!("{} {name}", t(T::NewTask)), BRASS_LIGHT),
                Some((_, false)) if *done => notices.push(format!("{} {name}", t(T::TaskDone)), BRASS_LIGHT),
                _ => {}
            }
        }
    }
    *seen_goal = Some((goal.clone(), tasks.clone()));

    // What others have set about, and whatever else was said to the corner.
    for said in std::mem::take(&mut table.notices) {
        let said = said.replace(world::prompt::GO_ON, "").trim().to_string();
        notices.push(format!("» {said}"), BRASS_LIGHT);
    }

    // Table talk is aged here: it fades from the column after a while.
    {
        let table = table.bypass_change_detection();
        for line in &mut table.chat {
            line.age += dt;
        }
    }

    let Ok(ctx) = egui.ctx_mut() else { return };
    if !kit::ready(ctx) {
        return;
    }
    let screen = crate::ui::kit::stage(ctx);
    let party = crate::view::COLUMN * crate::screen::UI_PIXEL;
    let strip = HUD_HEIGHT * crate::screen::UI_PIXEL;
    let map = egui::Rect::from_min_max(screen.min, Pos2::new(screen.right() - party, screen.bottom() - strip - story.0));
    let backing = Color32::from_rgba_unmultiplied(0x10, 0x0a, 0x08, 0xd8);
    let boxed = |ui: &mut egui::Ui, text: RichText| {
        egui::Frame::new().fill(backing).inner_margin(egui::Margin::symmetric(4, 2)).show(ui, |ui| {
            ui.add(egui::Label::new(text).wrap());
        });
    };

    // Top left: where and when, the goal and the tasks, and what the table says.
    // A shop takes the left of the screen: where and when wait until it closes.
    let shop_open = *phase == Phase::Bag && shopping.0.is_some();
    if !shop_open {
        egui::Area::new(egui::Id::new("hud_left")).order(egui::Order::Background).fixed_pos(map.min + vec2(4.0, 4.0)).interactable(false).show(ctx, |ui| {
            ui.set_max_width(COLUMN_WIDTH.min(map.width() * 0.5));
            ui.spacing_mut().item_spacing.y = 2.0;
            let (day, hour, minute) = world.time();
            let light = match world.daylight() {
                "day" => "",
                "dusk" => " ~",
                _ => " *",
            };
            boxed(ui, RichText::new(format!("{place} · {} {day} · {hour:02}:{minute:02}{light}", t(T::DayWord))).color(c(PARCHMENT)));
            if !goal.is_empty() {
                boxed(ui, RichText::new(read(&goal)).color(c(BRASS_LIGHT)));
                for (name, _) in tasks.iter().filter(|(_, done)| !done).take(3) {
                    boxed(ui, RichText::new(format!("· {}", read(name))).color(c(PARCHMENT_SHADE)));
                }
            }
            let fresh: Vec<&ChatLine> = table.chat.iter().filter(|l| l.age < CHAT_SECONDS).collect();
            for line in &fresh[fresh.len().saturating_sub(CHAT_LINES)..] {
                boxed(ui, RichText::new(format!("{}: {}", line.from, line.text)).color(c(PARCHMENT)));
            }
        });
    }

    // Top right: what has just happened, the oldest first, each for as long as it takes to read.
    let mut hovered = vec![false; notices.0.len()];
    // With the bag open the lines stand left of it, and of a shop beside it.
    let right = match *phase {
        Phase::Bag if shopping.0.is_some() => crate::bag_ui::draw::panels(screen, true).1.left() - 6.0,
        Phase::Bag => crate::bag_ui::draw::panels(screen, false).0.left() - 6.0,
        _ => map.right(),
    };
    egui::Area::new(egui::Id::new("hud_notices")).order(egui::Order::Foreground).pivot(Align2::RIGHT_TOP).fixed_pos(egui::pos2(right - 4.0, map.top() + 4.0)).show(ctx, |ui| {
        ui.set_max_width(NOTICE_WIDTH.min(map.width() * 0.45));
        ui.spacing_mut().item_spacing.y = 2.0;
        for (n, notice) in notices.0.iter().take(SHOWN_AT_ONCE).enumerate() {
            let fade = ((notice.life - notice.age) / 0.6).clamp(0.0, 1.0);
            let tint = c(notice.tint).gamma_multiply(fade);
            let r = egui::Frame::new().fill(backing.gamma_multiply(fade)).inner_margin(egui::Margin::symmetric(4, 2)).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(&notice.text).color(tint)).wrap());
            });
            hovered[n] = r.response.contains_pointer();
        }
    });
    for (notice, held) in notices.0.iter_mut().take(SHOWN_AT_ONCE).zip(&hovered) {
        if !held {
            notice.age += dt;
        }
    }
    notices.0.retain(|n| n.age < n.life);

    // The strip at the bottom: what pressing a key does now, or what just happened to the hero.
    let thing_name = world
        .thing_near(THING_REACH)
        .and_then(|id| here.props.iter().map(|p| (&p.id, &p.name)).chain(here.creatures.iter().map(|c| (&c.id, &c.name))).find(|(i, _)| i.as_str() == id).map(|(_, name)| name.clone()))
        .or_else(|| world.tile_near().and_then(|cell| world.ground_actions(cell)).map(|(name, _)| name.to_string()));
    let lying = world.lying_near().and_then(|n| here.items.get(n)).map(|l| l.item.name.clone());
    let hint = match (toast.text.filter(|_| toast.left > 0.0), thing_name, world.exit_near(EXIT_HINT_REACH)) {
        (Some(said), _, _) => t(said).to_string(),
        (None, _, _) if lying.is_some() => format!("E: {} {}", t(T::PickUp), lying.unwrap_or_default()),
        (None, Some(name), _) => format!("E: {}", read(&name)),
        (None, None, Some(exit)) if exit.label.is_empty() => format!("E: → {}", read(world.place_name(&exit.to))),
        (None, None, Some(exit)) => format!("E: {} → {}", read(&exit.label), read(world.place_name(&exit.to))),
        (None, None, None) if my_offer(world, &me, solo).is_some() => t(T::FeatOffer).to_string(),
        (None, None, None) if seat_state(world, &session, &me) == SeatState::Mine => format!("{}{}", t(T::HudHint), t(T::HudMaster)),
        (None, None, None) => t(T::HudHint).to_string(),
    };
    // One line, never wrapped word by word into a column: what does not fit the strip beside
    // the keys at hand is cut with an ellipsis, and shown whole on hover.
    let room = (crate::ui::kit::stage(ctx).width() * 0.5).max(120.0);
    egui::Area::new(egui::Id::new("hud_keys")).order(egui::Order::Background).pivot(Align2::RIGHT_BOTTOM).fixed_pos(crate::ui::kit::stage(ctx).right_bottom() + vec2(-6.0, -(strip - 9.0) / 2.0)).show(ctx, |ui| {
        ui.set_max_width(room);
        ui.add(egui::Label::new(RichText::new(&hint).color(c(PARCHMENT_SHADE))).wrap_mode(egui::TextWrapMode::Truncate)).on_hover_text(hint.as_str());
    });
    let _ = FADED;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_stays_for_as_long_as_it_takes_to_read() {
        assert_eq!(life_of("+ Rope"), 4.0);
        let long = "x".repeat(150);
        assert_eq!(life_of(&long), 13.0);
        assert_eq!(life_of(&"x".repeat(1000)), 20.0);
    }
}
