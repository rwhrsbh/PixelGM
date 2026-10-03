//! Motion. Nothing here decides anything: it only makes what happened visible — a lunge
//! and a flash for a blow, numbers that float off the wounded, dust under walking feet, a
//! die that tumbles before it shows its face, sheets that slide in, text that is written
//! out. All of it moves in whole pixels and a few flat steps, like the art it moves.

use crate::art::Pixels;
use crate::menu::Screen;
use crate::play::{Game, HeroPose, Phase, on_screen};
use crate::screen::MainCamera;
use crate::theme::*;
use bevy::prelude::*;
use world::TILE;
use world::walk::FEET;

/// A sprite of a creature or a hero, which blows and wounds can move and tint.
#[derive(Component)]
pub struct Actor {
    /// The id of the creature, or `hero_N` in a fight.
    pub id: String,
    /// Where it stands when nothing is moving it.
    pub home: Vec3,
    /// Its colour when nothing is tinting it.
    pub tint: Color,
    /// A standing creature shifts its weight; this sets it apart from its neighbours.
    pub bob: Option<f32>,
}

struct Lunge {
    who: String,
    by: Vec2,
    age: f32,
}

struct Flash {
    who: String,
    tint: Color,
    age: f32,
}

/// A die in the air: how long it has been, and the face it lands on.
struct Throw {
    age: f32,
    value: u32,
}

#[derive(Resource, Default)]
pub struct Fx {
    lunges: Vec<Lunge>,
    flashes: Vec<Flash>,
    shake: f32,
    wince: f32,
    floats: Vec<(Vec2, String, Color)>,
    bursts: Vec<(Vec2, Color, u32)>,
    /// The text being written out, and how many letters of it are there.
    typed: (String, f32),
    throw: Option<Throw>,
    seed: u32,
    /// Where each actor was last shown, so that a move is walked rather than jumped.
    shown: std::collections::HashMap<String, Vec3>,
    /// Sounds asked for by code that cannot write a message itself.
    pub cues: Vec<crate::audio::Sfx>,
}

const LUNGE_SECONDS: f32 = 0.22;
const LUNGE_PIXELS: f32 = 10.0;
/// How fast a creature or hero walks from one cell to the next in a fight, in canvas pixels a second.
const GLIDE_SPEED: f32 = 190.0;
const FLASH_SECONDS: f32 = 0.16;
const FLOAT_SECONDS: f32 = 0.9;
const WINCE_SECONDS: f32 = 0.3;
const LETTERS_PER_SECOND: f32 = 110.0;
const DIE_TUMBLE: f32 = 0.6;
const DIE_SHOWN: f32 = 1.5;

/// The centre of a cell of the map, on screen.
pub fn cell_centre(cell: (i32, i32)) -> Vec2 {
    on_screen((cell.0 as f32 + 0.5) * TILE as f32, (cell.1 as f32 + 0.5) * TILE as f32)
}

impl Fx {
    fn random(&mut self) -> f32 {
        // A small generator of its own: what dust does is nobody's business but its own.
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }

    /// `who` leans into a blow towards a point and back.
    pub fn lunge(&mut self, who: &str, from: Vec2, to: Vec2) {
        let by = (to - from).normalize_or_zero() * LUNGE_PIXELS;
        self.lunges.push(Lunge { who: who.to_string(), by, age: 0.0 });
    }

    /// `who` flashes a colour for a moment.
    pub fn flash(&mut self, who: &str, tint: Color) {
        self.flashes.push(Flash { who: who.to_string(), tint, age: 0.0 });
    }

    /// Words that rise from a point and fade: a wound, a miss, hit points restored.
    pub fn float(&mut self, at: Vec2, text: impl Into<String>, tint: Color) {
        self.floats.push((at, text.into(), tint));
    }

    /// Specks thrown out from a point.
    pub fn burst(&mut self, at: Vec2, tint: Color, count: u32) {
        self.bursts.push((at, tint, count));
    }

    pub fn shake(&mut self, seconds: f32) {
        self.shake = self.shake.max(seconds);
    }

    /// The screen reddens for a moment: a hero was hurt.
    pub fn wince(&mut self) {
        self.wince = WINCE_SECONDS;
    }

    /// A die is thrown and lands on `value`.
    pub fn throw(&mut self, value: u32) {
        self.throw = Some(Throw { age: 0.0, value });
    }
}

/// Up and back down over the life of a lunge.
fn there_and_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.4 { t / 0.4 } else { 1.0 - (t - 0.4) / 0.6 }
}

fn actors(time: Res<Time>, mut fx: ResMut<Fx>, mut actors: Query<(&Actor, &mut Transform, &mut Sprite)>) {
    let dt = time.delta_secs();
    for lunge in &mut fx.lunges {
        lunge.age += dt;
    }
    for flash in &mut fx.flashes {
        flash.age += dt;
    }
    fx.lunges.retain(|l| l.age < LUNGE_SECONDS);
    fx.flashes.retain(|f| f.age < FLASH_SECONDS);
    let mut seen = Vec::new();
    for (actor, mut transform, mut sprite) in &mut actors {
        // From where it was shown towards where it now stands; a far jump is not walked.
        let before = fx.shown.get(&actor.id).copied().unwrap_or(actor.home);
        let way = actor.home.truncate() - before.truncate();
        // A long way is walked faster, so that a creature crossing the place arrives in a couple of seconds.
        let speed = GLIDE_SPEED.max(way.length() * 1.2);
        let walked = if way.length() > 1100.0 || way.length() < 0.5 { actor.home.truncate() } else { before.truncate() + way.clamp_length_max(speed * dt) };
        let walking = (walked - actor.home.truncate()).length() > 0.5;
        fx.shown.insert(actor.id.clone(), walked.extend(actor.home.z));
        seen.push(actor.id.clone());
        let mut at = walked.round().extend(actor.home.z);
        // A walker bounces with its steps.
        if walking && (time.elapsed_secs() * 10.0) as i32 % 2 == 0 {
            at.y += 2.0;
        }
        if let Some(phase) = actor.bob
            && !walking
            && (time.elapsed_secs() + phase) % 1.4 >= 0.7
        {
            at.y += 2.0;
        }
        if let Some(lunge) = fx.lunges.iter().find(|l| l.who == actor.id) {
            let by = (lunge.by * there_and_back(lunge.age / LUNGE_SECONDS)).round();
            at.x += by.x;
            at.y += by.y;
        }
        if transform.translation != at {
            transform.translation = at;
        }
        // A blow lands as two flat beats of colour, not a fade.
        let tint = fx.flashes.iter().find(|f| f.who == actor.id).map_or(actor.tint, |f| if f.age < FLASH_SECONDS / 2.0 { Color::WHITE.mix(&f.tint, 0.2) } else { f.tint });
        if sprite.color != tint {
            sprite.color = tint;
        }
    }
    // Those no longer drawn are forgotten, so that coming back is not a walk across the map.
    if !seen.is_empty() {
        fx.shown.retain(|id, _| seen.contains(id));
    }
}

#[derive(Component)]
struct Float {
    age: f32,
    from: Vec2,
}

#[derive(Component)]
struct Speck {
    age: f32,
    life: f32,
    velocity: Vec2,
    at: Vec2,
}

fn spawn(mut commands: Commands, mut fx: ResMut<Fx>, fonts: Res<Fonts>) {
    for (at, text, tint) in std::mem::take(&mut fx.floats) {
        commands.spawn((DespawnOnExit(Screen::Play), Float { age: 0.0, from: at }, Text2d::new(text), fonts.body(), TextColor(tint), Transform::from_translation(at.extend(1.9)).with_scale(Vec3::splat(2.0))));
    }
    for (at, tint, count) in std::mem::take(&mut fx.bursts) {
        for _ in 0..count {
            let angle = fx.random() * std::f32::consts::TAU;
            let speed = 28.0 + fx.random() * 60.0;
            let life = 0.25 + fx.random() * 0.35;
            let size = if fx.random() < 0.3 { 4.0 } else { 2.0 };
            commands.spawn((
                DespawnOnExit(Screen::Play),
                Speck { age: 0.0, life, velocity: Vec2::new(angle.cos(), angle.sin().abs() * 0.8 + 0.2) * speed, at },
                Sprite { color: tint, custom_size: Some(Vec2::splat(size)), ..default() },
                Transform::from_translation(at.extend(1.85)),
            ));
        }
    }
}

fn floats(mut commands: Commands, time: Res<Time>, mut floats: Query<(Entity, &mut Float, &mut Transform, &mut TextColor)>) {
    for (entity, mut float, mut transform, mut tint) in &mut floats {
        float.age += time.delta_secs();
        let t = float.age / FLOAT_SECONDS;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        // Quick at first, then hanging: the number is read on the way up.
        let rise = (1.0 - (1.0 - t) * (1.0 - t)) * 24.0;
        transform.translation.x = float.from.x.round();
        transform.translation.y = (float.from.y + rise).round();
        let alpha = if t < 0.6 { 1.0 } else if t < 0.8 { 0.66 } else { 0.33 };
        if tint.0.alpha() != alpha {
            tint.0.set_alpha(alpha);
        }
    }
}

fn specks(mut commands: Commands, time: Res<Time>, mut specks: Query<(Entity, &mut Speck, &mut Transform)>) {
    let dt = time.delta_secs();
    for (entity, mut speck, mut transform) in &mut specks {
        speck.age += dt;
        if speck.age >= speck.life {
            commands.entity(entity).despawn();
            continue;
        }
        speck.velocity.y -= 140.0 * dt;
        let step = speck.velocity * dt;
        speck.at += step;
        transform.translation.x = speck.at.x.round();
        transform.translation.y = speck.at.y.round();
    }
}

fn shake(time: Res<Time>, mut fx: ResMut<Fx>, mut camera: Single<&mut Transform, With<MainCamera>>) {
    let offset = if fx.shake > 0.0 {
        fx.shake -= time.delta_secs();
        Vec2::new(((fx.random() * 3.0).floor() - 1.0) * 2.0, ((fx.random() * 3.0).floor() - 1.0) * 2.0)
    } else {
        Vec2::ZERO
    };
    if camera.translation.truncate() != offset {
        camera.translation.x = offset.x;
        camera.translation.y = offset.y;
    }
}

#[derive(Component)]
struct Wince;

fn spawn_wince(mut commands: Commands) {
    commands.spawn((
        Wince,
        Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), ..default() },
        BackgroundColor(Color::NONE),
        GlobalZIndex(4),
        Pickable::IGNORE,
    ));
}

fn wince(time: Res<Time>, mut fx: ResMut<Fx>, mut veils: Query<&mut BackgroundColor, With<Wince>>) {
    fx.wince = (fx.wince - time.delta_secs()).max(0.0);
    // Three flat steps down to nothing.
    let alpha = ((fx.wince / WINCE_SECONDS) * 3.0).ceil() / 3.0 * 0.3;
    for mut veil in &mut veils {
        if veil.0.alpha() != alpha {
            veil.0 = Color::srgba(0.7, 0.08, 0.06, alpha);
        }
    }
}

/// Dust under the feet of whoever is walking.
fn dust(time: Res<Time>, pose: Option<Res<HeroPose>>, phase: Option<Res<Phase>>, game: Option<Res<Game>>, mut fx: ResMut<Fx>, mut since: Local<f32>) {
    let (Some(pose), Some(phase), Some(game)) = (pose, phase, game) else { return };
    if !pose.moving || *phase != Phase::Walking {
        return;
    }
    *since += time.delta_secs();
    if *since >= 0.22 {
        *since = 0.0;
        let player = game.world.player;
        fx.burst(on_screen(player.x, player.y + FEET.1 - 1.0), Color::srgba(0.85, 0.8, 0.65, 0.8), 2);
    }
}

/// Text that is written out letter by letter. A press of anything writes the rest at once.
#[derive(Component)]
pub struct Typed(pub String);

fn typewriter(time: Res<Time>, keyboard: Res<ButtonInput<KeyCode>>, mouse: Res<ButtonInput<MouseButton>>, mut fx: ResMut<Fx>, mut texts: Query<(&Typed, &mut Text)>, fresh: Query<(), Added<Typed>>) {
    for (typed, mut text) in &mut texts {
        if fx.typed.0 != typed.0 {
            // A press that opened the sheet must not also finish its text.
            fx.typed = (typed.0.clone(), 0.0);
        } else if fresh.is_empty() && (keyboard.get_just_pressed().next().is_some() || mouse.get_just_pressed().next().is_some()) {
            fx.typed.1 = f32::MAX;
        }
        fx.typed.1 += LETTERS_PER_SECOND * time.delta_secs();
        let letters = fx.typed.1 as usize;
        let shown: String = typed.0.chars().take(letters).collect();
        if text.0 != shown {
            text.0 = shown;
        }
    }
}

/// A sheet that has just appeared slides up into place.
#[derive(Component, Default)]
pub struct PopIn(pub f32);

fn pop_in(time: Res<Time>, mut sheets: Query<(&mut PopIn, &mut Node)>) {
    for (mut pop, mut node) in &mut sheets {
        if pop.0 >= 0.12 && node.top == px(0) {
            continue;
        }
        pop.0 += time.delta_secs();
        let offset = match pop.0 {
            t if t < 0.04 => 6,
            t if t < 0.08 => 3,
            t if t < 0.12 => 1,
            _ => 0,
        };
        if node.top != px(offset) {
            node.top = px(offset);
        }
    }
}

#[derive(Component)]
struct DieView;

#[derive(Component)]
struct DieFace;

/// A twenty-sided die seen from above: a brass hexagon with its edges picked out.
fn die_picture() -> Pixels {
    const SIZE: i32 = 36;
    let mut picture = Pixels::new(SIZE as u32, SIZE as u32, CLEAR);
    for y in 0..SIZE {
        // A hexagon standing on a point: the sides slope in over the top and bottom thirds.
        let from_edge = y.min(SIZE - 1 - y);
        let inset = (SIZE / 2 - 2 - from_edge * 2).max(1);
        for x in inset..SIZE - inset {
            let edge = x == inset || x == SIZE - inset - 1 || from_edge == 0;
            let lit = y < SIZE / 2 && x < SIZE / 2;
            picture.set(x, y, if edge { INK } else if lit { BRASS_LIGHT } else { BRASS });
        }
    }
    // The facets of the upper face.
    for i in 0..SIZE / 2 - 4 {
        picture.set(SIZE / 2 - 1 - i / 2, 4 + i, BRASS_DARK);
        picture.set(SIZE / 2 + i / 2, 4 + i, BRASS_DARK);
    }
    picture
}

fn die(mut commands: Commands, time: Res<Time>, mut fx: ResMut<Fx>, mut images: ResMut<Assets<Image>>, fonts: Res<Fonts>, views: Query<Entity, With<DieView>>, mut faces: Query<&mut Text, With<DieFace>>, mut nodes: Query<&mut Node, With<DieFace>>) {
    let Some(throw) = &mut fx.throw else {
        for view in &views {
            commands.entity(view).despawn();
        }
        return;
    };
    if views.is_empty() {
        let picture = images.add(die_picture().into_image());
        commands
            .spawn((DieView, Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, GlobalZIndex(30), Pickable::IGNORE))
            .with_children(|root| {
                root.spawn((DieFace, Node { width: px(36), height: px(36), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, ImageNode::new(picture))).with_children(|face| {
                    face.spawn((DieFace, Text::default(), fonts.title(1), TextColor(color(INK))));
                });
            });
        return;
    }
    throw.age += time.delta_secs();
    let age = throw.age;
    let value = throw.value;
    if age >= DIE_SHOWN {
        fx.throw = None;
        return;
    }
    // In the air the faces flick past; landed, the one that counts stays.
    let shown = if age < DIE_TUMBLE { 1 + ((age * 22.0) as u32 * 7 + 3) % 20 } else { value };
    for mut face in &mut faces {
        let text = shown.to_string();
        if face.0 != text {
            face.0 = text;
        }
    }
    // Two bounces, each lower than the last, then still.
    let lift = if age < DIE_TUMBLE {
        let t = age / DIE_TUMBLE;
        let hop = if t < 0.6 { (t / 0.6 * std::f32::consts::PI).sin() * 26.0 } else { ((t - 0.6) / 0.4 * std::f32::consts::PI).sin() * 9.0 };
        hop.round() as i32
    } else {
        0
    };
    for mut node in &mut nodes {
        if node.width == px(36) && node.bottom != px(lift) {
            node.bottom = px(lift);
        }
    }
}

/// A thing thrown out of the bag onto the ground, to be seen flying there.
#[derive(Message, Clone)]
pub struct Thrown {
    pub item: world::Item,
    pub cell: (usize, usize),
}

#[derive(Component)]
struct Flying {
    age: f32,
    from: Vec2,
    to: Vec2,
}

fn throw_things(mut commands: Commands, mut thrown: MessageReader<Thrown>, mut images: ResMut<Assets<Image>>, game: Option<Res<Game>>, time: Res<Time>, mut flying: Query<(Entity, &mut Flying, &mut Transform)>) {
    for t in thrown.read() {
        let Some(game) = game.as_ref() else { continue };
        let tile = world::TILE as f32;
        let from = on_screen(game.world.player.x + 4.0, game.world.player.y - 8.0);
        let to = on_screen((t.cell.0 as f32 + 0.5) * tile + 6.0, (t.cell.1 as f32 + 0.8) * tile);
        let image = images.add(crate::icons::icon(&t.item).into_image());
        commands.spawn((DespawnOnExit(Screen::Play), Flying { age: 0.0, from, to }, Sprite::from_image(image), Transform::from_translation(from.extend(1.9))));
    }
    for (entity, mut fly, mut transform) in &mut flying {
        fly.age += time.delta_secs();
        let s = (fly.age / 0.45).min(1.0);
        let at = fly.from.lerp(fly.to, s) + Vec2::Y * (s * std::f32::consts::PI).sin() * 36.0;
        transform.translation = at.round().extend(1.9);
        transform.rotation = Quat::from_rotation_z(-s * 6.0);
        if s >= 1.0 {
            commands.entity(entity).despawn();
        }
    }
}

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Fx>().add_message::<Thrown>().add_systems(Update, throw_things).add_systems(Startup, spawn_wince).add_systems(Update, (dust, spawn, actors, floats, specks, shake, wince, typewriter, pop_in, die).chain());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lunge_goes_out_fast_and_comes_back() {
        assert_eq!(there_and_back(0.0), 0.0);
        assert_eq!(there_and_back(0.4), 1.0);
        assert!(there_and_back(0.7) < 1.0 && there_and_back(0.7) > 0.0);
        assert_eq!(there_and_back(1.0), 0.0);
        assert_eq!(there_and_back(5.0), 0.0);
    }

    #[test]
    fn the_generator_stays_between_nought_and_one() {
        let mut fx = Fx::default();
        let values: Vec<f32> = (0..200).map(|_| fx.random()).collect();
        assert!(values.iter().all(|v| (0.0..1.0).contains(v)));
        assert!(values.iter().any(|v| *v < 0.2) && values.iter().any(|v| *v > 0.8));
    }

    #[test]
    fn the_die_is_a_closed_shape_with_clear_corners() {
        let picture = die_picture();
        assert_eq!(picture.get(0, 0), CLEAR);
        assert_eq!(picture.get(18, 18), BRASS);
        assert_eq!(picture.get(18, 0), INK);
    }
}
