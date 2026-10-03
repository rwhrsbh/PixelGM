//! Movements the master asks for — a jump, a crouch, a blow, a fall, a dance — played on the
//! figures they belong to. Each is numbered in the world, so every player at the table sees
//! it once, wherever their screen is.

use crate::coop::{Session, player_name};
use crate::fx::Actor;
use crate::play::{Fellow, Game, Hero, my_hero};
use crate::store::Settings;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Motion {
    Jump,
    Crouch,
    Sit,
    Lie,
    Shake,
    Spin,
    Strike,
    Dance,
    Wave,
    Bow,
    Stagger,
}

impl Motion {
    /// The movement a word stands for, in either language.
    pub fn of(word: &str) -> Option<Self> {
        let w = word.trim().to_lowercase();
        let has = |list: &[&str]| list.iter().any(|x| w.contains(x));
        Some(if has(&["stagger", "slip", "stumble", "поскольз", "споткн", "шата", "оступ"]) {
            Motion::Stagger
        } else if has(&["jump", "leap", "hop", "прыг", "скач"]) {
            Motion::Jump
        } else if has(&["crouch", "squat", "kneel", "duck", "cower", "присе", "корточ", "пригну", "колен", "сжал"]) {
            Motion::Crouch
        } else if has(&["lie", "fall", "faint", "collapse", "прилег", "лёг", "упал", "пада", "рухн", "обмяк"]) {
            Motion::Lie
        } else if has(&["sit", "садит", "сел ", "сидит"]) {
            Motion::Sit
        } else if has(&["shake", "tremble", "shiver", "дрож", "тряс", "трепещ"]) {
            Motion::Shake
        } else if has(&["spin", "whirl", "кружи", "поверну", "разверн", "крут"]) {
            Motion::Spin
        } else if has(&["strike", "attack", "hit", "slash", "swing", "punch", "kick", "lunge", "удар", "атак", "бьёт", "бьет", "пина", "пнул", "замах", "выпад"]) {
            Motion::Strike
        } else if has(&["dance", "танц", "пляс"]) {
            Motion::Dance
        } else if has(&["wave", "nod", "gesture", "маш", "кива", "жест"]) {
            Motion::Wave
        } else if has(&["bow", "поклон", "кланя"]) {
            Motion::Bow
        } else {
            return None;
        })
    }

    fn seconds(self) -> f32 {
        match self {
            Motion::Jump => 0.7,
            Motion::Strike => 0.5,
            Motion::Shake => 0.8,
            Motion::Spin => 0.8,
            Motion::Wave | Motion::Bow => 1.2,
            Motion::Stagger => 1.0,
            Motion::Crouch => 1.6,
            Motion::Dance => 2.4,
            Motion::Sit | Motion::Lie => 3.5,
        }
    }

    /// Where the figure is and how it is turned at `t` seconds in: an offset in canvas
    /// pixels, a squash, a lean, whether it faces the other way.
    fn pose(self, t: f32) -> (Vec2, Vec2, f32, bool) {
        let s = (t / self.seconds()).clamp(0.0, 1.0);
        let arc = (s * std::f32::consts::PI).sin();
        // Down quickly, held, and up again at the end.
        let held = (s * 6.0).min(1.0).min((1.0 - s) * 6.0);
        let step = |v: f32| (v * 4.0).round() / 4.0;
        match self {
            Motion::Jump => (Vec2::new(0.0, (arc * 26.0).round()), Vec2::new(1.0 - arc * 0.08, 1.0 + arc * 0.1), 0.0, false),
            Motion::Crouch => (Vec2::ZERO, Vec2::new(1.0 + held * 0.1, 1.0 - step(held) * 0.3), 0.0, false),
            Motion::Sit => (Vec2::ZERO, Vec2::new(1.0, 1.0 - step(held) * 0.22), 0.0, false),
            Motion::Lie => (Vec2::new(0.0, -2.0 * held), Vec2::ONE, step(held) * 1.5708, false),
            Motion::Shake => (Vec2::new(((t * 50.0).sin() * 3.0 * arc).round(), 0.0), Vec2::ONE, 0.0, false),
            Motion::Spin => (Vec2::ZERO, Vec2::ONE, 0.0, (s * 6.0) as i32 % 2 == 1),
            Motion::Strike => (Vec2::new((arc * 14.0).round(), 0.0), Vec2::ONE, -arc * 0.15, false),
            Motion::Dance => (Vec2::new(0.0, ((t * 9.0).sin().abs() * 8.0 * arc).round()), Vec2::ONE, (t * 4.5).sin() * 0.12 * arc, (t * 2.5) as i32 % 2 == 1),
            Motion::Wave => (Vec2::ZERO, Vec2::ONE, (t * 10.0).sin() * 0.1 * arc, false),
            Motion::Bow => (Vec2::ZERO, Vec2::new(1.0, 1.0 - arc * 0.12), arc * 0.35, false),
            Motion::Stagger => (Vec2::new((arc * 8.0 * (t * 12.0).sin()).round(), 0.0), Vec2::ONE, (t * 12.0).sin() * 0.3 * arc, false),
        }
    }
}

/// A movement being played on a figure.
#[derive(Component)]
pub struct Moving {
    motion: Motion,
    age: f32,
}

pub struct MotionPlugin;

impl Plugin for MotionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, start).add_systems(PostUpdate, perform.before(TransformSystems::Propagate));
    }
}

/// Plays the movements the world has not shown here yet.
#[allow(clippy::too_many_arguments)]
fn start(
    mut commands: Commands,
    game: Option<Res<Game>>,
    session: Res<Session>,
    settings: Res<Settings>,
    heroes: Query<Entity, With<Hero>>,
    fellows: Query<(Entity, &Fellow)>,
    actors: Query<(Entity, &Actor)>,
    mut last: Local<Option<u64>>,
    mut fx: ResMut<crate::fx::Fx>,
) {
    let Some(game) = game else { return };
    let world = &game.world;
    // What happened before this screen opened is not played again.
    let seen = last.get_or_insert(world.cue_seq);
    if world.cue_seq < *seen {
        *seen = world.cue_seq;
    }
    let me = player_name(&settings);
    let solo = matches!(*session, Session::Solo);
    let mine = my_hero(world, &me, solo);
    for cue in world.cues.iter().filter(|c| c.seq > *seen) {
        if cue.place != world.current {
            continue;
        }
        // The scene itself shaking is the camera's business.
        if let Some(seconds) = cue.motion.strip_prefix("quake ").and_then(|s| s.parse::<f32>().ok()) {
            fx.shake(seconds);
            continue;
        }
        let Some(motion) = Motion::of(&cue.motion) else { continue };
        let id = cue.id.to_lowercase();
        let named = |name: &str| name.to_lowercase().replace(' ', "_") == id || name.to_lowercase() == id;
        let all = matches!(id.as_str(), "party" | "hero" | "heroes" | "all" | "everyone" | "group");
        let mut targets: Vec<Entity> = Vec::new();
        for (i, hero) in world.party.iter().enumerate() {
            if !(all || named(&hero.name)) {
                continue;
            }
            let owner = world.owners.get(i).cloned().unwrap_or_default();
            if Some(i) == mine {
                targets.extend(heroes.iter());
            } else {
                targets.extend(fellows.iter().filter(|(_, f)| f.0 == owner).map(|(e, _)| e));
            }
            targets.extend(actors.iter().filter(|(_, a)| a.id == format!("hero_{i}")).map(|(e, _)| e));
        }
        targets.extend(actors.iter().filter(|(_, a)| a.id == id).map(|(e, _)| e));
        for entity in targets {
            commands.entity(entity).try_insert(Moving { motion, age: 0.0 });
        }
    }
    *seen = world.cue_seq;
}

/// Moves the figures, after everything else has put them where they stand.
fn perform(time: Res<Time>, mut commands: Commands, mut figures: Query<(Entity, &mut Moving, &mut Transform)>) {
    for (entity, mut moving, mut transform) in &mut figures {
        moving.age += time.delta_secs();
        if moving.age >= moving.motion.seconds() {
            transform.scale = Vec3::ONE;
            transform.rotation = Quat::IDENTITY;
            commands.entity(entity).try_remove::<Moving>();
            continue;
        }
        let (offset, squash, lean, flip) = moving.motion.pose(moving.age);
        transform.translation.x += offset.x;
        transform.translation.y += offset.y;
        transform.scale = Vec3::new(squash.x * if flip { -1.0 } else { 1.0 }, squash.y, 1.0);
        transform.rotation = Quat::from_rotation_z(lean);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_masters_words_for_movements_are_read_in_either_language() {
        assert_eq!(Motion::of("Jump"), Some(Motion::Jump));
        assert_eq!(Motion::of("присел на корточки"), Some(Motion::Crouch));
        assert_eq!(Motion::of("поскользнулся"), Some(Motion::Stagger));
        assert_eq!(Motion::of("strike"), Some(Motion::Strike));
        assert_eq!(Motion::of("ponders"), None);
    }

    #[test]
    fn every_movement_ends_where_it_began() {
        for m in [Motion::Jump, Motion::Crouch, Motion::Sit, Motion::Lie, Motion::Shake, Motion::Strike, Motion::Wave, Motion::Bow, Motion::Stagger, Motion::Dance] {
            let (offset, squash, lean, _) = m.pose(m.seconds());
            assert!(offset.length() < 3.5 && (squash - Vec2::ONE).length() < 0.05 && lean.abs() < 0.05, "{m:?} ends at {offset} {squash} {lean}");
        }
    }
}
