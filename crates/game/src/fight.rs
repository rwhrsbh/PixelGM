//! A fight on screen: whose turn it is, what the hero can do, and the monsters' turns played
//! back one event at a time. The rules themselves are in the `rules` crate; nothing here
//! decides an outcome.

use crate::audio::Sfx;
use crate::fx::{Fx, cell_centre};
use crate::coop::{Down, Intent, Session, my_turn};
use crate::gm::Gm;
use crate::i18n::{Lang, T};
use crate::master::{Ask, Master};
use crate::menu::Screen;
use crate::play::{Game, HUD_HEIGHT, Phase, Redraw, depth, on_screen};
use crate::store::{Settings, Store};
use crate::theme::*;
use bevy::prelude::*;
use bevy::sprite::Anchor;

mod sheet;
use bevy::window::PrimaryWindow;
use rules::combat::{Cell, Strike};
use rules::{Encounter, Event, Outcome, Side, srd, tr};
use std::collections::VecDeque;
use world::party::spell_strike;
use world::{FightIntent, FightSummary, MAX_H as MAP_H, MAX_W as MAP_W, TILE, World};

/// What the hero is choosing a target for.
#[derive(Clone, PartialEq, Debug)]
enum Aim {
    Weapon(usize),
    Spell { index: String, slot: u8 },
}

#[derive(Clone, PartialEq, Debug, Default)]
enum Mode {
    #[default]
    Menu,
    Moving,
    Spells,
    /// Choosing a thing from the bag to drink or eat.
    Items,
    Aiming(Aim),
    /// Typing something the rules have no button for.
    Writing,
}

#[derive(Resource, Default)]
pub struct FightView {
    /// Events that happened and have not been shown yet.
    queue: VecDeque<Event>,
    until_next: f32,
    /// The latest lines of the fight's log.
    lines: Vec<String>,
    mode: Mode,
    cursor: Cell,
    /// Index into the list of possible targets while aiming.
    target: usize,
    over: Option<FightSummary>,
    /// The master is judging something the acting hero tried.
    asking: bool,
}

#[derive(Component)]
struct Marker;

#[derive(Clone, PartialEq, Debug)]
pub enum FightAction {
    Move,
    Weapon(usize),
    Spells,
    Spell(String, u8),
    /// Open the list of things in the bag that can be used in a fight.
    Items,
    Item(usize),
    Dodge,
    Dash,
    Disengage,
    EndTurn,
    Cancel,
    Onward,
    MasterDecides,
    /// Open the field for something of the hero's own.
    Stunt,
    SendStunt,
    /// A spell the rules cannot carry out: the master judges it.
    Ritual(String, u8),
    /// Back to an earlier save after a defeat.
    Load,
}

const EVENT_SECONDS: f32 = 0.5;
const LOG_LINES: usize = 3;

fn hero_index(id: &str) -> Option<usize> {
    id.strip_prefix("hero_")?.parse().ok()
}

fn name_of<'a>(fight: &'a Encounter, id: &'a str) -> &'a str {
    fight.combatants.iter().find(|c| c.id == id).map_or(id, |c| c.name.as_str())
}

/// One event as a line of the log.
fn describe(event: &Event, fight: &Encounter, lang: Lang) -> Option<String> {
    let t = |key: T| key.text(lang);
    let name = |id: &str| name_of(fight, id).to_string();
    Some(match event {
        Event::TurnBegan { .. } | Event::Moved { .. } => return None,
        Event::Attacked { who, target, strike, roll, hit, critical, damage } => {
            let strike = tr(lang == Lang::Ru, strike);
            let outcome = match (hit, critical) {
                (true, true) => format!("{}, {} {damage}", t(T::FightCrit), t(T::FightDamage)),
                (true, false) => format!("{}, {} {damage}", t(T::FightHit), t(T::FightDamage)),
                _ => t(T::FightMiss).to_string(),
            };
            format!("{} → {} ({strike}): d20 {}{:+} = {} — {outcome}", name(who), name(target), roll.die, roll.bonus, roll.total())
        }
        Event::Saved { who, strike, roll, difficulty, success, damage } => {
            let result = if *success { t(T::FightSaved) } else { t(T::FightFailed) };
            format!("{} — {} ({}): {} {} {difficulty} — {result}, {} {damage}", name(who), t(T::FightSave), tr(lang == Lang::Ru, strike), roll.total(), t(T::Against), t(T::FightDamage))
        }
        Event::Struck { who, target, strike, damage } => {
            // An improvised action has no name in the book to translate.
            let label = if strike == T::StuntName.text(Lang::En) { t(T::StuntName) } else { tr(lang == Lang::Ru, strike) };
            format!("{} → {} ({label}): {} {damage}", name(who), name(target), t(T::FightDamage))
        }
        Event::Healed { who, target, amount } => format!("{} {} {}: +{amount}", name(who), t(T::FightHeals), name(target)),
        Event::Fell { who } => format!("{} {}", name(who), t(T::FightFalls)),
        Event::Died { who } => format!("{} {}", name(who), t(T::FightDies)),
        Event::DeathSave { who, die, successes, failures } => format!("{} — {}: d20 {die} ({successes}/{failures})", name(who), t(T::DeathSave)),
        Event::Revived { who } => format!("{} {}", name(who), t(T::FightRises)),
        Event::Dodged { who } => format!("{} — {}", name(who), t(T::Dodge)),
        Event::Dashed { who } => format!("{} — {}", name(who), t(T::Dash)),
        Event::Disengaged { who } => format!("{} — {}", name(who), t(T::Disengage)),
    })
}

/// What the acting hero carries that can be drunk or eaten in a fight: its place in the bag,
/// its name, what it heals and how many there are.
fn usable(world: &World, fight: &Encounter) -> Vec<(usize, String, String, u32)> {
    let healing = |items: &[world::Item]| items.iter().enumerate().filter(|(_, item)| !item.heal.trim().is_empty()).map(|(i, item)| (i, item.name.clone(), item.heal.clone(), item.count as u32)).collect();
    if let Some(ally) = fight.actor().id.strip_prefix("ally_").and_then(|n| n.parse::<usize>().ok()).and_then(|n| world.companions.get(n)) {
        return healing(&ally.pack);
    }
    let Some(bag) = hero_index(&fight.actor().id).and_then(|h| world.story.bags.get(h)) else { return Vec::new() };
    bag.inventory.iter().enumerate().filter(|(_, item)| !item.heal.trim().is_empty()).map(|(i, item)| (i, item.name.clone(), item.heal.clone(), item.count as u32)).collect()
}

/// The strike an aim stands for, built from the hero's sheet.
fn strike_of(world: &World, fight: &Encounter, aim: &Aim) -> Option<Strike> {
    match aim {
        Aim::Weapon(i) => fight.actor().strikes.get(*i).cloned(),
        Aim::Spell { index, slot } => spell_strike(world, hero_index(&fight.actor().id)?, index, *slot),
    }
}

/// Whom the actor could use `strike` on from where it stands: the other side for harm,
/// its own side for healing.
fn targets(fight: &Encounter, strike: &Strike) -> Vec<usize> {
    let actor = fight.actor();
    (0..fight.combatants.len())
        .filter(|i| {
            let c = &fight.combatants[*i];
            let right_side = if strike.heals { c.side == actor.side && !c.dead } else { c.side != actor.side && c.standing() };
            right_side && fight.in_range(strike, *i)
        })
        .collect()
}

/// The spells the acting hero can cast right now, with the slot each would use: the lowest
/// that is left. Only spells the rules can carry out are offered in a fight.
fn castable(world: &World, fight: &Encounter) -> Vec<(String, u8)> {
    let Some(hero) = hero_index(&fight.actor().id).and_then(|i| world.party.get(i)) else { return Vec::new() };
    hero.spells
        .iter()
        .filter_map(|index| {
            let spell = srd().spells.get(index).filter(|s| s.mechanised())?;
            let slot = if spell.level == 0 { 0 } else { (spell.level..=9).find(|l| hero.slots_left(*l) > 0)? };
            Some((index.clone(), slot))
        })
        .collect()
}

/// The acting hero's spells that the rules cannot carry out, with the slot each would
    /// use. The master judges them as they are cast.
fn rituals(world: &World, fight: &Encounter) -> Vec<(String, u8)> {
    let Some(hero) = hero_index(&fight.actor().id).and_then(|i| world.party.get(i)) else { return Vec::new() };
    hero.spells
        .iter()
        .filter_map(|index| {
            let spell = srd().spells.get(index).filter(|s| !s.mechanised())?;
            let slot = if spell.level == 0 { 0 } else { (spell.level..=9).find(|l| hero.slots_left(*l) > 0)? };
            Some((index.clone(), slot))
        })
        .collect()
}

/// Asks the master to judge something the acting hero tries. A guest asks the host.
#[allow(clippy::too_many_arguments)]
fn ask_stunt(said: String, spell: Option<(String, u8)>, world: &World, view: &mut FightView, session: &Session, gm: &Gm, store: &Store, settings: &Settings, master: &Master) {
    view.wait_for_master();
    if session.is_client() {
        session.intend(Intent::Stunt { said, spell });
    } else {
        // The guests see what the host's hero tries.
        if session.is_host() {
            let who = world.fight.as_ref().map(|f| f.actor().name.clone()).unwrap_or_default();
            session.broadcast(&Down::Thinking(format!("{who}: {said}")));
        }
        master.ask(gm, store, settings, Ask::Stunt { world: Box::new(world.clone()), said, spell });
    }
}

/// What the master is told when a hero casts a spell the rules cannot carry out.
pub fn ritual_words(world: &World, index: &str) -> Option<String> {
    let spell = srd().spells.get(index)?;
    let caster = world.fight.as_ref().map(|f| f.actor().name.clone()).unwrap_or_default();
    Some(format!("{caster} casts the spell {}: {}", spell.name, spell.description.chars().take(400).collect::<String>()))
}

/// The cell under the mouse, if it is over the map.
fn cell_under_cursor(window: &Window, view: &crate::view::View) -> Option<Cell> {
    let canvas = crate::view::map_under_cursor(window, view)?;
    let cell_px = TILE as f32 * crate::scene::PX as f32;
    let cell = ((canvas.x / cell_px).floor() as i32, (canvas.y / cell_px).floor() as i32);
    ((0..MAP_W as i32).contains(&cell.0) && (0..MAP_H as i32).contains(&cell.1)).then_some(cell)
}

impl FightView {
    /// Events that happened on the host, to be shown here.
    pub fn extend(&mut self, events: Vec<Event>) {
        self.queue.extend(events);
    }

    /// A line for the log that is not an event of the rules; it also ends the wait for
    /// the master.
    pub fn note(&mut self, line: String) {
        self.lines.push(line);
        let extra = self.lines.len().saturating_sub(LOG_LINES);
        self.lines.drain(..extra);
        self.asking = false;
    }

    /// The master has been asked about something the acting hero tried.
    pub fn wait_for_master(&mut self) {
        self.asking = true;
        self.mode = Mode::Menu;
    }

    /// The host says the fight is over.
    pub fn finish(&mut self, summary: FightSummary) {
        self.over = Some(summary);
        self.mode = Mode::Menu;
    }
}

/// Makes an event of the fight seen and heard as its line is added to the log.
fn stage(event: &Event, fight: &Encounter, fx: &mut Fx, lang: Lang) {
    let at = |id: &str| fight.combatants.iter().find(|c| c.id == id).map(|c| cell_centre(c.at()));
    let hero = |id: &str| id.starts_with("hero_");
    let wound = |fx: &mut Fx, id: &str, damage: i32| {
        if let Some(point) = at(id) {
            fx.flash(id, Color::srgb(1.0, 0.3, 0.25));
            fx.float(point + Vec2::Y * 22.0, format!("-{damage}"), Color::srgb(1.0, 0.4, 0.35));
            fx.burst(point, Color::srgb(0.7, 0.1, 0.1), 7);
        }
        if hero(id) {
            fx.wince();
            fx.shake(0.12);
        }
    };
    match event {
        Event::Attacked { who, target, hit, critical, damage, .. } => {
            if let (Some(from), Some(to)) = (at(who), at(target)) {
                fx.lunge(who, from, to);
            }
            if *hit {
                wound(fx, target, *damage);
                if *critical {
                    fx.shake(0.22);
                }
                fx.cues.push(if *critical { Sfx::Crit } else if hero(target) { Sfx::Hurt } else { Sfx::Hit });
            } else {
                if let Some(point) = at(target) {
                    fx.float(point + Vec2::Y * 22.0, T::FightMiss.text(lang), Color::srgb(0.85, 0.85, 0.8));
                }
                fx.cues.push(Sfx::Miss);
            }
        }
        Event::Saved { who, damage, .. } => {
            fx.cues.push(Sfx::Magic);
            if *damage > 0 {
                wound(fx, who, *damage);
            }
        }
        Event::Struck { who, target, damage, .. } => {
            if let (Some(from), Some(to)) = (at(who), at(target)) {
                fx.lunge(who, from, to);
            }
            wound(fx, target, *damage);
            fx.cues.push(Sfx::Hit);
        }
        Event::Healed { target, amount, .. } => {
            if let Some(point) = at(target) {
                fx.flash(target, Color::srgb(0.5, 1.0, 0.55));
                fx.float(point + Vec2::Y * 22.0, format!("+{amount}"), Color::srgb(0.5, 1.0, 0.55));
                fx.burst(point, Color::srgb(0.45, 0.9, 0.5), 8);
            }
            fx.cues.push(Sfx::Heal);
        }
        Event::Fell { who } | Event::Died { who } => {
            if let Some(point) = at(who) {
                fx.burst(point, Color::srgb(0.25, 0.08, 0.08), 12);
            }
            fx.cues.push(Sfx::Fall);
        }
        Event::Revived { who } => {
            if let Some(point) = at(who) {
                fx.burst(point, Color::srgb(1.0, 0.85, 0.4), 10);
            }
            fx.cues.push(Sfx::Heal);
        }
        Event::Moved { .. } => fx.cues.push(Sfx::Step),
        Event::DeathSave { .. } => fx.cues.push(Sfx::Dice),
        _ => {}
    }
}

/// Does what the acting hero's player asked for. A guest asks the host; the host and a solo
/// player do it themselves, and the host tells its guests what came of it.
fn act(intent: FightIntent, world: &mut World, view: &mut FightView, session: &Session, redraw: &mut Redraw) {
    view.mode = Mode::Menu;
    if session.is_client() {
        session.intend(Intent::Fight(intent));
        return;
    }
    let events = world.fight_act(&intent);
    if events.is_empty() {
        return;
    }
    session.broadcast(&Down::FightEvents(events.clone()));
    view.queue.extend(events);
    view.until_next = 0.0;
    redraw.0 = true;
}

/// Shows waiting events one at a time and, when there is nothing to show, moves the fight
/// on: a monster takes its turn, or the fight is found to be over. Only the host (or a solo
/// player) moves the fight on; guests are told.
fn drive(time: Res<Time>, phase: Res<Phase>, mut game: ResMut<Game>, mut view: ResMut<FightView>, mut redraw: ResMut<Redraw>, lang: Res<Lang>, session: Res<Session>, mut fx: ResMut<Fx>, mut was_over: Local<bool>) {
    // The end of a fight has a sound of its own, on every machine that watched it.
    if let (Some(summary), false) = (&view.over, *was_over) {
        fx.cues.push(if summary.won { Sfx::Victory } else { Sfx::Defeat });
    }
    *was_over = view.over.is_some();
    if *phase != Phase::Fighting {
        return;
    }
    let world = &mut game.world;
    if !view.queue.is_empty() {
        view.until_next -= time.delta_secs();
        if view.until_next > 0.0 {
            return;
        }
        let line = view.queue.pop_front().and_then(|event| {
            let fight = world.fight.as_ref()?;
            stage(&event, fight, &mut fx, *lang);
            describe(&event, fight, *lang)
        });
        if let Some(line) = line {
            // Every line of a fight goes to the log file too, so a lost fight can be read back.
            info!("fight: {line}");
            view.lines.push(line);
            let extra = view.lines.len().saturating_sub(LOG_LINES);
            view.lines.drain(..extra);
            view.until_next = EVENT_SECONDS;
        }
        redraw.0 = true;
        return;
    }
    if view.over.is_some() || session.is_client() {
        return;
    }
    let Some(fight) = world.fight.as_ref() else { return };
    if fight.outcome() != Outcome::Ongoing {
        view.over = world.end_fight();
        view.mode = Mode::Menu;
        if let Some(summary) = &view.over {
            session.broadcast(&Down::FightOver(summary.clone()));
        }
        redraw.0 = true;
        return;
    }
    // A hero whose player has left the table does not hold the fight up.
    let abandoned = world.acting_owner().is_some_and(|owner| !world.present.is_empty() && !world.present.iter().any(|p| p == owner));
    let events = if abandoned { world.fight_act(&world::FightIntent::EndTurn) } else { world.fight_monster_turn() };
    if !events.is_empty() {
        session.broadcast(&Down::FightEvents(events.clone()));
        view.queue.extend(events);
        view.until_next = EVENT_SECONDS;
        redraw.0 = true;
    }
}

/// A fight in a place the host is not in still has to be moved on by the host: its
/// monsters take their turns and it ends, and the guests who are in it are told.
fn drive_elsewhere(time: Res<Time>, mut game: ResMut<Game>, session: Res<Session>, mut since: Local<f32>) {
    if !session.is_host() || game.world.fight.is_none() || game.world.fight_here() {
        return;
    }
    // At the pace the guests watch it played back.
    *since += time.delta_secs();
    if *since < EVENT_SECONDS * 2.0 {
        return;
    }
    *since = 0.0;
    let world = &mut game.world;
    if world.fight.as_ref().is_some_and(|f| f.outcome() != Outcome::Ongoing) {
        if let Some(summary) = world.end_fight() {
            session.broadcast(&Down::FightOver(summary));
        }
        return;
    }
    let abandoned = world.acting_owner().is_some_and(|owner| !world.present.is_empty() && !world.present.iter().any(|p| p == owner));
    let events = if abandoned { world.fight_act(&FightIntent::EndTurn) } else { world.fight_monster_turn() };
    if !events.is_empty() {
        session.broadcast(&Down::FightEvents(events));
    }
}

/// The hero's part: keys and the mouse while moving or aiming.
#[allow(clippy::too_many_arguments)]
fn input(
    phase: Res<Phase>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    frame: Res<crate::view::View>,
    mut game: ResMut<Game>,
    mut view: ResMut<FightView>,
    mut redraw: ResMut<Redraw>,
    session: Res<Session>,
    settings: Res<Settings>,
    (gm, store, master, lang): (Res<Gm>, Res<Store>, Res<Master>, Res<Lang>),
) {
    if *phase != Phase::Fighting || !view.queue.is_empty() || view.over.is_some() || view.asking {
        return;
    }
    let world = &mut game.world;
    let Some(fight) = world.fight.clone() else { return };
    if fight.actor().side != Side::Party || !my_turn(world, &session, &settings) {
        return;
    }
    let pressed = |codes: &[KeyCode]| keyboard.any_just_pressed(codes.iter().copied());
    // A thing kept at hand: drunk or eaten for the action, or tried on the master.
    if let (Mode::Menu, true, Some(slot)) = (&view.mode, view.queue.is_empty(), crate::bag_ui::QUICK_KEYS.iter().position(|key| keyboard.just_pressed(*key))) {
        let hero = hero_index(&fight.actor().id).unwrap_or(0);
        if let Some(i) = world.quick_item(hero, slot) {
            let item = &world.story.bags[hero].inventory[i];
            if !item.heal.trim().is_empty() {
                act(FightIntent::UseItem(i), world, &mut view, &session, &mut redraw);
            } else {
                let said = format!("{} {} {}", fight.actor().name, T::UsesThing.text(*lang), item.name);
                ask_stunt(said, None, world, &mut view, &session, &gm, &store, &settings, &master);
            }
        }
        return;
    }
    // The digits are the fight's own: move, the two weapons, things, dodge, end the turn.
    if view.mode == Mode::Menu {
        let digit = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6].iter().position(|key| keyboard.just_pressed(*key));
        let actor = fight.actor();
        match digit {
            Some(0) if actor.movement_left > 0 => {
                view.cursor = actor.at();
                view.mode = Mode::Moving;
                return;
            }
            Some(n @ (1 | 2)) if actor.action_left && n - 1 < actor.strikes.len() => {
                view.target = 0;
                view.mode = Mode::Aiming(Aim::Weapon(n - 1));
                return;
            }
            Some(3) if actor.action_left && !usable(world, &fight).is_empty() => {
                view.mode = Mode::Items;
                return;
            }
            Some(4) if actor.action_left => {
                act(FightIntent::Dodge, world, &mut view, &session, &mut redraw);
                return;
            }
            Some(5) => {
                act(FightIntent::EndTurn, world, &mut view, &session, &mut redraw);
                return;
            }
            _ => {}
        }
    }
    let confirm = pressed(&[KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]);
    let clicked = mouse.just_pressed(MouseButton::Left).then(|| cell_under_cursor(&window, &frame)).flatten();
    if pressed(&[KeyCode::Escape]) && view.mode != Mode::Menu {
        view.mode = Mode::Menu;
        return;
    }
    let mode = view.mode.clone();
    let intent = match mode {
        Mode::Moving => {
            let step = (
                pressed(&[KeyCode::ArrowRight, KeyCode::KeyD]) as i32 - pressed(&[KeyCode::ArrowLeft, KeyCode::KeyA]) as i32,
                pressed(&[KeyCode::ArrowDown, KeyCode::KeyS]) as i32 - pressed(&[KeyCode::ArrowUp, KeyCode::KeyW]) as i32,
            );
            view.cursor = ((view.cursor.0 + step.0).clamp(0, MAP_W as i32 - 1), (view.cursor.1 + step.1).clamp(0, MAP_H as i32 - 1));
            let Some(goal) = clicked.or(confirm.then_some(view.cursor)) else { return };
            let blocked = |cell: Cell| world.fight_blocked(cell);
            // A cell that cannot be reached is not asked for: the mode stays, to pick again.
            if !fight.reach(&blocked).iter().any(|p| p.last() == Some(&goal)) {
                return;
            }
            FightIntent::Move(goal)
        }
        Mode::Aiming(aim) => {
            let Some(strike) = strike_of(world, &fight, &aim) else { return };
            let options = targets(&fight, &strike);
            if options.is_empty() {
                return;
            }
            let turn = pressed(&[KeyCode::ArrowRight, KeyCode::ArrowDown, KeyCode::KeyD]) as usize + (options.len() - 1) * pressed(&[KeyCode::ArrowLeft, KeyCode::ArrowUp, KeyCode::KeyA]) as usize;
            view.target = (view.target + turn) % options.len();
            let by_click = clicked.and_then(|cell| options.iter().copied().find(|i| fight.combatants[*i].at() == cell));
            let Some(target) = by_click.or(confirm.then(|| options[view.target])) else { return };
            match aim {
                Aim::Weapon(weapon) => FightIntent::Attack { target, weapon },
                Aim::Spell { index, slot } => FightIntent::Cast { spell: index, slot, target },
            }
        }
        _ => return,
    };
    act(intent, world, &mut view, &session, &mut redraw);
}

/// Highlights on the map: where the hero can walk, the cell or target chosen, whose turn it
/// is, and a bar of hit points over everyone.
fn markers(mut commands: Commands, phase: Res<Phase>, game: Res<Game>, view: Res<FightView>, session: Res<Session>, settings: Res<Settings>, old: Query<Entity, With<Marker>>) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let (Phase::Fighting, Some(fight)) = (&*phase, game.world.fight.as_ref()) else { return };
    let tile = TILE as f32;
    let centre = |cell: Cell| on_screen((cell.0 as f32 + 0.5) * tile, (cell.1 as f32 + 0.5) * tile);
    let mut square = |cell: Cell, tint: Color, size: f32, z: f32| {
        commands.spawn((Marker, Sprite { color: tint, custom_size: Some(Vec2::splat(size * crate::scene::PX as f32)), ..default() }, Transform::from_translation(centre(cell).extend(z))));
    };
    let heros_turn = fight.actor().side == Side::Party && view.queue.is_empty() && view.over.is_none();
    if heros_turn {
        square(fight.actor().at(), Color::srgba(1.0, 0.9, 0.4, 0.35), tile, 0.5);
    }
    let my_turn = heros_turn && my_turn(&game.world, &session, &settings);
    match &view.mode {
        Mode::Moving if my_turn => {
            let blocked = |cell: Cell| game.world.fight_blocked(cell);
            for path in fight.reach(&blocked) {
                square(*path.last().unwrap(), Color::srgba(0.3, 0.6, 1.0, 0.3), tile - 2.0, 0.6);
            }
            square(view.cursor, Color::srgba(1.0, 1.0, 1.0, 0.55), tile - 6.0, 0.7);
        }
        Mode::Aiming(aim) if my_turn => {
            if let Some(strike) = strike_of(&game.world, fight, aim) {
                let options = targets(fight, &strike);
                for (n, i) in options.iter().enumerate() {
                    let chosen = n == view.target % options.len();
                    square(fight.combatants[*i].at(), if chosen { Color::srgba(1.0, 0.2, 0.2, 0.6) } else { Color::srgba(1.0, 0.2, 0.2, 0.25) }, tile, 0.6);
                }
            }
        }
        _ => {}
    }
    for c in fight.combatants.iter().filter(|c| !c.dead) {
        // Over the head: a hero stands half a cell taller than the cell.
        let at = on_screen(c.x as f32 * tile + 2.0, c.y as f32 * tile - if c.side == Side::Party { 9.0 } else { 1.0 });
        let share = (c.hp.max(0) as f32 / c.max_hp.max(1) as f32).clamp(0.0, 1.0);
        let tint = if c.side == Side::Party { color(MOSS) } else { color(WAX) };
        commands.spawn((Marker, Sprite { color: color(INK), custom_size: Some(Vec2::new(24.0, 4.0)), ..default() }, Anchor::TOP_LEFT, Transform::from_translation(at.extend(depth(c.y as f32 * tile) + 0.3))));
        if share > 0.0 {
            commands.spawn((Marker, Sprite { color: tint, custom_size: Some(Vec2::new((24.0 * share).ceil(), 4.0)), ..default() }, Anchor::TOP_LEFT, Transform::from_translation(at.extend(depth(c.y as f32 * tile) + 0.31))));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn on_activate(
    used: On<sheet::FightPress>,
    mut next: ResMut<NextState<Screen>>,
    game: Option<ResMut<Game>>,
    mut view: ResMut<FightView>,
    mut phase: ResMut<Phase>,
    mut redraw: ResMut<Redraw>,
    gm: Res<Gm>,
    store: Res<Store>,
    settings: Res<Settings>,
    master: Res<Master>,
    lang: Res<Lang>,
    session: Res<Session>,
) {
    let action = &used.action;
    let Some(mut game) = game else { return };
    let world = &mut game.world;
    match action {
        FightAction::Stunt => {
            view.mode = Mode::Writing;
            return;
        }
        FightAction::SendStunt => {
            let said = used.typed.trim().to_string();
            if !said.is_empty() && world.fight.is_some() {
                ask_stunt(said, None, world, &mut view, &session, &gm, &store, &settings, &master);
            }
            return;
        }
        FightAction::Ritual(index, slot) => {
            if let Some(said) = ritual_words(world, index) {
                ask_stunt(said, Some((index.clone(), *slot)), world, &mut view, &session, &gm, &store, &settings, &master);
            }
            return;
        }
        _ => {}
    }
    match action {
        FightAction::Load => {
            next.set(Screen::Load);
            return;
        }
        FightAction::Onward => {
            *view = FightView::default();
            *phase = Phase::Walking;
            redraw.0 = true;
            return;
        }
        FightAction::MasterDecides => {
            // Nobody is left standing: the story goes on where the master takes it.
            world.revive_fallen();
            *view = FightView::default();
            redraw.0 = true;
            let said = T::DefeatSaid.text(*lang).to_string();
            master.ask(&gm, &store, &settings, Ask::Turn { world: Box::new(world.clone()), said: said.clone(), near: None, direction: None, id: 0 });
            *phase = Phase::Asking { said, near: None };
            return;
        }
        _ => {}
    }
    let Some(fight) = world.fight.as_ref() else { return };
    let intent = match action {
        FightAction::Move => {
            view.cursor = fight.actor().at();
            view.mode = Mode::Moving;
            return;
        }
        FightAction::Weapon(n) => {
            view.target = 0;
            view.mode = Mode::Aiming(Aim::Weapon(*n));
            return;
        }
        FightAction::Spells => {
            view.mode = Mode::Spells;
            return;
        }
        FightAction::Spell(index, slot) => {
            view.target = 0;
            view.mode = Mode::Aiming(Aim::Spell { index: index.clone(), slot: *slot });
            return;
        }
        FightAction::Cancel => {
            view.mode = Mode::Menu;
            return;
        }
        FightAction::Items => {
            view.mode = Mode::Items;
            return;
        }
        FightAction::Item(i) => {
            view.mode = Mode::Menu;
            FightIntent::UseItem(*i)
        }
        FightAction::Dodge => FightIntent::Dodge,
        FightAction::Dash => FightIntent::Dash,
        FightAction::Disengage => FightIntent::Disengage,
        FightAction::EndTurn => FightIntent::EndTurn,
        FightAction::Onward | FightAction::MasterDecides | FightAction::Load | FightAction::Stunt | FightAction::SendStunt | FightAction::Ritual(..) => return,
    };
    act(intent, world, &mut view, &session, &mut redraw);
}

/// A fight that starts gets a clean view. It is the fight that counts, not the screen: a
/// sheet opened and closed in the middle of one must not wipe the blows still to be shown.
fn reset(game: Res<Game>, mut view: ResMut<FightView>, mut was: Local<bool>) {
    let now = game.world.fight.is_some();
    if now && !*was {
        *view = FightView::default();
    }
    *was = now;
}

pub struct FightPlugin;

impl Plugin for FightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FightView>()
            .add_systems(Update, (reset, input, drive, drive_elsewhere, markers).chain().run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>))
            // A fight left behind (a save loaded after a defeat) leaves nothing on the next screen.
            .add_systems(OnExit(Screen::Play), |mut view: ResMut<FightView>| *view = FightView::default())
            .init_resource::<sheet::StuntField>()
            .add_systems(bevy_egui::EguiPrimaryContextPass, sheet::sheet.in_set(crate::ui::UiSet::Screens).run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>))
            .add_observer(on_activate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rules::{Character, Rng};

    fn world_in_a_fight(class: &str, skills: [&str; 2]) -> World {
        let draft: world::WorldDraft = serde_json::from_str(&world::fake::world("fight")).unwrap();
        let mut w = world::build_world(&draft, "fight", "English").unwrap().0;
        let skills: Vec<String> = skills.iter().map(|s| s.to_string()).collect();
        let mut hero = Character::create("Brak", "", "human", class, [15, 12, 14, 8, 10, 13], &skills, &[]).unwrap();
        let spells = rules::character::suggested_spells(srd(), &hero);
        hero.set_spells(&spells).unwrap();
        w.party.push(hero);
        w.here_mut().creatures[0].attitude = world::Attitude::Hostile;
        w.here_mut().creatures[0].monster = Some("goblin".into());
        assert!(w.start_fight(Rng::seeded(1)));
        let fight = w.fight.as_mut().unwrap();
        fight.turn = fight.order.iter().position(|i| *i == 0).unwrap();
        w
    }

    #[test]
    fn hero_ids_map_back_to_party_members() {
        assert_eq!(hero_index("hero_0"), Some(0));
        assert_eq!(hero_index("hero_12"), Some(12));
        assert_eq!(hero_index("goblin"), None);
    }

    #[test]
    fn a_fighter_has_no_spells_to_offer_and_a_wizard_has_the_ones_the_rules_can_run() {
        let w = world_in_a_fight("fighter", ["athletics", "perception"]);
        assert!(castable(&w, w.fight.as_ref().unwrap()).is_empty());
        let w = world_in_a_fight("wizard", ["arcana", "history"]);
        let spells = castable(&w, w.fight.as_ref().unwrap());
        assert!(!spells.is_empty());
        assert!(spells.iter().all(|(index, _)| srd().spells[index].mechanised()));
        assert!(spells.iter().any(|(_, slot)| *slot == 0) && spells.iter().any(|(_, slot)| *slot == 1));
    }

    #[test]
    fn spent_slots_take_levelled_spells_off_the_list() {
        let mut w = world_in_a_fight("wizard", ["arcana", "history"]);
        w.party[0].slots_used = [9; 9];
        let spells = castable(&w, w.fight.as_ref().unwrap());
        assert!(!spells.is_empty() && spells.iter().all(|(_, slot)| *slot == 0), "only cantrips are left");
    }

    #[test]
    fn harm_targets_the_other_side_in_range_and_healing_ones_own() {
        let mut w = world_in_a_fight("fighter", ["athletics", "perception"]);
        let fight = w.fight.as_mut().unwrap();
        fight.combatants[1].x = fight.combatants[0].x + 1;
        fight.combatants[1].y = fight.combatants[0].y;
        let sword = fight.actor().strikes[0].clone();
        assert_eq!(targets(fight, &sword), [1]);
        fight.combatants[1].x += 5;
        assert_eq!(targets(fight, &sword), Vec::<usize>::new(), "a sword does not reach six cells");
        let bow = fight.actor().strikes[1].clone();
        assert_eq!(targets(fight, &bow), [1]);
        let heal = Strike { heals: true, range: 1, long_range: 1, ..Strike::default() };
        assert_eq!(targets(fight, &heal), [0]);
    }

    #[test]
    fn events_read_as_lines_and_moves_are_not_logged() {
        let w = world_in_a_fight("fighter", ["athletics", "perception"]);
        let fight = w.fight.as_ref().unwrap();
        let hit = Event::Attacked { who: "hero_0".into(), target: "cat".into(), strike: "Longsword".into(), roll: rules::Roll { die: 14, other: None, bonus: 5 }, hit: true, critical: false, damage: 7 };
        assert_eq!(describe(&hit, fight, Lang::En).unwrap(), "Brak → Grey cat (Longsword): d20 14+5 = 19 — hits, damage 7");
        assert!(describe(&hit, fight, Lang::Ru).unwrap().contains("Длинный меч"));
        assert_eq!(describe(&Event::Moved { who: "cat".into(), path: vec![] }, fight, Lang::En), None);
        assert_eq!(describe(&Event::Died { who: "cat".into() }, fight, Lang::En).unwrap(), "Grey cat dies");
    }
}
