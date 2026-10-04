//! The party in the top right corner: a face, a name, a level, hit points and armour class for
//! every hero at the table, the player's own one framed in brass. Always there while playing,
//! so nobody has to open a sheet to see who is hurt.

use crate::coop::{Session, player_name};
use crate::i18n::{Lang, T};
use crate::menu::Screen;
use crate::play::{Game, Phase, my_hero};
use crate::store::Settings;
use crate::theme::*;
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Component)]
struct PartyHud;

pub struct PartyHudPlugin;

impl Plugin for PartyHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, build.run_if(in_state(Screen::Play))).add_systems(OnExit(Screen::Play), clear);
    }
}

fn clear(mut commands: Commands, huds: Query<Entity, With<PartyHud>>) {
    for hud in &huds {
        commands.entity(hud).despawn();
    }
}

#[allow(clippy::too_many_arguments)]
fn build(
    mut commands: Commands,
    game: Res<Game>,
    phase: Res<Phase>,
    session: Res<Session>,
    settings: Res<Settings>,
    lang: Res<Lang>,
    fonts: Res<Fonts>,
    huds: Query<Entity, With<PartyHud>>,
    mut images: ResMut<Assets<Image>>,
    mut faces: Local<HashMap<String, Handle<Image>>>,
    mut built: Local<String>,
) {
    let world = &game.world;
    let me = player_name(&settings);
    let mine = my_hero(world, &me, matches!(*session, Session::Solo));
    // In a fight the wounds are the fight's until it ends.
    let hp_of = |i: usize, hero: &rules::Character| world.fight.as_ref().and_then(|f| f.combatants.iter().find(|c| c.id == format!("hero_{i}"))).map_or(hero.hp, |c| c.hp);
    let acting = world.fight.as_ref().map(|f| f.actor().id.clone());
    let _ = &phase;
    let print = format!(
        "{mine:?}|{acting:?}|{}|{:?}|{:?}",
        world.tired(),
        world.party.iter().enumerate().map(|(i, h)| (&h.name, hp_of(i, h), h.max_hp, h.ac(), h.level, h.dead, h.look, &h.armor, h.shield, h.weapons.first())).collect::<Vec<_>>(),
        (&world.owners, &world.present, *lang, world.companions.iter().enumerate().map(|(n, c)| (&c.creature.name, world.fight.as_ref().and_then(|f| f.combatants.iter().find(|f| f.id == format!("ally_{n}"))).map_or(c.hp, |f| f.hp), c.max_hp)).collect::<Vec<_>>())
    );
    if *built == print {
        return;
    }
    *built = print;
    for hud in &huds {
        commands.entity(hud).despawn();
    }
    // A column down the right edge, part of the frame: the map is never under it.
    let column = Node {
        position_type: PositionType::Absolute,
        right: px(0),
        top: px(0),
        bottom: px(crate::play::HUD_HEIGHT),
        width: px(crate::view::COLUMN),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        row_gap: px(3),
        padding: UiRect::axes(px(2), px(3)),
        border: UiRect::left(px(1)),
        overflow: Overflow::clip(),
        ..default()
    };
    commands.spawn((PartyHud, column, BackgroundColor(color(WOOD_DEEP)), BorderColor::all(color(BRASS)), GlobalZIndex(8))).with_children(|column| {
        for (i, hero) in world.party.iter().enumerate() {
            let face = faces.entry(crate::doll::key(hero)).or_insert_with(|| images.add(crate::doll::frames(hero).swap_remove(0).into_image())).clone();
            let away = world.is_away(i);
            let own = Some(i) == mine;
            let hp = hp_of(i, hero);
            let turn = acting.as_deref() == Some(format!("hero_{i}").as_str());
            let dim = if hero.dead || away { 0.45 } else { 1.0 };
            let edge = if turn { BRASS_LIGHT } else if own { BRASS } else { WOOD_DARK };
            column
                .spawn((Node { width: percent(100), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(1), padding: UiRect::all(px(1)), border: UiRect::all(px(1)), ..default() }, BackgroundColor(Color::srgba(0.08, 0.06, 0.05, 0.9)), BorderColor::all(color(edge))))
                .with_children(|frame| {
                    // The head and shoulders of the hero's own figure.
                    frame.spawn((
                        Node { width: px(30), height: px(30), flex_shrink: 0.0, ..default() },
                        ImageNode { rect: Some(Rect::new(6.0, 2.0, 26.0, 22.0)), color: Color::srgba(1.0, 1.0, 1.0, dim), ..ImageNode::new(face) },
                        BackgroundColor(Color::srgba(0.25, 0.3, 0.2, 0.6)),
                    ));
                    let name: String = hero.name.chars().take(6).collect();
                    frame.spawn((Text::new(format!("{name} {}", hero.level)), fonts.body(), TextColor(color(if own { BRASS_LIGHT } else { PARCHMENT })), TextLayout::no_wrap()));
                    // Hit points as a bar, red when low, with the numbers under it.
                    let share = (hp.max(0) as f32 / hero.max_hp.max(1) as f32).clamp(0.0, 1.0);
                    let bar = if share > 0.5 { [0x5c, 0xb0, 0x4a, 0xff] } else if share > 0.25 { [0xe0, 0xb0, 0x3a, 0xff] } else { WAX };
                    frame.spawn((Node { width: px(36), height: px(4), flex_shrink: 0.0, ..default() }, BackgroundColor(color(INK)))).with_children(|back| {
                        back.spawn((Node { width: px((36.0 * share).round()), height: px(4), ..default() }, BackgroundColor(color(bar))));
                    });
                    let state = if hero.dead { T::HudFallen.text(*lang).to_string() } else if away { T::HudAway.text(*lang).to_string() } else { format!("{}/{}", hp.max(0), hero.max_hp) };
                    if world.tired() && !hero.dead {
                        frame.spawn((Text::new(T::Tired.text(*lang)), fonts.body(), TextColor(color(WAX)), TextLayout::no_wrap()));
                    }
                    frame.spawn((Text::new(state), fonts.body(), TextColor(color(TALLOW)), TextLayout::no_wrap()));
                    frame.spawn((Text::new(format!("{} {}", T::ArmorClass.text(*lang), hero.ac())), fonts.body(), TextColor(color(PARCHMENT_SHADE)), TextLayout::no_wrap()));
                });
        }
        // Those who travel with the party, smaller: a name and a bar.
        for (n, companion) in world.companions.iter().enumerate() {
            let fighter = world.fight.as_ref().and_then(|f| f.combatants.iter().find(|c| c.id == format!("ally_{n}")));
            let hp = fighter.map_or(companion.hp, |c| c.hp);
            let turn = acting.as_deref() == Some(format!("ally_{n}").as_str());
            column
                .spawn((Node { width: percent(100), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(1), padding: UiRect::all(px(1)), border: UiRect::all(px(1)), ..default() }, BackgroundColor(Color::srgba(0.08, 0.06, 0.05, 0.9)), BorderColor::all(color(if turn { BRASS_LIGHT } else { WOOD_DARK }))))
                .with_children(|frame| {
                    let name: String = companion.creature.name.chars().take(7).collect();
                    frame.spawn((Text::new(name), fonts.body(), TextColor(color(PARCHMENT)), TextLayout::no_wrap()));
                    let share = (hp.max(0) as f32 / companion.max_hp.max(1) as f32).clamp(0.0, 1.0);
                    frame.spawn((Node { width: px(36), height: px(3), flex_shrink: 0.0, ..default() }, BackgroundColor(color(INK)))).with_children(|back| {
                        back.spawn((Node { width: px((36.0 * share).round()), height: px(3), ..default() }, BackgroundColor(color([0x5c, 0xb0, 0x4a, 0xff]))));
                    });
                    frame.spawn((Text::new(format!("{}/{}", hp.max(0), companion.max_hp)), fonts.body(), TextColor(color(TALLOW)), TextLayout::no_wrap()));
                });
        }
    });
}
