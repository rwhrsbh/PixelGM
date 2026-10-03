//! The sky over a place: rain that rings the water where it falls, a storm with lightning,
//! snow, fog drifting through, and on a sunny day shafts of light and the shadows of clouds
//! passing over the ground. The master says what the sky is; the player may switch it off.
//!
//! An emitter covers a rectangle of the canvas and owns everything it makes as its children,
//! so that the sky goes with the place, or with the preview, when they go.

use crate::art::Pixels;
use crate::i18n::T;
use crate::scene::{PX, cloud_noise};
use crate::theme::CLEAR;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub enum Sky {
    #[default]
    Clear,
    Sun,
    Rain,
    Storm,
    Snow,
    Fog,
}

impl Sky {
    pub const ALL: [Sky; 6] = [Sky::Clear, Sky::Sun, Sky::Rain, Sky::Storm, Sky::Snow, Sky::Fog];

    pub fn label(self) -> T {
        match self {
            Sky::Clear => T::SkyClear,
            Sky::Sun => T::SkySun,
            Sky::Rain => T::SkyRain,
            Sky::Storm => T::SkyStorm,
            Sky::Snow => T::SkySnow,
            Sky::Fog => T::SkyFog,
        }
    }

    /// The sky the master wrote for a place, in either language.
    pub fn of(words: &str) -> Self {
        let w = words.trim().to_lowercase();
        let has = |list: &[&str]| list.iter().any(|x| w.contains(x));
        if has(&["storm", "thunder", "гроз", "шторм", "буря"]) {
            Sky::Storm
        } else if has(&["rain", "drizzle", "shower", "дожд", "ливень", "морось"]) {
            Sky::Rain
        } else if has(&["snow", "blizzard", "снег", "метел", "вьюг"]) {
            Sky::Snow
        } else if has(&["fog", "mist", "haze", "туман", "дымк", "мгла"]) {
            Sky::Fog
        } else if has(&["sun", "bright", "солн", "ясн"]) {
            Sky::Sun
        } else {
            Sky::Clear
        }
    }
}

/// Makes the weather over a rectangle of the canvas (top left origin, y down).
#[derive(Component)]
pub struct Emitter {
    pub sky: Sky,
    /// How dark the hour is: fog and light are dimmed with it.
    pub dim: f32,
    pub area: Rect,
    /// Rectangles of water, where rain makes rings instead of splashes.
    pub water: Vec<Rect>,
    since: f32,
    /// Seconds to the next lightning, and how bright the last one still is.
    flash: f32,
    bright: f32,
    seed: u32,
}

impl Emitter {
    pub fn new(sky: Sky, area: Rect, water: Vec<Rect>) -> Self {
        Self { sky, dim: 0.0, area, water, since: 0.0, flash: 2.0, bright: 0.0, seed: 0x9e37_79b9 }
    }

    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed % 10_000) as f32 / 10_000.0
    }
}

#[derive(Component)]
struct Drop {
    velocity: Vec2,
    /// Where it lands, in canvas pixels down.
    lands: f32,
    snow: bool,
    sway: f32,
}

#[derive(Component)]
struct Fades {
    age: f32,
    life: f32,
    grow: f32,
}

/// A layer that slides across its emitter: fog, cloud shadows, light.
#[derive(Component)]
struct Drift {
    speed: f32,
    width: f32,
    pulse: f32,
    base: f32,
    /// How much of its own colour the layer keeps: less in the dark.
    shade: f32,
}

#[derive(Component)]
struct Flash;

#[derive(Resource)]
struct WeatherArt {
    streak: Handle<Image>,
    flake: Handle<Image>,
    splash: Handle<Image>,
    ring: Handle<Image>,
    white: Handle<Image>,
    /// Large layers, by kind and size.
    layers: HashMap<(u8, u32, u32), Handle<Image>>,
}

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, make_art).add_systems(Update, (start_layers, emit, fall, fade, drift, flash));
    }
}

fn small(w: u32, h: u32, f: impl Fn(i32, i32) -> [u8; 4]) -> Pixels {
    let mut p = Pixels::new(w, h, CLEAR);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            p.set(x, y, f(x, y));
        }
    }
    p
}

fn make_art(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let streak = small(3, 14, |x, y| if x < 2 && (x + y / 7) % 2 == 0 || x == 1 { [200, 222, 245, (90 + y * 10) as u8] } else { CLEAR });
    let flake = small(3, 3, |x, y| if (x - 1).abs() + (y - 1).abs() <= 1 { [245, 248, 255, if x == 1 && y == 1 { 240 } else { 140 }] } else { CLEAR });
    let splash = small(6, 3, |x, y| if (y == 0 && (x == 0 || x == 5)) || (y == 1 && (x == 1 || x == 4)) || (y == 2 && (2..4).contains(&x)) { [200, 220, 240, 170] } else { CLEAR });
    let ring = small(16, 8, |x, y| {
        let (dx, dy) = ((x as f32 + 0.5 - 8.0) / 8.0, (y as f32 + 0.5 - 4.0) / 4.0);
        let d = dx * dx + dy * dy;
        if (0.62..=1.0).contains(&d) { [215, 230, 245, 180] } else { CLEAR }
    });
    let white = small(1, 1, |_, _| [255, 255, 255, 255]);
    commands.insert_resource(WeatherArt {
        streak: images.add(streak.into_image()),
        flake: images.add(flake.into_image()),
        splash: images.add(splash.into_image()),
        ring: images.add(ring.into_image()),
        white: images.add(white.into_image()),
        layers: HashMap::new(),
    });
}

/// From the canvas (top left origin) to the world.
fn world_at(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x - crate::screen::CANVAS.x as f32 / 2.0, crate::screen::CANVAS.y as f32 / 2.0 - y, z)
}

/// A layer twice as wide as its area, whose two halves match, so that sliding it loops.
fn layer(kind: u8, w: u32, h: u32) -> Pixels {
    let (w2, step) = (w * 2, PX as i32);
    small(w2, h, |x, y| {
        // Noise that wraps at the layer's width, in steps of the world's pixels.
        let (x, y) = (x / step * step, y / step * step);
        let n = cloud_noise(x.rem_euclid(w as i32), y, w as i32, 90);
        match kind {
            // Fog: a pale veil, thicker in patches.
            0 => [210, 214, 222, (n * 120.0) as u8],
            // Cloud shadows: soft dark blobs with clear sky between.
            1 => {
                let a = ((n - 0.5) * 2.5).clamp(0.0, 1.0);
                [12, 18, 34, (a * 80.0) as u8]
            }
            // Sun: shafts of light across the picture, on a slant.
            _ => {
                let band = (((x as f32 * 0.7 + y as f32) / 70.0).sin() * 0.5 + 0.5).powf(3.0);
                [255, 236, 170, (band * n * 70.0) as u8]
            }
        }
    })
}

/// An emitter's layers, made once when it appears.
fn start_layers(mut commands: Commands, new: Query<(Entity, &Emitter), Added<Emitter>>, mut art: ResMut<WeatherArt>, mut images: ResMut<Assets<Image>>) {
    for (entity, emitter) in &new {
        let (w, h) = (emitter.area.width() as u32, emitter.area.height() as u32);
        let mut get = |kind: u8| art.layers.entry((kind, w, h)).or_insert_with(|| images.add(layer(kind, w, h).into_image())).clone();
        let corner = world_at(emitter.area.min.x, emitter.area.min.y, 0.0);
        let mut layers: Vec<(u8, f32, f32, f32, f32)> = Vec::new();
        match emitter.sky {
            Sky::Sun => {
                layers.push((1, 0.95, 9.0, 0.0, 1.0));
                layers.push((2, 1.56, 4.0, 0.25, 0.8));
            }
            Sky::Fog => {
                layers.push((0, 1.55, 7.0, 0.08, 0.5));
                layers.push((0, 1.57, -4.0, 0.1, 0.35));
            }
            Sky::Rain | Sky::Storm => layers.push((1, 0.95, 16.0, 0.0, 0.7)),
            Sky::Snow => layers.push((0, 1.55, 3.0, 0.0, 0.35)),
            Sky::Clear => {}
        }
        let mut emitter_entity = commands.entity(entity);
        for (kind, z, speed, pulse, base) in layers {
            let image = get(kind);
            emitter_entity.with_children(|sky| {
                sky.spawn((
                    Drift { speed, width: w as f32, pulse, base, shade: 1.0 - emitter.dim * 0.85 },
                    Sprite { rect: Some(Rect::new(0.0, 0.0, w as f32, h as f32)), color: Color::srgba(1.0 - emitter.dim * 0.85, 1.0 - emitter.dim * 0.85, 1.0 - emitter.dim * 0.85, base), ..Sprite::from_image(image) },
                    Anchor::TOP_LEFT,
                    Transform::from_translation(corner.with_z(z)),
                ));
            });
        }
        // Rain dims the day a little.
        if matches!(emitter.sky, Sky::Rain | Sky::Storm) {
            let white = art.white.clone();
            emitter_entity.with_children(|sky| {
                sky.spawn((Sprite { custom_size: Some(emitter.area.size()), color: Color::srgba(0.12, 0.16, 0.24, 0.22), ..Sprite::from_image(white.clone()) }, Anchor::TOP_LEFT, Transform::from_translation(corner.with_z(1.54))));
                sky.spawn((Flash, Sprite { custom_size: Some(emitter.area.size()), color: Color::srgba(0.9, 0.93, 1.0, 0.0), ..Sprite::from_image(white) }, Anchor::TOP_LEFT, Transform::from_translation(corner.with_z(1.59))));
            });
        }
    }
}

fn emit(time: Res<Time>, mut commands: Commands, mut emitters: Query<(Entity, &mut Emitter)>, art: Res<WeatherArt>) {
    let dt = time.delta_secs().min(0.1);
    for (entity, mut e) in &mut emitters {
        let (per_second, snow) = match e.sky {
            Sky::Rain => (0.0011, false),
            Sky::Storm => (0.0024, false),
            Sky::Snow => (0.00028, true),
            _ => continue,
        };
        // As many drops as the area is large.
        let area = e.area.width() * e.area.height();
        e.since += dt * per_second * area;
        let mut drops = Vec::new();
        while e.since >= 1.0 {
            e.since -= 1.0;
            let x = e.area.min.x + e.random() * (e.area.width() + 60.0);
            let lands = e.area.min.y + 10.0 + e.random() * (e.area.height() - 10.0);
            let start = e.area.min.y - 10.0 + e.random() * (lands - e.area.min.y).min(140.0) * 0.2;
            let velocity = if snow { Vec2::new(-8.0 + e.random() * 6.0, 34.0 + e.random() * 18.0) } else { Vec2::new(-70.0, 520.0 + e.random() * 140.0) * if e.sky == Sky::Storm { 1.25 } else { 1.0 } };
            let sway = e.random() * 6.28;
            drops.push((x, start, lands, velocity, sway));
        }
        if e.sky == Sky::Storm {
            e.flash -= dt;
            if e.flash <= 0.0 {
                e.flash = 4.0 + e.random() * 7.0;
                e.bright = 1.0;
            }
        }
        if drops.is_empty() {
            continue;
        }
        let image = if snow { art.flake.clone() } else { art.streak.clone() };
        let area = e.area;
        commands.entity(entity).with_children(|sky| {
            for (x, y, lands, velocity, sway) in drops {
                // Only what will fall inside the area is made.
                if x + velocity.x * (lands - y) / velocity.y < area.min.x {
                    continue;
                }
                sky.spawn((Drop { velocity, lands, snow, sway }, Sprite::from_image(image.clone()), Transform::from_translation(world_at(x, y, 1.58)), Visibility::Inherited));
            }
        });
    }
}

fn fall(time: Res<Time>, mut commands: Commands, mut drops: Query<(Entity, &mut Drop, &mut Transform, &ChildOf)>, emitters: Query<&Emitter>, art: Res<WeatherArt>) {
    let dt = time.delta_secs().min(0.1);
    let t = time.elapsed_secs();
    for (entity, mut drop, mut transform, parent) in &mut drops {
        let sway = if drop.snow { (t * 1.7 + drop.sway).sin() * 14.0 } else { 0.0 };
        transform.translation.x += (drop.velocity.x + sway) * dt;
        transform.translation.y -= drop.velocity.y * dt;
        let canvas_y = crate::screen::CANVAS.y as f32 / 2.0 - transform.translation.y;
        if canvas_y < drop.lands {
            continue;
        }
        commands.entity(entity).despawn();
        let canvas_x = transform.translation.x + crate::screen::CANVAS.x as f32 / 2.0;
        let Ok(emitter) = emitters.get(parent.parent()) else { continue };
        if !emitter.area.contains(Vec2::new(canvas_x, canvas_y)) {
            continue;
        }
        // On water a ring spreads; on the ground a drop splashes and snow lies a moment.
        let on_water = emitter.water.iter().any(|w| w.contains(Vec2::new(canvas_x, canvas_y)));
        let (image, life, grow, z) = if drop.snow {
            (art.flake.clone(), 1.2, 0.0, 0.6)
        } else if on_water {
            (art.ring.clone(), 0.7, 1.6, 0.5)
        } else {
            (art.splash.clone(), 0.18, 0.3, 0.6)
        };
        let _ = drop.as_mut();
        commands.entity(parent.parent()).with_children(|sky| {
            sky.spawn((Fades { age: 0.0, life, grow }, Sprite::from_image(image), Transform::from_translation(world_at(canvas_x, canvas_y, z)).with_scale(Vec3::splat(if grow > 1.0 { 0.4 } else { 1.0 }))));
        });
    }
}

fn fade(time: Res<Time>, mut commands: Commands, mut things: Query<(Entity, &mut Fades, &mut Sprite, &mut Transform)>) {
    for (entity, mut f, mut sprite, mut transform) in &mut things {
        f.age += time.delta_secs();
        let t = f.age / f.life;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        sprite.color = Color::srgba(1.0, 1.0, 1.0, ((1.0 - t) * 4.0).round() / 4.0);
        if f.grow > 0.0 {
            let s = (0.4 + t * f.grow).round_to_step(0.2);
            transform.scale = Vec3::new(s, s, 1.0);
        }
    }
}

trait RoundTo {
    fn round_to_step(self, step: f32) -> f32;
}

impl RoundTo for f32 {
    fn round_to_step(self, step: f32) -> f32 {
        (self / step).round() * step
    }
}

fn drift(time: Res<Time>, mut layers: Query<(&Drift, &mut Sprite)>) {
    let t = time.elapsed_secs();
    for (d, mut sprite) in &mut layers {
        // Slides in whole pixels of the world, so the layer never smears.
        let offset = ((t * d.speed).rem_euclid(d.width) / PX as f32).floor() * PX as f32;
        if let Some(rect) = sprite.rect.as_mut() {
            let h = rect.height();
            *rect = Rect::new(offset, 0.0, offset + d.width, h);
        }
        if d.pulse > 0.0 {
            let a = d.base + (t * 0.4).sin() * d.pulse;
            sprite.color = Color::srgba(d.shade, d.shade, d.shade, (a.clamp(0.0, 1.0) * 8.0).round() / 8.0);
        }
    }
}

/// Lightning: when a storm's clock runs out, the sky flashes twice.
fn flash(time: Res<Time>, mut emitters: Query<(&mut Emitter, &Children)>, mut flashes: Query<&mut Sprite, With<Flash>>) {
    for (mut emitter, children) in &mut emitters {
        if emitter.sky != Sky::Storm {
            continue;
        }
        emitter.bright = (emitter.bright - time.delta_secs() * 2.2).max(0.0);
        // Flickering on the way down, in steps.
        let flicker = if (emitter.bright * 14.0) as i32 % 3 == 0 { 0.35 } else { 1.0 };
        let a = (emitter.bright * flicker * 0.6 * 4.0).round() / 4.0;
        for child in children {
            if let Ok(mut sprite) = flashes.get_mut(*child) {
                sprite.color = Color::srgba(0.9, 0.93, 1.0, a);
            }
        }
    }
}

/// The preview's sky, over its little scene.
pub fn preview(commands: &mut Commands, index: usize, enabled: bool, area: Rect, water: Vec<Rect>, marker: impl Component) {
    let sky = Sky::ALL[index % Sky::ALL.len()];
    if !enabled || sky == Sky::Clear {
        return;
    }
    commands.spawn((marker, Emitter::new(sky, area, water), Transform::default(), Visibility::default()));
}
