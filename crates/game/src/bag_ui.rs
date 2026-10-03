//! The bag, the way the old dungeon crawlers kept it: one hero's bag as a grid of cells on the
//! right half of the screen, the hero with what it wears above it, the world still there on
//! the left. A thing is picked up with a click and put down with another; put down on one
//! other thing, it takes that one's place and the other comes up into the hand. Dropped
//! outside the bag, it falls on the ground by the hero, for anyone to pick up. A right click
//! uses a thing. A guest's moves are shown at once and confirmed by the host.

use crate::art::Art;
use crate::audio::{Play, Sfx};
use crate::coop::{Intent, Session, player_name};
use crate::i18n::{Lang, T};
use crate::icons::{CELL, icon};
use crate::play::{Game, Phase, Redraw, my_hero};
use crate::screen::{UI_PIXEL, fit};
use crate::store::Settings;
use crate::theme::*;
use crate::widgets::panel_backing;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::window::PrimaryWindow;
use std::collections::HashMap;
use world::bag::{BAG_H, BAG_W, Gear, Held, Slot, Spot, Used, gear_kind, gear_name, gear_size, worn};
use world::{Item, World};

pub struct BagPlugin;

impl Plugin for BagPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BagState>()
            .init_resource::<Said>()
            .add_systems(Update, (open, build, press, outside, hover, follow, highlight, quick_assign, quick_bar, quick_use).chain().run_if(in_state(crate::menu::Screen::Play)).run_if(resource_exists::<Game>));
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
    tip: Vec<(String, Rgba)>,
    built: String,
    icons: HashMap<String, Handle<Image>>,
    dolls: HashMap<String, Vec<Handle<Image>>>,
}

#[derive(Component)]
struct BagRoot;

#[derive(Component)]
struct BagContent;

#[derive(Component, Clone, Copy)]
enum Target {
    Cell(u8, u8),
    Slot(Slot),
    Loose(usize),
    PrevHero,
    NextHero,
}

#[derive(Component)]
struct HeldIcon;

#[derive(Component)]
struct Tip;

#[derive(Component)]
struct Shadow;

/// The panel on the right half of the interface, and its parts, in pixels of the interface.
const PANEL: (f32, f32, f32, f32) = (240.0, 2.0, 238.0, 252.0);
const GRID: (f32, f32) = (23.0, 116.0);
const DOLL: (f32, f32) = (50.0, 16.0);

fn slot_box(slot: Slot) -> (f32, f32, u8, u8) {
    match slot {
        Slot::Main => (12.0, 20.0, 2, 4),
        Slot::Spare => (190.0, 20.0, 2, 4),
        Slot::Shield => (154.0, 20.0, 2, 2),
        Slot::Body => (118.0, 20.0, 2, 3),
    }
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

fn open(mut commands: Commands, phase: Res<Phase>, roots: Query<Entity, With<BagRoot>>, mut state: ResMut<BagState>, mut game: ResMut<Game>, session: Res<Session>, mut was: Local<bool>) {
    let open = *phase == Phase::Bag;
    if open && !*was {
        if !session.is_client() && game.world.party.iter().any(|h| h.weapons.len() > 2) {
            game.world.tidy_gear();
        }
        game.world.ensure_bags();
        commands.spawn((BagRoot, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() }, GlobalZIndex(10), Pickable::IGNORE, FocusPolicy::Pass));
        state.built.clear();
        state.held = None;
    }
    if !open {
        for root in &roots {
            commands.entity(root).despawn();
        }
    }
    *was = open;
}

fn bag(world: &World, hero: usize) -> &[Item] {
    world.story.bags.get(hero).map_or(&[], |b| b.inventory.as_slice())
}

fn fingerprint(world: &World, state: &BagState, hero: Option<usize>, lang: Lang) -> String {
    let h = hero.unwrap_or(0);
    let worn: Vec<_> = hero.and_then(|h| world.party.get(h)).map(|h| Slot::ALL.map(|s| worn(h, s))).into_iter().collect();
    format!("{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}", bag(world, h), world.held_light(h), worn, hero, state.held, lang, hero.and_then(|h| world.party.get(h)).map(|h| (h.hp, h.max_hp, h.look)))
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

fn item_lines(item: &Item, lang: Lang) -> Vec<(String, Rgba)> {
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
    lines.push((T::BagUseHint.text(lang).to_string(), FADED));
    lines
}

fn abs(left: f32, top: f32, width: f32, height: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: px(left), top: px(top), width: px(width), height: px(height), ..default() }
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

#[allow(clippy::too_many_arguments)]
fn build(
    mut commands: Commands,
    phase: Res<Phase>,
    game: Res<Game>,
    session: Res<Session>,
    settings: Res<Settings>,
    lang: Res<Lang>,
    art: Res<Art>,
    fonts: Res<Fonts>,
    roots: Query<Entity, With<BagRoot>>,
    contents: Query<Entity, With<BagContent>>,
    mut state: ResMut<BagState>,
    mut images: ResMut<Assets<Image>>,
) {
    if *phase != Phase::Bag {
        return;
    }
    let Ok(root) = roots.single() else { return };
    let world = &game.world;
    let mine = my_heroes(world, &session, &settings);
    if !mine.contains(&state.hero) {
        state.hero = mine.first().copied().unwrap_or(0);
    }
    let hero_index = mine.contains(&state.hero).then_some(state.hero);
    let print = fingerprint(world, &state, hero_index, *lang);
    if print == state.built {
        return;
    }
    state.built = print;
    for old in &contents {
        commands.entity(old).despawn();
    }
    let ru = *lang == Lang::Ru;
    let h = hero_index.unwrap_or(0);
    let mut icon_of = |item: &Item, state: &mut BagState| -> Handle<Image> { state.icons.entry(icon_key(item)).or_insert_with(|| images.add(picture(item, world).into_image())).clone() };
    let held_item = match &state.held {
        Some(Held::Item(i)) => Some(*i),
        _ => None,
    };
    let held_slot = match &state.held {
        Some(Held::Worn(_, slot)) => Some(*slot),
        _ => None,
    };
    let cell = CELL as f32;
    let placed: Vec<(usize, (u8, u8), (u8, u8), Handle<Image>)> = bag(world, h).iter().enumerate().filter_map(|(i, item)| item.at.map(|at| (i, at, item.size(), icon_of(item, &mut state)))).collect();
    let hero = hero_index.and_then(|h| world.party.get(h));
    let mut worn_icons: Vec<(Slot, Handle<Image>)> = hero
        .map(|hero| Slot::ALL.iter().filter_map(|slot| worn(hero, *slot).map(|index| (*slot, icon_of(&Item { name: gear_name(&index, false), gear: Some(index), count: 1, ..default() }, &mut state)))).collect())
        .unwrap_or_default();
    // A light in the off hand shows where a shield would.
    if let Some(light) = world.held_light(h).cloned() {
        worn_icons.push((Slot::Shield, icon_of(&light, &mut state)));
    }
    let doll = hero.map(|hero| {
        let key = format!("{}|{}", crate::doll::key(hero), world.held_light(h).is_some());
        state.dolls.entry(key).or_insert_with(|| crate::doll::frames_holding(hero, world.held_light(h).is_some()).into_iter().map(|f| images.add(f.into_image())).collect())[0].clone()
    });
    let loose: Vec<(usize, String)> = bag(world, h).iter().enumerate().filter(|(_, i)| i.at.is_none()).map(|(n, i)| (n, item_name(i, ru))).collect();

    commands.entity(root).with_children(|root| {
        root.spawn((BagContent, abs(PANEL.0, PANEL.1, PANEL.2, PANEL.3))).with_children(|panel| {
            panel.spawn(panel_backing(&art));
            if let Some(hero) = hero {
                let name = format!("{} · {} {} · {} {}/{}", hero.name, T::ArmorClass.text(*lang), hero.ac(), T::HitPoints.text(*lang), hero.hp, hero.max_hp);
                panel.spawn((abs(10.0, 6.0, 200.0, 10.0), Text::new(name), fonts.body(), TextColor(color(INK)), TextLayout::no_wrap()));
                if mine.len() > 1 {
                    for (left, target, mark) in [(198.0, Target::PrevHero, "<"), (214.0, Target::NextHero, ">")] {
                        panel.spawn((abs(left, 4.0, 14.0, 12.0), Button, target, BackgroundColor(color(PARCHMENT_DARK)))).with_children(|b| {
                            b.spawn((Text::new(mark), fonts.body(), TextColor(color(INK))));
                        });
                    }
                }
            }
            panel.spawn((abs(DOLL.0 - 4.0, DOLL.1 - 2.0, 64.0, 98.0), BackgroundColor(Color::srgba(0.2, 0.15, 0.1, 0.25))));
            if let Some(doll) = doll {
                panel.spawn((abs(DOLL.0, DOLL.1, 56.0, 84.0), ImageNode::new(doll), Pickable::IGNORE));
            }
            for slot in Slot::ALL {
                let (left, top, w, hh) = slot_box(slot);
                let holds = worn_icons.iter().find(|(s, _)| *s == slot).map(|(_, image)| image.clone());
                panel
                    .spawn((Node { border: UiRect::all(px(1)), ..abs(left, top, w as f32 * cell, hh as f32 * cell) }, Button, Target::Slot(slot), BackgroundColor(color(WOOD_DARK)), BorderColor::all(color(BRASS_DARK))))
                    .with_children(|slot_node| {
                        if let Some(image) = holds {
                            let tint = if held_slot == Some(slot) { Color::srgba(1.0, 1.0, 1.0, 0.35) } else { Color::WHITE };
                            slot_node
                                .spawn((Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, Pickable::IGNORE, FocusPolicy::Pass))
                                .with_children(|centre| {
                                    centre.spawn((ImageNode { color: tint, ..ImageNode::new(image) }, Pickable::IGNORE, FocusPolicy::Pass));
                                });
                        }
                    });
            }
            panel.spawn((abs(GRID.0 - 2.0, GRID.1 - 2.0, BAG_W as f32 * cell + 4.0, BAG_H as f32 * cell + 4.0), BackgroundColor(color(WOOD_DEEP))));
            for y in 0..BAG_H {
                for x in 0..BAG_W {
                    let shade = if (x + y) % 2 == 0 { WOOD_DARK } else { [0x3c, 0x24, 0x17, 0xff] };
                    panel.spawn((Node { border: UiRect::all(px(1)), ..abs(GRID.0 + x as f32 * cell, GRID.1 + y as f32 * cell, cell, cell) }, Button, Target::Cell(x, y), BackgroundColor(color(shade)), BorderColor::all(color(WOOD_DEEP))));
                }
            }
            for (i, (x, y), (w, hh), image) in placed {
                let (left, top) = (GRID.0 + x as f32 * cell, GRID.1 + y as f32 * cell);
                let tint = if held_item == Some(i) { Color::srgba(1.0, 1.0, 1.0, 0.35) } else { Color::WHITE };
                panel.spawn((Node { border: UiRect::all(px(1)), ..abs(left, top, w as f32 * cell, hh as f32 * cell) }, BackgroundColor(Color::srgba(0.25, 0.32, 0.55, 0.45)), BorderColor::all(Color::srgba(0.45, 0.55, 0.85, 0.7)), Pickable::IGNORE, FocusPolicy::Pass));
                panel.spawn((abs(left, top, w as f32 * cell, hh as f32 * cell), ImageNode { color: tint, ..ImageNode::new(image) }, Pickable::IGNORE, FocusPolicy::Pass));
            }
            // Where the held thing would go: green to lie, yellow to swap, red where it cannot.
            panel.spawn((Shadow, Node { border: UiRect::all(px(1)), display: Display::None, ..abs(0.0, 0.0, cell, cell) }, BackgroundColor(Color::NONE), BorderColor::all(Color::NONE), Pickable::IGNORE, FocusPolicy::Pass, GlobalZIndex(11)));
            if !loose.is_empty() {
                panel.spawn(Node { position_type: PositionType::Absolute, left: px(118.0), top: px(76.0), width: px(110.0), flex_wrap: FlexWrap::Wrap, column_gap: px(2), row_gap: px(2), ..default() }).with_children(|line| {
                    line.spawn((Text::new(T::BagNoRoom.text(*lang)), fonts.body(), TextColor(color(WAX))));
                    for (i, name) in loose.iter().take(3) {
                        line.spawn((Node { padding: UiRect::axes(px(3), px(1)), ..default() }, Button, Target::Loose(*i), BackgroundColor(color(PARCHMENT_DARK)))).with_children(|b| {
                            b.spawn((Text::new(name.chars().take(14).collect::<String>()), fonts.body(), TextColor(color(INK)), TextLayout::no_wrap()));
                        });
                    }
                });
            }
        });
        root.spawn((BagContent, Tip, Node { position_type: PositionType::Absolute, display: Display::None, padding: UiRect::all(px(4)), flex_direction: FlexDirection::Column, row_gap: px(1), max_width: px(170), ..default() }, BackgroundColor(Color::srgba(0.06, 0.04, 0.03, 0.92)), BorderColor::all(color(BRASS_DARK)), Pickable::IGNORE, FocusPolicy::Pass, GlobalZIndex(13)));
        root.spawn((BagContent, HeldIcon, Node { position_type: PositionType::Absolute, ..default() }, Pickable::IGNORE, FocusPolicy::Pass, GlobalZIndex(12)));
    });
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
    targets: Query<(&Target, &Interaction), Changed<Interaction>>,
    all_targets: Query<(&Target, &Interaction)>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut game: ResMut<Game>,
    session: Res<Session>,
    settings: Res<Settings>,
    mut state: ResMut<BagState>,
    mut sound: MessageWriter<Play>,
    mut said: ResMut<Said>,
    lang: Res<Lang>,
    (keyboard, mut use_for): (Res<ButtonInput<KeyCode>>, ResMut<crate::play::UseFor>),
) {
    if *phase != Phase::Bag {
        return;
    }
    let hero = state.hero;
    // A right click lets go of what is held, or uses what is under the cursor.
    if mouse.just_pressed(MouseButton::Right) {
        if state.held.take().is_some() {
            state.built.clear();
            sound.write(Play(Sfx::Back));
            return;
        }
        let under = all_targets.iter().find(|(_, i)| **i != Interaction::None).map(|(t, _)| *t);
        if let Some(Target::Cell(x, y)) = under
            && let Some(i) = game.world.story.bags.get(hero).and_then(|b| b.item_at((x, y)))
        {
            let name = bag(&game.world, hero)[i].name.clone();
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
    for (target, interaction) in &targets {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let world = &mut game.world;
        let to = match (*target, &state.held) {
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

#[derive(Component)]
struct QuickBar;

/// The things kept at hand, in the strip at the bottom: a picture, how many, and the key.
#[allow(clippy::too_many_arguments)]
fn quick_bar(mut commands: Commands, game: Res<Game>, session: Res<Session>, settings: Res<Settings>, fonts: Res<Fonts>, mut images: ResMut<Assets<Image>>, old: Query<Entity, With<QuickBar>>, mut shown: Local<String>) {
    let world = &game.world;
    let me = player_name(&settings);
    let hero = crate::play::my_hero(world, &me, matches!(*session, Session::Solo)).unwrap_or(0);
    let slots: Vec<Option<&Item>> = (0..world::bag::QUICK_SLOTS).map(|slot| world.quick_item(hero, slot).and_then(|i| bag(world, hero).get(i))).collect();
    let print = format!("{hero}|{:?}", slots.iter().map(|s| s.map(|i| (&i.name, i.count, i.rows.len()))).collect::<Vec<_>>());
    if *shown == print && !old.is_empty() {
        return;
    }
    *shown = print;
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let height = crate::play::HUD_HEIGHT;
    let bar = Node { position_type: PositionType::Absolute, left: px(112), bottom: px(1), height: px(height - 2.0), column_gap: px(2), align_items: AlignItems::Center, ..default() };
    commands.spawn((QuickBar, DespawnOnExit(crate::menu::Screen::Play), bar, GlobalZIndex(9), Pickable::IGNORE)).with_children(|row| {
        for (n, slot) in slots.iter().enumerate() {
            row.spawn((Node { width: px(height - 2.0), height: px(height - 2.0), border: UiRect::all(px(1)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(color(WOOD_DEEP)), BorderColor::all(color(if slot.is_some() { BRASS } else { WOOD_DARK })), Pickable::IGNORE))
                .with_children(|cell| match slot {
                    Some(item) => {
                        let image = images.add(picture(item, world).into_image());
                        cell.spawn((Node { width: px(height - 4.0), height: px(height - 4.0), ..default() }, ImageNode::new(image), Pickable::IGNORE));
                        if item.count > 1 {
                            cell.spawn((Node { position_type: PositionType::Absolute, right: px(0), bottom: px(-2), ..default() }, Text::new(item.count.to_string()), fonts.body(), TextColor(color(PARCHMENT)), Pickable::IGNORE));
                        }
                    }
                    None => {
                        cell.spawn((Text::new((n + 1).to_string()), fonts.body(), TextColor(color(WOOD_LIGHT)), Pickable::IGNORE));
                    }
                });
        }
    });
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

/// The cursor on the grid of the interface, if it is over the window.
fn cursor(window: &Window) -> Option<Vec2> {
    let at = window.cursor_position()?;
    let (scale, origin, _) = fit(window.physical_size());
    Some((at * window.scale_factor() - origin.as_vec2()) / (scale * UI_PIXEL))
}

/// A click outside the bag with a thing in hand throws it on the ground by the hero.
#[allow(clippy::too_many_arguments)]
fn outside(
    phase: Res<Phase>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut game: ResMut<Game>,
    session: Res<Session>,
    mut state: ResMut<BagState>,
    mut redraw: ResMut<Redraw>,
    mut sound: MessageWriter<Play>,
    mut throws: MessageWriter<crate::fx::Thrown>,
    mut landing: ResMut<crate::fx::Landing>,
) {
    if *phase != Phase::Bag || !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(at) = cursor(&window) else { return };
    if at.x >= PANEL.0 || state.held.is_none() {
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
    let free = |(x, y): (i32, i32)| (0..world::MAP_W as i32).contains(&x) && (0..world::MAP_H as i32).contains(&y) && !here.blocked(x, y, &world.tiles) && here.exit_at(x as usize, y as usize).is_none() && !here.items.iter().any(|l| (l.x as i32, l.y as i32) == (x, y));
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

/// The tip by the cursor tells what it is over.
#[allow(clippy::too_many_arguments)]
fn hover(
    phase: Res<Phase>,
    targets: Query<(&Target, &Interaction)>,
    game: Res<Game>,
    lang: Res<Lang>,
    fonts: Res<Fonts>,
    mut state: ResMut<BagState>,
    mut tips: Query<(Entity, &mut Node), With<Tip>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    if *phase != Phase::Bag {
        return;
    }
    let world = &game.world;
    let hero = state.hero;
    let mut lines: Vec<(String, Rgba)> = Vec::new();
    state.over = None;
    for (target, interaction) in &targets {
        if *interaction == Interaction::None {
            continue;
        }
        match *target {
            Target::Cell(x, y) => {
                state.over = Some((x, y));
                if state.held.is_none()
                    && let Some(item) = world.story.bags.get(hero).and_then(|b| b.item_at((x, y))).and_then(|i| bag(world, hero).get(i))
                {
                    lines = item_lines(item, *lang);
                }
            }
            Target::Loose(i) => {
                if let Some(item) = bag(world, hero).get(i) {
                    lines = item_lines(item, *lang);
                }
            }
            Target::Slot(slot) if state.held.is_none() => {
                lines.push((slot_label(slot).text(*lang).to_string(), FADED));
                if let (Slot::Shield, Some(light)) = (slot, world.held_light(hero)) {
                    lines.extend(item_lines(light, *lang));
                } else if let Some(index) = world.party.get(hero).and_then(|h| worn(h, slot)) {
                    lines.push((gear_name(&index, *lang == Lang::Ru), BRASS_LIGHT));
                    lines.extend(gear_lines(&index, *lang));
                } else {
                    lines.push((T::BagEmptySlot.text(*lang).to_string(), FADED));
                }
            }
            _ => {}
        }
    }
    let Ok((entity, mut node)) = tips.single_mut() else { return };
    if let Some(at) = cursor(&window) {
        // Beside the cursor, kept on the screen.
        node.left = px((at.x + 10.0).min(crate::screen::UI.x as f32 - 172.0));
        node.top = px((at.y + 8.0).min(crate::screen::UI.y as f32 - 60.0));
    }
    let shown = if lines.is_empty() { Display::None } else { Display::Flex };
    if node.display != shown {
        node.display = shown;
    }
    if lines == state.tip {
        return;
    }
    state.tip = lines.clone();
    commands.entity(entity).despawn_children().with_children(|tip| {
        for (text, tint) in &lines {
            tip.spawn((Text::new(text.clone()), fonts.body(), TextColor(color(*tint))));
        }
    });
}

/// Where a held thing would go, outlined: green to lie, yellow to swap, red where it cannot.
fn highlight(phase: Res<Phase>, state: Res<BagState>, game: Res<Game>, mut shadows: Query<(&mut Node, &mut BackgroundColor, &mut BorderColor), With<Shadow>>) {
    let Ok((mut node, mut fill, mut border)) = shadows.single_mut() else { return };
    let (Some(held), Some(cell), true) = (&state.held, state.over, *phase == Phase::Bag) else {
        if node.display != Display::None {
            node.display = Display::None;
        }
        return;
    };
    let world = &game.world;
    let size = held_size(world, state.hero, held);
    let at = corner_for(cell, size);
    let ignore = match held {
        Held::Item(i) => Some(*i),
        _ => None,
    };
    let under = world.story.bags.get(state.hero).map_or(0, |b| b.overlapped(size, at, ignore).len());
    let tone = match under {
        0 => Color::srgba(0.3, 0.9, 0.35, 0.35),
        1 => Color::srgba(0.95, 0.85, 0.3, 0.35),
        _ => Color::srgba(0.95, 0.25, 0.2, 0.4),
    };
    let c = CELL as f32;
    node.display = Display::Flex;
    node.left = px(GRID.0 + at.0 as f32 * c);
    node.top = px(GRID.1 + at.1 as f32 * c);
    node.width = px(size.0 as f32 * c);
    node.height = px(size.1 as f32 * c);
    fill.0 = tone;
    *border = BorderColor::all(tone.with_alpha(0.9));
}

/// The held thing follows the cursor, centred on it as in the old games.
#[allow(clippy::too_many_arguments)]
fn follow(
    phase: Res<Phase>,
    state: Res<BagState>,
    game: Res<Game>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut held: Query<(Entity, &mut Node), With<HeldIcon>>,
    mut commands: Commands,
    mut shown: Local<Option<(Entity, Option<Held>)>>,
) {
    if *phase != Phase::Bag {
        *shown = None;
        return;
    }
    let Ok((entity, mut node)) = held.single_mut() else { return };
    if *shown != Some((entity, state.held.clone())) {
        *shown = Some((entity, state.held.clone()));
        commands.entity(entity).despawn_children();
        let hero = state.hero;
        let item = match &state.held {
            Some(Held::Item(i)) => bag(&game.world, hero).get(*i).cloned(),
            Some(Held::Worn(h, Slot::Shield)) if game.world.held_light(*h).is_some() => game.world.held_light(*h).cloned(),
            Some(Held::Worn(h, slot)) => game.world.party.get(*h).and_then(|hero| worn(hero, *slot)).map(|g| Item { name: gear_name(&g, false), gear: Some(g), count: 1, ..default() }),
            None => None,
        };
        if let Some(item) = item
            && let Some(image) = state.icons.get(&icon_key(&item))
        {
            let (w, h) = item.size();
            commands.entity(entity).with_children(|n| {
                n.spawn((Node { width: px(w as f32 * CELL as f32), height: px(h as f32 * CELL as f32), left: px(-(w as f32) * CELL as f32 / 2.0), top: px(-(h as f32) * CELL as f32 / 2.0), ..default() }, ImageNode::new(image.clone()), Pickable::IGNORE, FocusPolicy::Pass));
            });
        }
    }
    let Some(at) = cursor(&window) else { return };
    node.position_type = PositionType::Absolute;
    node.left = px(at.x);
    node.top = px(at.y);
}
