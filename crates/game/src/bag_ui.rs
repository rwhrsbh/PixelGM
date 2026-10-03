//! The bag screen: the party's bag as a grid of cells, the player's hero beside it with what
//! it wears in its slots. A thing is picked up with a click and put down with another; the
//! right button lets go of it. A guest's moves are shown at once and confirmed by the host.

use crate::art::Art;
use crate::audio::{Play, Sfx};
use crate::coop::{Intent, Session, player_name};
use crate::i18n::{Lang, T};
use crate::icons::{CELL, icon};
use crate::play::{Game, Phase, my_hero};
use crate::screen::{UI_PIXEL, fit};
use crate::store::Settings;
use crate::theme::*;
use crate::widgets::panel_backing;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::window::PrimaryWindow;
use std::collections::HashMap;
use world::bag::{BAG_H, BAG_W, Gear, Held, Slot, Spot, gear_kind, gear_name, gear_size, worn};
use world::{Item, World};

pub struct BagPlugin;

impl Plugin for BagPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BagState>().add_systems(Update, (open, build, press, hover, follow).chain().run_if(in_state(crate::menu::Screen::Play)));
    }
}

/// What the bag screen remembers between frames.
#[derive(Resource, Default)]
pub struct BagState {
    /// Which hero of the party is shown.
    hero: usize,
    held: Option<Held>,
    /// What the cursor is over, in words, for the box under the bag.
    info: Vec<(String, Rgba)>,
    /// What the screen was built from, to build it again when that changes.
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
    /// A thing that found no room in the bag, by its index.
    Loose(usize),
    PrevHero,
    NextHero,
}

#[derive(Component)]
struct HeldIcon;

#[derive(Component)]
struct InfoText;

/// The bag's corner and the slots' places on the panel, in pixels of the interface.
const PANEL: (f32, f32, f32, f32) = (10.0, 6.0, 460.0, 248.0);
const GRID: (f32, f32) = (250.0, 22.0);
const DOLL: (f32, f32) = (52.0, 50.0);

fn slot_box(slot: Slot) -> (f32, f32, u8, u8) {
    match slot {
        Slot::Main => (12.0, 50.0, 2, 4),
        Slot::Spare => (12.0, 118.0, 2, 4),
        Slot::Shield => (124.0, 50.0, 2, 2),
        Slot::Body => (124.0, 86.0, 2, 3),
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

/// The heroes whose slots this player may change: alone, the whole party; at a shared table,
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
        // Heroes from older saves carry every weapon they began with: the rest go in the bag.
        if !session.is_client() && game.world.party.iter().any(|h| h.weapons.len() > 2) {
            game.world.tidy_gear();
        }
        commands.spawn((BagRoot, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() }, GlobalZIndex(10)));
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

/// Everything about the world the screen shows, so that it is built again when any of it changes.
fn fingerprint(world: &World, state: &BagState, hero: Option<usize>, lang: Lang) -> String {
    let worn: Vec<_> = hero.and_then(|h| world.party.get(h)).map(|h| Slot::ALL.map(|s| worn(h, s))).into_iter().collect();
    format!("{:?}|{:?}|{:?}|{:?}|{:?}|{:?}", world.story.inventory, worn, hero, state.held, lang, hero.and_then(|h| world.party.get(h)).map(|h| (h.hp, h.max_hp, h.look)))
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
    let mut lines = vec![(format!("{}{count}", item_name(item, ru)), INK)];
    if let Some(index) = &item.gear {
        lines.extend(gear_lines(index, lang));
    }
    if !item.description.trim().is_empty() {
        lines.push((item.description.trim().to_string(), FADED));
    }
    lines
}

fn abs(left: f32, top: f32, width: f32, height: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: px(left), top: px(top), width: px(width), height: px(height), ..default() }
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
    let mut icon_of = |item: &Item, state: &mut BagState| -> Handle<Image> {
        let key = format!("{}|{:?}", item.name, item.gear);
        state.icons.entry(key).or_insert_with(|| images.add(icon(item).into_image())).clone()
    };
    let held_item = match &state.held {
        Some(Held::Item(i)) => Some(*i),
        _ => None,
    };
    let held_slot = match &state.held {
        Some(Held::Worn(_, slot)) => Some(*slot),
        _ => None,
    };
    let cell = CELL as f32;
    // Pictures prepared before the screen is put together.
    let placed: Vec<(usize, (u8, u8), (u8, u8), Handle<Image>)> =
        world.story.inventory.iter().enumerate().filter_map(|(i, item)| item.at.map(|at| (i, at, item.size(), icon_of(item, &mut state)))).collect();
    let hero = hero_index.and_then(|h| world.party.get(h));
    let worn_icons: Vec<(Slot, Handle<Image>)> = hero
        .map(|h| {
            Slot::ALL
                .iter()
                .filter_map(|slot| worn(h, *slot).map(|index| (*slot, icon_of(&Item { name: gear_name(&index, false), gear: Some(index), count: 1, ..default() }, &mut state))))
                .collect()
        })
        .unwrap_or_default();
    let doll = hero.map(|h| {
        let key = crate::doll::key(h);
        state.dolls.entry(key).or_insert_with(|| crate::doll::frames(h).into_iter().map(|f| images.add(f.into_image())).collect())[0].clone()
    });
    let loose: Vec<(usize, String)> = world.story.inventory.iter().enumerate().filter(|(_, i)| i.at.is_none()).map(|(n, i)| (n, item_name(i, ru))).collect();

    commands.entity(root).with_children(|root| {
        root.spawn((BagContent, abs(PANEL.0, PANEL.1, PANEL.2, PANEL.3))).with_children(|panel| {
            panel.spawn(panel_backing(&art));
            panel.spawn((abs(12.0, 8.0, 230.0, 12.0), Text::new(T::BagTitle.text(*lang)), fonts.title(1), TextColor(color(INK))));
            panel.spawn((abs(12.0, 226.0, 230.0, 10.0), Text::new(T::BagHint.text(*lang)), fonts.body(), TextColor(color(FADED)), TextLayout::no_wrap()));
            // The hero, with arrows when there is more than one to look after.
            if let Some(hero) = hero {
                let name = format!("{} · {} {} · {} {}/{}", hero.name, T::ArmorClass.text(*lang), hero.ac(), T::HitPoints.text(*lang), hero.hp, hero.max_hp);
                panel.spawn((abs(12.0, 26.0, 230.0, 10.0), Text::new(name), fonts.body(), TextColor(color(INK)), TextLayout::no_wrap()));
                if mine.len() > 1 {
                    for (left, target, mark) in [(170.0, Target::PrevHero, "<"), (186.0, Target::NextHero, ">")] {
                        panel.spawn((abs(left, 172.0, 14.0, 14.0), Button, target, BackgroundColor(color(PARCHMENT_DARK)))).with_children(|b| {
                            b.spawn((Text::new(mark), fonts.body(), TextColor(color(INK))));
                        });
                    }
                }
            }
            // The figure, at twice the interface's measure, between the slots.
            panel.spawn((abs(DOLL.0 - 4.0, DOLL.1 - 4.0, 72.0, 104.0), BackgroundColor(Color::srgba(0.2, 0.15, 0.1, 0.25))));
            if let Some(doll) = doll {
                panel.spawn((abs(DOLL.0, DOLL.1, 64.0, 96.0), ImageNode::new(doll), Pickable::IGNORE));
            }
            for slot in Slot::ALL {
                let (left, top, w, h) = slot_box(slot);
                let holds = worn_icons.iter().find(|(s, _)| *s == slot).map(|(_, image)| image.clone());
                panel
                    .spawn((Node { border: UiRect::all(px(1)), ..abs(left, top, w as f32 * cell, h as f32 * cell) }, Button, Target::Slot(slot), BackgroundColor(color(WOOD_DARK)), BorderColor::all(color(BRASS_DARK))))
                    .with_children(|slot_node| {
                        if let Some(image) = holds {
                            // A thing in the slot is drawn centred in it, dimmed while it is held.
                            let tint = if held_slot == Some(slot) { Color::srgba(1.0, 1.0, 1.0, 0.35) } else { Color::WHITE };
                            slot_node.spawn((
                                Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                                Pickable::IGNORE,
                                FocusPolicy::Pass,
                            ))
                            .with_children(|centre| {
                                centre.spawn((ImageNode { color: tint, ..ImageNode::new(image) }, Pickable::IGNORE, FocusPolicy::Pass));
                            });
                        }
                    });
            }
            // The grid of the bag, and the things on it.
            panel.spawn((abs(GRID.0 - 2.0, GRID.1 - 2.0, BAG_W as f32 * cell + 4.0, BAG_H as f32 * cell + 4.0), BackgroundColor(color(WOOD_DEEP))));
            for y in 0..BAG_H {
                for x in 0..BAG_W {
                    let shade = if (x + y) % 2 == 0 { WOOD_DARK } else { [0x3c, 0x24, 0x17, 0xff] };
                    panel.spawn((Node { border: UiRect::all(px(1)), ..abs(GRID.0 + x as f32 * cell, GRID.1 + y as f32 * cell, cell, cell) }, Button, Target::Cell(x, y), BackgroundColor(color(shade)), BorderColor::all(color(WOOD_DEEP))));
                }
            }
            for (i, (x, y), (w, h), image) in placed {
                let (left, top) = (GRID.0 + x as f32 * cell, GRID.1 + y as f32 * cell);
                let tint = if held_item == Some(i) { Color::srgba(1.0, 1.0, 1.0, 0.35) } else { Color::WHITE };
                // The cells a thing lies on are tinted, the way the old games did it.
                panel.spawn((Node { border: UiRect::all(px(1)), ..abs(left, top, w as f32 * cell, h as f32 * cell) }, BackgroundColor(Color::srgba(0.25, 0.32, 0.55, 0.45)), BorderColor::all(Color::srgba(0.45, 0.55, 0.85, 0.7)), Pickable::IGNORE, FocusPolicy::Pass));
                panel.spawn((abs(left, top, w as f32 * cell, h as f32 * cell), ImageNode { color: tint, ..ImageNode::new(image) }, Pickable::IGNORE, FocusPolicy::Pass));
            }
            // What the bag had no room for, to be picked up and put somewhere.
            if !loose.is_empty() {
                panel.spawn((abs(12.0, 196.0, 230.0, 10.0), Text::new(T::BagNoRoom.text(*lang)), fonts.body(), TextColor(color(WAX))));
                panel.spawn(Node { position_type: PositionType::Absolute, left: px(12.0), top: px(208.0), width: px(230.0), flex_wrap: FlexWrap::Wrap, column_gap: px(3), row_gap: px(2), ..default() }).with_children(|line| {
                    for (i, name) in &loose {
                        line.spawn((Node { padding: UiRect::axes(px(3), px(1)), ..default() }, Button, Target::Loose(*i), BackgroundColor(color(PARCHMENT_DARK)))).with_children(|b| {
                            b.spawn((Text::new(name.clone()), fonts.body(), TextColor(color(INK)), TextLayout::no_wrap()));
                        });
                    }
                });
            }
            // What the cursor is over.
            panel.spawn((InfoText, Node { position_type: PositionType::Absolute, left: px(GRID.0), top: px(GRID.1 + BAG_H as f32 * cell + 6.0), width: px(BAG_W as f32 * cell), flex_direction: FlexDirection::Column, row_gap: px(1), ..default() }));
        });
        // The thing held, under the cursor.
        root.spawn((BagContent, HeldIcon, Node { position_type: PositionType::Absolute, ..default() }, Pickable::IGNORE, FocusPolicy::Pass, GlobalZIndex(12)));
    });
    state.info.clear();
}

/// Puts down or picks up what was clicked.
#[allow(clippy::too_many_arguments)]
fn press(
    phase: Res<Phase>,
    targets: Query<(&Target, &Interaction), Changed<Interaction>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut game: ResMut<Game>,
    session: Res<Session>,
    settings: Res<Settings>,
    mut state: ResMut<BagState>,
    mut sound: MessageWriter<Play>,
) {
    if *phase != Phase::Bag {
        return;
    }
    if mouse.just_pressed(MouseButton::Right) && state.held.take().is_some() {
        state.built.clear();
        sound.write(Play(Sfx::Back));
        return;
    }
    let mine = my_heroes(&game.world, &session, &settings);
    for (target, interaction) in &targets {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let hero = state.hero;
        let world = &mut game.world;
        let to = match (*target, &state.held) {
            (Target::PrevHero | Target::NextHero, _) => {
                if let Some(at) = mine.iter().position(|h| *h == hero) {
                    let step = if matches!(target, Target::NextHero) { 1 } else { mine.len() - 1 };
                    state.hero = mine[(at + step) % mine.len()];
                }
                sound.write(Play(Sfx::Page));
                continue;
            }
            (Target::Cell(x, y), Some(held)) => {
                // The thing goes where the cursor is its middle, kept inside the bag.
                let size = match held {
                    Held::Item(i) => world.story.inventory.get(*i).map_or((1, 1), Item::size),
                    Held::Worn(h, slot) => world.party.get(*h).and_then(|hero| worn(hero, *slot)).map_or((1, 1), |g| gear_size(&g)),
                };
                let left = (x as i32 - (size.0 as i32 - 1) / 2).clamp(0, (BAG_W - size.0.min(BAG_W)) as i32) as u8;
                let top = (y as i32 - (size.1 as i32 - 1) / 2).clamp(0, (BAG_H - size.1.min(BAG_H)) as i32) as u8;
                Spot::Bag(left, top)
            }
            (Target::Slot(slot), Some(_)) => Spot::Worn(hero, slot),
            (Target::Cell(x, y), None) => {
                if let Some(i) = world.story.item_at((x, y)) {
                    state.held = Some(Held::Item(i));
                    sound.write(Play(Sfx::Item));
                }
                continue;
            }
            (Target::Slot(slot), None) => {
                if world.party.get(hero).and_then(|h| worn(h, slot)).is_some() {
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
        // A guest's move is shown at once; the host's world confirms it.
        let done = world.arrange(&held, to);
        info!("bag: {held:?} to {to:?}: {}", if done { "done" } else { "refused" });
        if done {
            if !matches!(*session, Session::Solo | Session::Host(_)) {
                session.intend(Intent::Arrange { held, to });
            }
            state.held = None;
            sound.write(Play(Sfx::Confirm));
        } else {
            sound.write(Play(Sfx::Error));
        }
    }
}

/// The box under the bag tells what the cursor is over.
fn hover(
    phase: Res<Phase>,
    targets: Query<(&Target, &Interaction)>,
    game: Res<Game>,
    lang: Res<Lang>,
    fonts: Res<Fonts>,
    mut state: ResMut<BagState>,
    info: Query<Entity, With<InfoText>>,
    mut commands: Commands,
) {
    if *phase != Phase::Bag {
        return;
    }
    let world = &game.world;
    let mut lines: Vec<(String, Rgba)> = Vec::new();
    for (target, interaction) in &targets {
        if *interaction == Interaction::None {
            continue;
        }
        match *target {
            Target::Cell(x, y) => {
                if let Some(item) = world.story.item_at((x, y)).and_then(|i| world.story.inventory.get(i)) {
                    lines = item_lines(item, *lang);
                }
            }
            Target::Loose(i) => {
                if let Some(item) = world.story.inventory.get(i) {
                    lines = item_lines(item, *lang);
                }
            }
            Target::Slot(slot) => {
                lines.push((slot_label(slot).text(*lang).to_string(), FADED));
                if let Some(index) = world.party.get(state.hero).and_then(|h| worn(h, slot)) {
                    lines.push((gear_name(&index, *lang == Lang::Ru), INK));
                    lines.extend(gear_lines(&index, *lang));
                } else {
                    lines.push((T::BagEmptySlot.text(*lang).to_string(), FADED));
                }
            }
            _ => {}
        }
    }
    if lines == state.info {
        return;
    }
    state.info = lines.clone();
    for entity in &info {
        commands.entity(entity).despawn_children().with_children(|box_| {
            for (text, tint) in &lines {
                box_.spawn((Text::new(text.clone()), fonts.body(), TextColor(color(*tint))));
            }
        });
    }
}

/// The held thing follows the cursor.
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
    // The screen is built again after each move, and the icon with it.
    if *shown != Some((entity, state.held.clone())) {
        *shown = Some((entity, state.held.clone()));
        commands.entity(entity).despawn_children();
        let item = match &state.held {
            Some(Held::Item(i)) => game.world.story.inventory.get(*i).cloned(),
            Some(Held::Worn(h, slot)) => game.world.party.get(*h).and_then(|hero| worn(hero, *slot)).map(|g| Item { name: gear_name(&g, false), gear: Some(g), count: 1, ..default() }),
            None => None,
        };
        if let Some(item) = item
            && let Some(image) = state.icons.get(&format!("{}|{:?}", item.name, item.gear))
        {
            let (w, h) = item.size();
            commands.entity(entity).with_children(|n| {
                n.spawn((Node { width: px(w as f32 * CELL as f32), height: px(h as f32 * CELL as f32), left: px(-(w as f32) * CELL as f32 / 2.0), top: px(-(h as f32) * CELL as f32 / 2.0), ..default() }, ImageNode::new(image.clone()), Pickable::IGNORE, FocusPolicy::Pass));
            });
        }
    }
    let Some(cursor) = window.cursor_position() else { return };
    let (scale, origin, _) = fit(window.physical_size());
    let at = (cursor * window.scale_factor() - origin.as_vec2()) / (scale * UI_PIXEL);
    node.position_type = PositionType::Absolute;
    node.left = px(at.x);
    node.top = px(at.y);
}

