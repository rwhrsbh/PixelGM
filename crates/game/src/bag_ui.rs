//! The bag, the way the old dungeon crawlers kept it: one hero's bag as a grid of cells on the
//! right half of the screen, the hero with what it wears above it, the world still there on
//! the left. A thing is picked up with a click and put down with another; put down on one
//! other thing, it takes that one's place and the other comes up into the hand. Dropped
//! outside the bag, it falls on the ground by the hero, for anyone to pick up. A right click
//! uses a thing. A guest's moves are shown at once and confirmed by the host.

use crate::audio::{Play, Sfx};
use crate::coop::{Intent, Session, player_name};
use crate::i18n::{Lang, T};
use crate::icons::{CELL, icon};
use crate::play::{Game, Phase, Redraw, my_hero};
use crate::screen::UI_PIXEL;
use crate::store::Settings;
use crate::theme::*;
use bevy::prelude::*;
use std::collections::HashMap;
use world::bag::{BAG_H, BAG_W, Gear, Held, Slot, Spot, Used, gear_kind, gear_name, gear_size, worn};
use world::{Item, World};

pub mod draw;

pub struct BagPlugin;

impl Plugin for BagPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BagState>()
            .init_resource::<Said>()
            .init_resource::<draw::Clicks>()
            .init_resource::<draw::Pictures>()
            .add_systems(Update, (open, press, outside, quick_assign, quick_use).chain().run_if(in_state(crate::menu::Screen::Play)).run_if(resource_exists::<Game>))
            .add_systems(bevy_egui::EguiPrimaryContextPass, (draw::draw, draw::quick_bar).in_set(crate::ui::UiSet::Screens).run_if(in_state(crate::menu::Screen::Play)).run_if(resource_exists::<Game>));
    }
}

/// Words a player's use of a thing puts to the master, for the play screen to send.
#[derive(Resource, Default)]
pub struct Said(pub Option<String>);

/// What the bag remembers between frames.
#[derive(Resource, Default)]
pub struct BagState {
    /// Whose bag is shown.
    hero: usize,
    held: Option<Held>,
    /// What the cursor is over: a cell, or none.
    over: Option<(u8, u8)>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Target {
    /// Nothing of the bag: a right click there only lets go of what is held.
    Nothing,
    Cell(u8, u8),
    Slot(Slot),
    Loose(usize),
    PrevHero,
    NextHero,
    /// The purse: a click lays ten coins on the ground, to be picked up by another.
    Gold,
    /// A companion's hand (`true`) or back: a weapon or armour is handed over, or taken back.
    Ally(usize, bool),
    /// A companion's pack: a thing in hand is put in.
    AllyPack(usize),
    /// A thing in a companion's pack: taken out.
    AllyItem(usize, usize),
}

fn slot_label(slot: Slot) -> T {
    match slot {
        Slot::Main => T::SlotMain,
        Slot::Spare => T::SlotSpare,
        Slot::Shield => T::SlotShield,
        Slot::Body => T::SlotBody,
    }
}

/// The heroes whose bags this player may open: alone, the whole party; at a shared table,
/// one's own.
fn my_heroes(world: &World, session: &Session, settings: &Settings) -> Vec<usize> {
    if matches!(session, Session::Solo) {
        (0..world.party.len()).filter(|i| !world.party[*i].dead).collect()
    } else {
        my_hero(world, &player_name(settings), false).into_iter().collect()
    }
}

fn open(phase: Res<Phase>, mut state: ResMut<BagState>, mut game: ResMut<Game>, session: Res<Session>, mut was: Local<bool>) {
    let open = *phase == Phase::Bag;
    if open && !*was {
        if !session.is_client() && game.world.party.iter().any(|h| h.weapons.len() > 2) {
            game.world.tidy_gear();
        }
        game.world.ensure_bags();
        state.held = None;
    }
    *was = open;
}

fn bag(world: &World, hero: usize) -> &[Item] {
    world.story.bags.get(hero).map_or(&[], |b| b.inventory.as_slice())
}

fn item_name(item: &Item, ru: bool) -> String {
    match &item.gear {
        Some(index) if item.name.eq_ignore_ascii_case(&gear_name(index, false)) => gear_name(index, ru),
        _ => item.name.clone(),
    }
}

fn gear_lines(index: &str, lang: Lang) -> Vec<(String, Rgba)> {
    let data = rules::srd();
    let mut lines = Vec::new();
    let (index, better) = rules::gear(index);
    if better > 0 {
        lines.push((format!("+{better} {}", T::BagBetter.text(lang)), BRASS_LIGHT));
    }
    match gear_kind(index) {
        Some(Gear::Weapon) => {
            if let Some(w) = data.weapons.get(index) {
                let mut traits = vec![format!("{} {}", T::BagDamage.text(lang), w.dice)];
                if w.two_handed {
                    traits.push(T::BagTwoHanded.text(lang).to_string());
                }
                if w.ranged {
                    traits.push(format!("{} {}/{}", T::BagRanged.text(lang), w.range.0, w.range.1));
                }
                lines.push((traits.join(" · "), FADED));
            }
        }
        Some(Gear::Armor) => {
            if let Some(a) = data.armors.get(index) {
                let dex = if a.adds_dex { a.max_dex.map_or(" + Dex".to_string(), |m| format!(" + Dex (max {m})")) } else { String::new() };
                lines.push((format!("{} {}{dex}", T::ArmorClass.text(lang), a.base), FADED));
            }
        }
        Some(Gear::Shield) => lines.push((format!("{} +2", T::ArmorClass.text(lang)), FADED)),
        None => {}
    }
    lines
}

pub fn item_lines(item: &Item, lang: Lang) -> Vec<(String, Rgba)> {
    let ru = lang == Lang::Ru;
    let count = if item.count > 1 { format!(" ×{}", item.count) } else { String::new() };
    let mut lines = vec![(format!("{}{count}", item_name(item, ru)), BRASS_LIGHT)];
    if let Some(index) = &item.gear {
        lines.extend(gear_lines(index, lang));
    }
    if !item.heal.is_empty() {
        lines.push((format!("{} {}", T::BagHeals.text(lang), item.heal), [0x7c, 0xd0, 0x6a, 0xff]));
    }
    if !item.damage.is_empty() {
        lines.push((format!("{} {}", T::BagDamage.text(lang), item.damage), PARCHMENT));
    }
    if item.light > 0 {
        lines.push((format!("{} {}", T::BagLight.text(lang), item.light), FLAME));
    }
    if !item.description.trim().is_empty() {
        lines.push((item.description.trim().to_string(), PARCHMENT_SHADE));
    }
    match item.rarity.as_str() {
        "uncommon" => lines.push((T::RarityUncommon.text(lang).to_string(), MOSS)),
        "rare" => lines.push((T::RarityRare.text(lang).to_string(), [0x6a, 0xa0, 0xe0, 0xff])),
        "legendary" => lines.push((T::RarityLegendary.text(lang).to_string(), FLAME)),
        _ => {}
    }
    let worth = World::worth(item);
    if worth > 0 {
        lines.push((format!("{} {worth}", T::Price.text(lang)), PARCHMENT_SHADE));
    }
    lines.push((T::BagUseHint.text(lang).to_string(), FADED));
    lines
}

fn icon_key(item: &Item) -> String {
    format!("{}|{:?}|{:?}|{}", item.name, item.gear, item.size_cells, item.rows.len())
}

/// The picture of a thing: the master's own, when it drew one, or the game's.
pub fn picture(item: &Item, world: &World) -> crate::art::Pixels {
    if item.rows.is_empty() {
        return icon(item);
    }
    let (w, h) = item.size();
    let (pw, ph) = (w as i32 * CELL, h as i32 * CELL);
    let rows: Vec<Vec<u8>> = item.rows.iter().map(|r| r.chars().map(|c| c.to_digit(36).unwrap_or(0) as u8).collect()).collect();
    let (sw, sh) = (rows.iter().map(Vec::len).max().unwrap_or(1).max(1) as i32, rows.len().max(1) as i32);
    let mut p = crate::art::Pixels::new(pw as u32, ph as u32, CLEAR);
    for y in 0..ph {
        for x in 0..pw {
            let index = rows.get((y * sh / ph) as usize).and_then(|r| r.get((x * sw / pw) as usize)).copied().unwrap_or(0);
            if index != 0 {
                p.set(x, y, world.palette.rgba(index));
            }
        }
    }
    crate::scene::outline(&mut p, INK);
    p
}

/// The cell a held thing of `size` would take with the cursor at its middle, kept in the bag.
fn corner_for(cell: (u8, u8), size: (u8, u8)) -> (u8, u8) {
    let left = (cell.0 as i32 - (size.0 as i32 - 1) / 2).clamp(0, (BAG_W - size.0.min(BAG_W)) as i32) as u8;
    let top = (cell.1 as i32 - (size.1 as i32 - 1) / 2).clamp(0, (BAG_H - size.1.min(BAG_H)) as i32) as u8;
    (left, top)
}

fn held_size(world: &World, hero: usize, held: &Held) -> (u8, u8) {
    match held {
        Held::Item(i) => bag(world, hero).get(*i).map_or((1, 1), Item::size),
        Held::Worn(h, Slot::Shield) if world.held_light(*h).is_some() => world.held_light(*h).map_or((1, 1), Item::size),
        Held::Worn(h, slot) => world.party.get(*h).and_then(|hero| worn(hero, *slot)).map_or((1, 1), |g| gear_size(&g)),
    }
}

/// Carries out a move here and, at a shared table, asks the host for it.
fn carry(world: &mut World, session: &Session, bag_of: usize, held: &Held, to: Spot) -> bool {
    let done = world.arrange(bag_of, held, to);
    info!("bag: {held:?} to {to:?}: {}", if done { "done" } else { "refused" });
    if done && session.is_client() {
        session.intend(Intent::Arrange { held: held.clone(), to });
    }
    done
}

#[allow(clippy::too_many_arguments)]
fn press(
    mut phase: ResMut<Phase>,
    mut clicks: ResMut<draw::Clicks>,
    mut game: ResMut<Game>,
    session: Res<Session>,
    settings: Res<Settings>,
    mut state: ResMut<BagState>,
    mut sound: MessageWriter<Play>,
    mut said: ResMut<Said>,
    lang: Res<Lang>,
    (keyboard, mut use_for, shopping, mut deal): (Res<ButtonInput<KeyCode>>, ResMut<crate::play::UseFor>, Res<crate::shop_ui::Shopping>, ResMut<crate::shop_ui::Deal>),
) {
    if *phase != Phase::Bag {
        return;
    }
    let hero = state.hero;
    let hits = std::mem::take(&mut clicks.hits);
    // A right click lets go of what is held, or uses what is under the cursor.
    if let Some(under) = hits.iter().find(|(_, right)| *right).map(|(target, _)| *target) {
        if state.held.take().is_some() {
            sound.write(Play(Sfx::Back));
            return;
        }
        if let Target::Cell(x, y) = under
            && let Some(i) = game.world.story.bags.get(hero).and_then(|b| b.item_at((x, y)))
        {
            let name = bag(&game.world, hero)[i].name.clone();
            // In a shop a right click sells.
            if let Some(keeper) = shopping.0.clone() {
                if session.is_client() {
                    session.intend(Intent::Sell { keeper: keeper.clone(), index: i });
                }
                match game.world.sell(hero, &keeper, i) {
                    Ok((sold, coins)) => {
                        deal.0 = format!("{} {sold} (+{coins})", T::Sold.text(*lang));
                        sound.write(Play(Sfx::Item));
                    }
                    Err(trouble) => {
                        deal.0 = crate::shop_ui::trouble_words(trouble, *lang);
                        sound.write(Play(Sfx::Error));
                    }
                }
                return;
            }
            // With Alt held the player says what the thing is used for: the field opens, and
            // what is typed goes to the master after "X uses Y:".
            if keyboard.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]) {
                let who = game.world.party.get(hero).map_or(String::new(), |h| h.name.clone());
                use_for.0 = Some(format!("{who} {} {name}", T::UsesThing.text(*lang)));
                *phase = Phase::Writing { near: None };
                return;
            }
            if session.is_client() {
                session.intend(Intent::UseItem { index: i });
            }
            match game.world.use_item(hero, i, &mut rules::Rng::from_clock()) {
                Used::Healed(_) => {
                    sound.write(Play(Sfx::Heal));
                }
                Used::Ask => {
                    // What the game has no rule for, the master tells: the bag closes and the
                    // master is asked.
                    let who = game.world.party.get(hero).map_or(String::new(), |h| h.name.clone());
                    said.0 = Some(format!("{who} {} {name}", T::UsesThing.text(*lang)));
                    *phase = Phase::Walking;
                }
            }
        }
        return;
    }
    let mine = my_heroes(&game.world, &session, &settings);
    for (target, _) in hits.iter().filter(|(_, right)| !right) {
        let world = &mut game.world;
        let to = match (*target, &state.held) {
            (Target::AllyPack(n), Some(Held::Item(i))) => {
                if session.is_client() {
                    session.intend(Intent::PackCompanion { companion: n, index: *i });
                }
                let done = world.pack_companion(n, hero, *i);
                if done {
                    state.held = None;
                }
                sound.write(Play(if done { Sfx::Item } else { Sfx::Error }));
                continue;
            }
            (Target::AllyPack(_), _) | (Target::Nothing, _) => continue,
            (Target::AllyItem(n, k), held) => {
                // With a thing in hand it goes into the pack; with none, this one comes out.
                let done = match held {
                    Some(Held::Item(i)) => {
                        if session.is_client() {
                            session.intend(Intent::PackCompanion { companion: n, index: *i });
                        }
                        world.pack_companion(n, hero, *i)
                    }
                    Some(Held::Worn(..)) => false,
                    None => {
                        if session.is_client() {
                            session.intend(Intent::UnpackCompanion { companion: n, item: k });
                        }
                        world.unpack_companion(n, hero, k)
                    }
                };
                if done {
                    state.held = None;
                }
                sound.write(Play(if done { Sfx::Item } else { Sfx::Error }));
                continue;
            }
            (Target::Ally(n, hand), held) => {
                // With a weapon or armour in hand: given. With nothing: taken back.
                let done = match held {
                    Some(Held::Item(i)) => {
                        if session.is_client() {
                            session.intend(Intent::ArmCompanion { companion: n, index: *i });
                        }
                        world.arm_companion(n, hero, *i)
                    }
                    Some(Held::Worn(..)) => false,
                    None => {
                        if session.is_client() {
                            session.intend(Intent::DisarmCompanion { companion: n, weapon: hand });
                        }
                        world.disarm_companion(n, hero, hand)
                    }
                };
                if done {
                    state.held = None;
                }
                sound.write(Play(if done { Sfx::Item } else { Sfx::Error }));
                continue;
            }
            (Target::Gold, _) => {
                // Ten coins, or what is left, laid beside the hero.
                let (cx, cy) = world.player.cell();
                let here = world.here();
                let free = |(x, y): (i32, i32)| (0..world::MAX_W as i32).contains(&x) && (0..world::MAX_H as i32).contains(&y) && !here.blocked(x, y, &world.tiles) && here.exit_at(x as usize, y as usize).is_none();
                let around = [(1, 0), (0, 1), (-1, 0), (1, 1), (-1, 1), (0, -1), (1, -1), (-1, -1)];
                let spot = around.iter().map(|(dx, dy)| (cx + dx, cy + dy)).find(|c| free(*c)).unwrap_or((cx, cy));
                let cell = (spot.0.max(0) as usize, spot.1.max(0) as usize);
                if session.is_client() {
                    session.intend(Intent::DropGold { amount: 10, cell });
                }
                let done = world.drop_gold(hero, 10, cell);
                sound.write(Play(if done { Sfx::Item } else { Sfx::Error }));
                continue;
            }
            (Target::PrevHero | Target::NextHero, _) => {
                if let Some(at) = mine.iter().position(|h| *h == hero) {
                    let step = if matches!(target, Target::NextHero) { 1 } else { mine.len() - 1 };
                    state.hero = mine[(at + step) % mine.len()];
                }
                state.held = None;
                sound.write(Play(Sfx::Page));
                continue;
            }
            (Target::Cell(x, y), Some(held)) => {
                let (left, top) = corner_for((x, y), held_size(world, hero, held));
                Spot::Bag(left, top)
            }
            (Target::Slot(slot), Some(_)) => Spot::Worn(hero, slot),
            (Target::Cell(x, y), None) => {
                if let Some(i) = world.story.bags.get(hero).and_then(|b| b.item_at((x, y))) {
                    state.held = Some(Held::Item(i));
                    sound.write(Play(Sfx::Item));
                }
                continue;
            }
            (Target::Slot(slot), None) => {
                let has = world.party.get(hero).and_then(|h| worn(h, slot)).is_some() || (slot == Slot::Shield && world.held_light(hero).is_some());
                if has {
                    state.held = Some(Held::Worn(hero, slot));
                    sound.write(Play(Sfx::Item));
                }
                continue;
            }
            (Target::Loose(i), _) => {
                state.held = Some(Held::Item(i));
                sound.write(Play(Sfx::Item));
                continue;
            }
        };
        let Some(held) = state.held.clone() else { continue };
        // A thing put down on exactly one other lifts that one: it is the one left without a place.
        let before: Vec<bool> = bag(world, hero).iter().map(|i| i.at.is_some()).collect();
        if carry(world, &session, hero, &held, to) {
            let lifted = bag(world, hero).iter().enumerate().find(|(n, i)| i.at.is_none() && before.get(*n).copied().unwrap_or(false)).map(|(n, _)| n);
            state.held = lifted.map(Held::Item);
            sound.write(Play(if lifted.is_some() { Sfx::Item } else { Sfx::Confirm }));
        } else {
            sound.write(Play(Sfx::Error));
        }
    }
}

pub const QUICK_KEYS: [KeyCode; world::bag::QUICK_SLOTS] = [KeyCode::F1, KeyCode::F2, KeyCode::F3, KeyCode::F4, KeyCode::F5, KeyCode::F6];

/// With the bag open, F1-F6 over a thing keeps it at hand under that key.
fn quick_assign(phase: Res<Phase>, keyboard: Res<ButtonInput<KeyCode>>, mut game: ResMut<Game>, session: Res<Session>, state: Res<BagState>, mut sound: MessageWriter<Play>) {
    if *phase != Phase::Bag {
        return;
    }
    let Some(slot) = QUICK_KEYS.iter().position(|key| keyboard.just_pressed(*key)) else { return };
    let hero = state.hero;
    let name = state.over.and_then(|cell| game.world.story.bags.get(hero).and_then(|b| b.item_at(cell))).map(|i| bag(&game.world, hero)[i].name.clone()).unwrap_or_default();
    if session.is_client() {
        session.intend(Intent::Quick { slot, name: name.clone() });
    }
    game.world.set_quick(hero, slot, &name);
    sound.write(Play(if name.is_empty() { Sfx::Back } else { Sfx::Item }));
}

/// F1-F6 while walking uses the thing kept under that key, as a right click in the bag would.
#[allow(clippy::too_many_arguments)]
fn quick_use(mut phase: ResMut<Phase>, keyboard: Res<ButtonInput<KeyCode>>, mut game: ResMut<Game>, session: Res<Session>, settings: Res<Settings>, mut sound: MessageWriter<Play>, mut said: ResMut<Said>, lang: Res<Lang>) {
    if *phase != Phase::Walking {
        return;
    }
    let Some(slot) = QUICK_KEYS.iter().position(|key| keyboard.just_pressed(*key)) else { return };
    let me = player_name(&settings);
    let hero = crate::play::my_hero(&game.world, &me, matches!(*session, Session::Solo)).unwrap_or(0);
    let Some(i) = game.world.quick_item(hero, slot) else {
        sound.write(Play(Sfx::Error));
        return;
    };
    let name = bag(&game.world, hero)[i].name.clone();
    if session.is_client() {
        session.intend(Intent::UseItem { index: i });
    }
    match game.world.use_item(hero, i, &mut rules::Rng::from_clock()) {
        Used::Healed(_) => {
            sound.write(Play(Sfx::Heal));
        }
        Used::Ask => {
            let who = game.world.party.get(hero).map_or(String::new(), |h| h.name.clone());
            said.0 = Some(format!("{who} {} {name}", T::UsesThing.text(*lang)));
            *phase = Phase::Walking;
        }
    }
}

/// A click outside the bag with a thing in hand throws it on the ground by the hero.
#[allow(clippy::too_many_arguments)]
fn outside(
    phase: Res<Phase>,
    mut clicks: ResMut<draw::Clicks>,
    mut game: ResMut<Game>,
    session: Res<Session>,
    mut state: ResMut<BagState>,
    mut redraw: ResMut<Redraw>,
    mut sound: MessageWriter<Play>,
    mut throws: MessageWriter<crate::fx::Thrown>,
    mut landing: ResMut<crate::fx::Landing>,
) {
    if !std::mem::take(&mut clicks.outside) || *phase != Phase::Bag || state.held.is_none() {
        return;
    }
    let hero = state.hero;
    let world = &mut game.world;
    // Gear in hand goes into the bag first, to be thrown from there.
    let index = match state.held.clone() {
        Some(Held::Item(i)) => Some(i),
        Some(held @ Held::Worn(..)) => {
            let spot = world.story.bags.get(hero).and_then(|b| b.free_spot(held_size(world, hero, &held), None));
            spot.filter(|(x, y)| carry(world, &session, hero, &held, Spot::Bag(*x, *y))).and_then(|(x, y)| world.story.bags[hero].item_at((x, y)))
        }
        None => None,
    };
    let Some(i) = index else {
        sound.write(Play(Sfx::Error));
        return;
    };
    // Beside the hero, as in the old games: the first open cell around, nearest first.
    let (cx, cy) = world.player.cell();
    let here = world.here();
    let free = |(x, y): (i32, i32)| (0..world::MAX_W as i32).contains(&x) && (0..world::MAX_H as i32).contains(&y) && !here.blocked(x, y, &world.tiles) && here.exit_at(x as usize, y as usize).is_none() && !here.items.iter().any(|l| (l.x as i32, l.y as i32) == (x, y));
    let around = [(1, 0), (0, 1), (-1, 0), (1, 1), (-1, 1), (0, -1), (1, -1), (-1, -1)];
    let spot = around.iter().map(|(dx, dy)| (cx + dx, cy + dy)).find(|c| free(*c)).unwrap_or((cx, cy));
    let cell = (spot.0.max(0) as usize, spot.1.max(0) as usize);
    let item = bag(world, hero).get(i).cloned();
    if session.is_client() {
        session.intend(Intent::Drop { index: i, cell });
    }
    if world.drop_item(hero, i, cell) {
        if let Some(item) = item {
            // The thing on the ground stays unseen until the thrown one lands on it.
            landing.0.push((cell, crate::fx::FLIGHT_SECONDS));
            throws.write(crate::fx::Thrown { item, cell });
        }
        state.held = None;
        redraw.0 = true;
        sound.write(Play(Sfx::Item));
    }
}
