//! Sound and music. Effects are Kenney's (kenney.nl), the music is Juhani Junkala's
//! "Chiptune Adventures"; both are in the public domain (CC0) and travel inside the game's
//! own file. The music follows what is happening: the menu, a quiet place, a place with
//! something hostile in it, a fight.

use crate::menu::Screen;
use crate::play::{Game, HeroPose, Phase};
use crate::store::Settings;
use crate::widgets::Activate;
use bevy::audio::{AudioSinkPlayback, Volume};
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Sfx {
    Click,
    Back,
    Confirm,
    Error,
    Open,
    Close,
    Page,
    Door,
    Draw,
    Hit,
    Crit,
    Miss,
    Hurt,
    Fall,
    Heal,
    Magic,
    Item,
    Dice,
    Step,
    LevelUp,
    Victory,
    Defeat,
    Chat,
}

/// Asks for a sound. Anything in the game may write one.
#[derive(Message, Clone, Copy, Debug)]
pub struct Play(pub Sfx);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Track {
    Menu,
    Calm,
    Tense,
    Fight,
}

/// Set for instances that must make no noise: tests running with a hidden window.
#[derive(Resource, Default)]
pub struct Muted(pub bool);

#[derive(Resource)]
struct Bank {
    sfx: HashMap<Sfx, Vec<Handle<AudioSource>>>,
    music: HashMap<Track, Handle<AudioSource>>,
}

macro_rules! sound {
    ($assets:expr, $path:literal) => {
        $assets.add(AudioSource { bytes: include_bytes!(concat!("../assets/audio/", $path)).as_slice().into() })
    };
}

impl FromWorld for Bank {
    fn from_world(world: &mut World) -> Self {
        let mut assets = world.resource_mut::<Assets<AudioSource>>();
        let mut sfx = HashMap::new();
        let mut one = |kind: Sfx, handle: Handle<AudioSource>| sfx.entry(kind).or_insert_with(Vec::new).push(handle);
        one(Sfx::Click, sound!(assets, "sfx/click.ogg"));
        one(Sfx::Back, sound!(assets, "sfx/back.ogg"));
        one(Sfx::Confirm, sound!(assets, "sfx/confirm.ogg"));
        one(Sfx::Error, sound!(assets, "sfx/error.ogg"));
        one(Sfx::Open, sound!(assets, "sfx/open.ogg"));
        one(Sfx::Close, sound!(assets, "sfx/close.ogg"));
        one(Sfx::Page, sound!(assets, "sfx/page.ogg"));
        one(Sfx::Door, sound!(assets, "sfx/door.ogg"));
        one(Sfx::Draw, sound!(assets, "sfx/draw.ogg"));
        one(Sfx::Hit, sound!(assets, "sfx/hit1.ogg"));
        one(Sfx::Hit, sound!(assets, "sfx/hit2.ogg"));
        one(Sfx::Crit, sound!(assets, "sfx/crit.ogg"));
        one(Sfx::Miss, sound!(assets, "sfx/miss.ogg"));
        one(Sfx::Hurt, sound!(assets, "sfx/hurt.ogg"));
        one(Sfx::Fall, sound!(assets, "sfx/fall.ogg"));
        one(Sfx::Heal, sound!(assets, "sfx/heal.ogg"));
        one(Sfx::Magic, sound!(assets, "sfx/magic.ogg"));
        one(Sfx::Item, sound!(assets, "sfx/item.ogg"));
        one(Sfx::Dice, sound!(assets, "sfx/dice.ogg"));
        one(Sfx::Step, sound!(assets, "sfx/step0.ogg"));
        one(Sfx::Step, sound!(assets, "sfx/step1.ogg"));
        one(Sfx::Step, sound!(assets, "sfx/step2.ogg"));
        one(Sfx::Step, sound!(assets, "sfx/step3.ogg"));
        one(Sfx::LevelUp, sound!(assets, "sfx/levelup.ogg"));
        one(Sfx::Victory, sound!(assets, "sfx/victory.ogg"));
        one(Sfx::Defeat, sound!(assets, "sfx/defeat.ogg"));
        one(Sfx::Chat, sound!(assets, "sfx/chat.ogg"));
        let music = HashMap::from([
            (Track::Menu, sound!(assets, "music/menu.ogg")),
            (Track::Calm, sound!(assets, "music/calm.ogg")),
            (Track::Tense, sound!(assets, "music/tense.ogg")),
            (Track::Fight, sound!(assets, "music/fight.ogg")),
        ]);
        Self { sfx, music }
    }
}

/// A setting of 0–4 as a volume: off, then quarter steps. Music sits under the effects.
pub fn level(setting: u8, ceiling: f32) -> f32 {
    setting.min(4) as f32 / 4.0 * ceiling
}

const MUSIC_CEILING: f32 = 0.45;
const SOUND_CEILING: f32 = 0.9;
/// Volume gained or lost per second while one track gives way to another.
const FADE_PER_SECOND: f32 = 1.2;
const STEP_SECONDS: f32 = 0.28;

fn play_sounds(mut commands: Commands, mut asked: MessageReader<Play>, bank: Res<Bank>, settings: Res<Settings>, muted: Res<Muted>, mut turn: Local<usize>) {
    let volume = level(settings.sound, SOUND_CEILING);
    let mut heard: Vec<Sfx> = Vec::new();
    for Play(kind) in asked.read() {
        // The same sound asked for twice in one frame is one sound.
        if muted.0 || volume == 0.0 || heard.contains(kind) {
            continue;
        }
        heard.push(*kind);
        let Some(variants) = bank.sfx.get(kind).filter(|v| !v.is_empty()) else { continue };
        *turn = turn.wrapping_add(1);
        commands.spawn((AudioPlayer(variants[*turn % variants.len()].clone()), PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume))));
    }
}

/// The track playing, and how loud it is on its way in or out.
#[derive(Component)]
struct Music {
    track: Track,
    loudness: f32,
    leaving: bool,
}

/// What should be playing now.
fn wanted_track(screen: Screen, phase: Option<&Phase>, game: Option<&Game>) -> Track {
    if screen != Screen::Play {
        return Track::Menu;
    }
    match (phase, game) {
        (Some(Phase::Fighting), _) => Track::Fight,
        (_, Some(game)) if game.world.hostiles().next().is_some() => Track::Tense,
        _ => Track::Calm,
    }
}

#[allow(clippy::too_many_arguments)]
fn music(
    mut commands: Commands,
    time: Res<Time>,
    bank: Res<Bank>,
    settings: Res<Settings>,
    muted: Res<Muted>,
    screen: Res<State<Screen>>,
    phase: Option<Res<Phase>>,
    game: Option<Res<Game>>,
    mut playing: Query<(Entity, &mut Music, Option<&mut AudioSink>)>,
) {
    let ceiling = if muted.0 { 0.0 } else { level(settings.music, MUSIC_CEILING) };
    let wanted = wanted_track(*screen.get(), phase.as_deref(), game.as_deref());
    let step = FADE_PER_SECOND * time.delta_secs();
    let mut on_air = false;
    for (entity, mut track, sink) in &mut playing {
        if track.track == wanted && !track.leaving {
            on_air = true;
            track.loudness = (track.loudness + step).min(1.0);
        } else {
            track.leaving = true;
            track.loudness -= step;
            if track.loudness <= 0.0 {
                commands.entity(entity).despawn();
                continue;
            }
        }
        if let Some(mut sink) = sink {
            sink.set_volume(Volume::Linear(track.loudness * ceiling));
        }
    }
    if !on_air && ceiling > 0.0 {
        commands.spawn((AudioPlayer(bank.music[&wanted].clone()), PlaybackSettings::LOOP.with_volume(Volume::SILENT), Music { track: wanted, loudness: 0.0, leaving: false }));
    }
}

/// Sounds asked for through the effects, by code that cannot write a message itself.
fn cues(mut fx: ResMut<crate::fx::Fx>, mut sounds: MessageWriter<Play>) {
    if fx.cues.is_empty() {
        return;
    }
    for sound in std::mem::take(&mut fx.cues) {
        sounds.write(Play(sound));
    }
}

/// Every button makes the same small click.
fn click(_used: On<Activate>, mut sounds: MessageWriter<Play>) {
    sounds.write(Play(Sfx::Click));
}

/// What the play screen is doing has a sound when it changes.
fn phase_sounds(phase: Option<Res<Phase>>, screen: Res<State<Screen>>, mut before: Local<Option<std::mem::Discriminant<Phase>>>, mut sounds: MessageWriter<Play>) {
    let Some(phase) = phase.filter(|_| *screen.get() == Screen::Play) else {
        *before = None;
        return;
    };
    let now = std::mem::discriminant(&*phase);
    if before.replace(now) == Some(now) {
        return;
    }
    let sound = match &*phase {
        Phase::Leaving { .. } => Sfx::Door,
        Phase::Reading { .. } | Phase::Journal => Sfx::Page,
        Phase::Looking { .. } | Phase::Sheet | Phase::Paused | Phase::Writing { .. } | Phase::Master | Phase::Judging => Sfx::Open,
        Phase::Fighting => Sfx::Draw,
        Phase::Refused { .. } | Phase::Stuck { .. } => Sfx::Error,
        _ => return,
    };
    sounds.write(Play(sound));
}

fn footsteps(time: Res<Time>, pose: Option<Res<HeroPose>>, screen: Res<State<Screen>>, mut since: Local<f32>, mut sounds: MessageWriter<Play>) {
    if !pose.is_some_and(|pose| pose.moving) || *screen.get() != Screen::Play {
        *since = STEP_SECONDS;
        return;
    }
    *since += time.delta_secs();
    if *since >= STEP_SECONDS {
        *since = 0.0;
        sounds.write(Play(Sfx::Step));
    }
}

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Bank>().init_resource::<Muted>().add_message::<Play>().add_systems(Update, (cues, phase_sounds, footsteps, play_sounds, music).chain()).add_observer(click);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_step_from_silence_to_the_ceiling() {
        assert_eq!(level(0, 0.8), 0.0);
        assert_eq!(level(2, 0.8), 0.4);
        assert_eq!(level(4, 0.8), 0.8);
        assert_eq!(level(9, 0.8), 0.8, "a setting past the top is the top");
    }

    #[test]
    fn the_music_follows_the_screen_and_the_fight() {
        assert_eq!(wanted_track(Screen::Main, None, None), Track::Menu);
        assert_eq!(wanted_track(Screen::Settings, Some(&Phase::Fighting), None), Track::Menu);
        assert_eq!(wanted_track(Screen::Play, Some(&Phase::Fighting), None), Track::Fight);
        assert_eq!(wanted_track(Screen::Play, Some(&Phase::Walking), None), Track::Calm);
        let draft: world::WorldDraft = serde_json::from_str(&world::fake::world("a")).unwrap();
        let mut w = world::build_world(&draft, "a", "English").unwrap().0;
        let game = |world: world::World| Game { world, world_id: "a".into() };
        assert_eq!(wanted_track(Screen::Play, Some(&Phase::Walking), Some(&game(w.clone()))), Track::Calm);
        w.here_mut().creatures[0].attitude = world::Attitude::Hostile;
        assert_eq!(wanted_track(Screen::Play, Some(&Phase::Walking), Some(&game(w))), Track::Tense);
    }
}
