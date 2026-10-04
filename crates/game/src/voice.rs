//! The master's voice: a telling read aloud by one of Gemini's speaking models. Off unless
//! the player turns it on. It goes through the same keys as everything else, with a
//! rotation of its own over the models that speak, and spends their quota, not the
//! game master's.

use crate::audio::{Muted, level};
use crate::gm::Gm;
use crate::master::Master;
use crate::menu::Screen;
use crate::play::Phase;
use crate::store::Settings;
use bevy::audio::Volume;
use bevy::prelude::*;
use llm::GenError;
use std::sync::{Arc, Mutex};

/// The voice the master speaks in: one of the voices every speaking model has.
const VOICE: &str = "Charon";
/// A telling longer than this is read only so far.
const MOST_LETTERS: usize = 900;
/// After the voices refuse (no keys, quota spent) they are left alone for a while.
const REST_SECONDS: f32 = 300.0;

type Spoken = (String, Result<(Vec<u8>, f32, String), GenError>);

#[derive(Resource, Default)]
pub struct Voice {
    /// The telling last sent to be read, so that it is asked for once.
    asked_for: String,
    waiting: bool,
    rest: f32,
    ready: Arc<Mutex<Vec<Spoken>>>,
}

#[derive(Component)]
struct Speaking;

/// The text to read aloud: what the master tells, without the lines the game added itself.
fn script(narration: &str) -> String {
    let told: Vec<&str> = narration.lines().filter(|line| !line.starts_with('★')).collect();
    told.join(" ").chars().take(MOST_LETTERS).collect::<String>().trim().to_string()
}

#[allow(clippy::too_many_arguments)]
fn narrate(
    mut commands: Commands,
    time: Res<Time>,
    phase: Option<Res<Phase>>,
    screen: Res<State<Screen>>,
    settings: Res<Settings>,
    muted: Res<Muted>,
    gm: Res<Gm>,
    master: Res<Master>,
    mut voice: ResMut<Voice>,
    mut sources: ResMut<Assets<AudioSource>>,
    speaking: Query<Entity, With<Speaking>>,
) {
    voice.rest = (voice.rest - time.delta_secs()).max(0.0);
    let reading = match phase.as_deref() {
        Some(Phase::Reading { turn }) if *screen.get() == Screen::Play => script(&turn.narration),
        _ => String::new(),
    };
    // The sheet was closed, or shows something else: the voice stops with it.
    if reading != voice.asked_for {
        for entity in &speaking {
            commands.entity(entity).despawn();
        }
    }
    let arrived = std::mem::take(&mut *voice.ready.lock().unwrap());
    for (text, result) in arrived {
        voice.waiting = false;
        match result {
            Ok((wav, seconds, model)) => {
                info!("the master speaks: {seconds:.1} s from {model}");
                if text == reading {
                    commands.spawn((Speaking, AudioPlayer(sources.add(AudioSource { bytes: wav.into() })), PlaybackSettings::DESPAWN.with_volume(Volume::Linear(level(settings.sound, 1.0)))));
                }
            }
            Err(error) => {
                warn!("the master's voice did not answer: {error:?}");
                voice.rest = REST_SECONDS;
            }
        }
    }
    if !settings.voice || muted.0 || master.fake || reading.is_empty() || reading == voice.asked_for || voice.waiting || voice.rest > 0.0 {
        if reading.is_empty() {
            voice.asked_for.clear();
        }
        return;
    }
    voice.asked_for = reading.clone();
    voice.waiting = true;
    let (gm, ready) = (gm.clone(), voice.ready.clone());
    let (narrators, rotation, russian) = (settings.narrators(), master.rotation.clone(), settings.lang == crate::i18n::Lang::Ru);
    std::thread::spawn(move || {
        // The speaking models the player chose come first, in their order, their keys in
        // turn; Gemini's voices are the last to be asked.
        let chosen = if narrators.is_empty() { None } else { Some(rotation.speak(&narrators, &reading, russian, &llm::providers::Network)) };
        let result = match chosen {
            Some(Ok((sound, who))) => Ok((sound, 0.0, who)),
            other => {
                if let Some(Err(why)) = other {
                    warn!("the chosen voices did not answer: {}", why.chars().take(300).collect::<String>());
                }
                gm.dispatcher.speak(&reading, VOICE).map(|(speech, model)| (speech.wav(), speech.seconds(), model.id))
            }
        };
        ready.lock().unwrap().push((reading, result));
    });
}

pub struct VoicePlugin;

impl Plugin for VoicePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Voice>().add_systems(Update, narrate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_is_the_telling_without_the_games_own_lines_and_not_too_long() {
        assert_eq!(script("The door opens.\n★ Brak: 2"), "The door opens.");
        assert_eq!(script("One.\nTwo."), "One. Two.");
        assert_eq!(script(&"a".repeat(5_000)).chars().count(), MOST_LETTERS);
        assert_eq!(script("★ only"), "");
    }
}
