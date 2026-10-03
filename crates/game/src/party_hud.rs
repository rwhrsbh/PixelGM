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
    // The bag covers the corner; the frames step aside while it is open.
    let hidden = matches!(*phase, Phase::Bag | Phase::Paused);
    let print = format!(
        "{hidden}|{mine:?}|{:?}|{:?}",
        world.party.iter().map(|h| (&h.name, h.hp, h.max_hp, h.ac(), h.level, h.dead, h.look, &h.armor, h.shield, h.weapons.first())).collect::<Vec<_>>(),
        (&world.owners, &world.present, *lang)
    );
    if *built == print {
        return;
    }
    *built = print;
    for hud in &huds {
        commands.entity(hud).despawn();
    }
    if hidden || world.party.is_empty() {
        return;
    }
    commands
        .spawn((PartyHud, Node { position_type: PositionType::Absolute, right: px(4), top: px(4), flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }, GlobalZIndex(8), Pickable::IGNORE))
        .with_children(|column| {
            for (i, hero) in world.party.iter().enumerate() {
                let face = faces.entry(crate::doll::key(hero)).or_insert_with(|| images.add(crate::doll::frames(hero).swap_remove(0).into_image())).clone();
                let away = world.is_away(i);
                let own = Some(i) == mine;
                let dim = if hero.dead || away { 0.45 } else { 1.0 };
                column
                    .spawn((
                        Node { width: px(150), padding: UiRect::all(px(2)), column_gap: px(3), border: UiRect::all(px(1)), align_items: AlignItems::Center, ..default() },
                        BackgroundColor(Color::srgba(0.08, 0.06, 0.05, 0.78)),
                        BorderColor::all(color(if own { BRASS } else { WOOD_DARK })),
                        Pickable::IGNORE,
                    ))
                    .with_children(|frame| {
                        // The head and shoulders of the hero's own figure.
                        frame.spawn((
                            Node { width: px(20), height: px(20), flex_shrink: 0.0, ..default() },
                            ImageNode { rect: Some(Rect::new(6.0, 2.0, 26.0, 22.0)), color: Color::srgba(1.0, 1.0, 1.0, dim), ..ImageNode::new(face) },
                            BackgroundColor(Color::srgba(0.25, 0.3, 0.2, 0.6)),
                        ));
                        frame.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(1), flex_grow: 1.0, ..default() }).with_children(|lines| {
                            let name: String = hero.name.chars().take(11).collect();
                            let state = if hero.dead { format!(" · {}", T::HudFallen.text(*lang)) } else if away { format!(" · {}", T::HudAway.text(*lang)) } else { String::new() };
                            lines.spawn((Text::new(format!("{name} {}{state}", hero.level)), fonts.body(), TextColor(color(if own { BRASS_LIGHT } else { PARCHMENT })), TextLayout::no_wrap()));
                            // Hit points as a bar, red when low, with the numbers beside.
                            lines.spawn(Node { column_gap: px(3), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                                let share = (hero.hp.max(0) as f32 / hero.max_hp.max(1) as f32).clamp(0.0, 1.0);
                                let bar = if share > 0.5 { [0x5c, 0xb0, 0x4a, 0xff] } else if share > 0.25 { [0xe0, 0xb0, 0x3a, 0xff] } else { WAX };
                                row.spawn((Node { width: px(40), height: px(5), flex_shrink: 0.0, ..default() }, BackgroundColor(color(WOOD_DEEP)))).with_children(|back| {
                                    back.spawn((Node { width: px((40.0 * share).round()), height: px(5), ..default() }, BackgroundColor(color(bar))));
                                });
                                row.spawn((Text::new(format!("{}/{} · {} {}", hero.hp.max(0), hero.max_hp, T::ArmorClass.text(*lang), hero.ac())), fonts.body(), TextColor(color(TALLOW)), TextLayout::no_wrap()));
                            });
                        });
                    });
            }
        });
}
