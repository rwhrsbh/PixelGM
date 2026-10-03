//! Music that follows the story and does not repeat itself. A catalogue of hundreds of free
//! pieces, each tagged with the moods it fits — calm, exploring, tense, battle, mystery,
//! horror, sorrow, a tavern, triumph, the deep — is fetched a piece at a time in the
//! background, the mood being played first, and kept on disk. Without a connection whatever
//! was already fetched plays, and the few pieces built into the game. The master may set the
//! mood of a place; otherwise the game picks one from what is going on.

use crate::audio::{Bank, Muted, Track, level};
use crate::menu::Screen;
use crate::play::{Game, Phase};
use crate::store::Settings;
use bevy::audio::{AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

const CATALOGUE: &str = include_str!("../assets/music/catalog.json");
const MUSIC_CEILING: f32 = 0.45;
const FADE_PER_SECOND: f32 = 0.6;
/// Pieces of a mood to have on disk before fetching for other moods.
const ENOUGH: usize = 10;
/// How much music may be kept, in bytes.
const KEEP: u64 = 4 * 1024 * 1024 * 1024;
/// After a failed fetch, how long to wait before trying the network again.
const OFFLINE_SECONDS: f32 = 300.0;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Mood {
    Menu,
    Calm,
    Explore,
    Tense,
    Battle,
    Boss,
    Mystery,
    Horror,
    Sad,
    Tavern,
    Triumph,
    Dungeon,
}

impl Mood {
    const PLAYED: [Mood; 11] = [Mood::Calm, Mood::Explore, Mood::Tense, Mood::Battle, Mood::Boss, Mood::Mystery, Mood::Horror, Mood::Sad, Mood::Tavern, Mood::Triumph, Mood::Dungeon];

    fn tag(self) -> &'static str {
        match self {
            Mood::Menu => "menu",
            Mood::Calm => "calm",
            Mood::Explore => "explore",
            Mood::Tense => "tense",
            Mood::Battle => "battle",
            Mood::Boss => "boss",
            Mood::Mystery => "mystery",
            Mood::Horror => "horror",
            Mood::Sad => "sad",
            Mood::Tavern => "tavern",
            Mood::Triumph => "triumph",
            Mood::Dungeon => "dungeon",
        }
    }

    /// The mood a word of the master's stands for.
    pub fn of(word: &str) -> Option<Self> {
        let w = word.trim().to_lowercase();
        Mood::PLAYED.into_iter().find(|m| w.contains(m.tag())).or(match w.as_str() {
            "peaceful" | "quiet" => Some(Mood::Calm),
            "danger" | "suspense" => Some(Mood::Tense),
            "fight" | "combat" => Some(Mood::Battle),
            "dark" | "deep" | "cave" => Some(Mood::Dungeon),
            "fear" | "dread" => Some(Mood::Horror),
            "sorrow" | "grief" => Some(Mood::Sad),
            "victory" => Some(Mood::Triumph),
            "inn" => Some(Mood::Tavern),
            _ => None,
        })
    }

    /// The built-in piece that stands in when nothing of the mood has been fetched.
    fn fallback(self) -> Track {
        match self {
            Mood::Menu => Track::Menu,
            Mood::Battle | Mood::Boss => Track::Fight,
            Mood::Tense | Mood::Horror | Mood::Dungeon | Mood::Mystery => Track::Tense,
            _ => Track::Calm,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[allow(dead_code)]
pub struct Piece {
    pub id: String,
    pub url: String,
    pub title: String,
    pub author: String,
    pub license: String,
    pub moods: Vec<String>,
}

enum Fetched {
    Done(String),
    Failed,
}

/// The catalogue, what of it is on disk, and the fetching.
#[derive(Resource)]
pub struct Library {
    pieces: Vec<Piece>,
    on_disk: HashMap<String, PathBuf>,
    asked: Option<String>,
    offline_for: f32,
    to_fetch: Sender<(String, String, PathBuf)>,
    fetched: Mutex<Receiver<Fetched>>,
    dir: PathBuf,
    /// The pieces played lately, not to be played again soon.
    lately: VecDeque<String>,
}

impl Library {
    fn new() -> Self {
        let pieces: Vec<Piece> = serde_json::from_str(CATALOGUE).unwrap_or_default();
        let dir = crate::store::home().join("music");
        let _ = std::fs::create_dir_all(&dir);
        let on_disk = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|f| f.path())
            .filter(|p| p.extension().is_some_and(|e| e == "mp3" || e == "ogg"))
            .filter_map(|p| Some((p.file_stem()?.to_str()?.to_string(), p)))
            .collect();
        let (to_fetch, jobs) = channel::<(String, String, PathBuf)>();
        let (done, fetched) = channel::<Fetched>();
        std::thread::spawn(move || {
            for (id, url, path) in jobs {
                let part = path.with_extension("part");
                let result = ureq::get(&url).timeout(std::time::Duration::from_secs(120)).call().map_err(|e| e.to_string()).and_then(|answer| {
                    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
                    std::io::copy(&mut answer.into_reader(), &mut file).map_err(|e| e.to_string())?;
                    std::fs::rename(&part, &path).map_err(|e| e.to_string())
                });
                let _ = done.send(match result {
                    Ok(()) => Fetched::Done(id),
                    Err(e) => {
                        let _ = std::fs::remove_file(&part);
                        warn!("music: could not fetch {id}: {e}");
                        let _ = id;
                        Fetched::Failed
                    }
                });
            }
        });
        Self { pieces, on_disk, asked: None, offline_for: 0.0, to_fetch, fetched: Mutex::new(fetched), dir, lately: VecDeque::new() }
    }

    fn of_mood(&self, mood: Mood) -> impl Iterator<Item = &Piece> {
        self.pieces.iter().filter(move |p| p.moods.iter().any(|m| m == mood.tag()))
    }

    fn kept(&self, mood: Mood) -> usize {
        self.of_mood(mood).filter(|p| self.on_disk.contains_key(&p.id)).count()
    }

    fn bytes_kept(&self) -> u64 {
        self.on_disk.values().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum()
    }

    /// A piece of the mood that is on disk and was not played lately.
    fn pick(&self, mood: Mood, seed: u64) -> Option<&Piece> {
        let ready: Vec<&Piece> = self.of_mood(mood).filter(|p| self.on_disk.contains_key(&p.id)).collect();
        let fresh: Vec<&Piece> = ready.iter().copied().filter(|p| !self.lately.contains(&p.id)).collect();
        let from = if fresh.is_empty() { ready } else { fresh };
        (!from.is_empty()).then(|| from[(seed % from.len() as u64) as usize])
    }

}

/// The piece on the air.
#[derive(Component)]
struct OnAir {
    mood: Mood,
    loudness: f32,
    leaving: bool,
    /// A built-in piece loops; a fetched one plays once and gives way to the next.
    looping: bool,
}

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Library::new()).add_systems(Update, (fetch, play).chain());
    }
}

/// The mood of what is happening now.
fn wanted(screen: Screen, phase: Option<&Phase>, game: Option<&Game>) -> Mood {
    if screen != Screen::Play {
        return Mood::Menu;
    }
    if matches!(phase, Some(Phase::Fighting)) && game.is_none() {
        return Mood::Battle;
    }
    let Some(game) = game else { return Mood::Calm };
    let world = &game.world;
    let here = world.here();
    if matches!(phase, Some(Phase::Fighting)) || world.fight_here() {
        let many = world.fight.as_ref().is_some_and(|f| f.combatants.iter().filter(|c| c.side == rules::Side::Enemy && !c.dead).count() >= 4);
        return if many { Mood::Boss } else { Mood::Battle };
    }
    // What the master said the place feels like.
    if let Some(mood) = Mood::of(&here.mood) {
        return mood;
    }
    if world.hostiles().next().is_some() {
        return Mood::Tense;
    }
    let words = format!("{} {}", here.name, here.description).to_lowercase();
    let has = |list: &[&str]| list.iter().any(|w| words.contains(w));
    if has(&["tavern", "inn", "таверн", "трактир", "корчм", "харчевн"]) {
        return Mood::Tavern;
    }
    match here.light.as_str() {
        "dark" | "cave" | "underground" => Mood::Dungeon,
        "night" => Mood::Mystery,
        _ if has(&["crypt", "tomb", "склеп", "гробниц", "могил", "кладбищ"]) => Mood::Horror,
        _ => Mood::Explore,
    }
}

/// Fetches pieces in the background: of the mood wanted first, then of the others.
fn fetch(time: Res<Time>, settings: Res<Settings>, screen: Res<State<Screen>>, phase: Option<Res<Phase>>, game: Option<Res<Game>>, mut library: ResMut<Library>) {
    // What came in.
    let results: Vec<Fetched> = library.fetched.lock().map(|r| r.try_iter().collect()).unwrap_or_default();
    for result in results {
        match result {
            Fetched::Done(id) => {
                let path = library.pieces.iter().find(|p| p.id == id).map(|p| library.dir.join(format!("{}.{}", p.id, if p.url.ends_with(".ogg") { "ogg" } else { "mp3" })));
                if let Some(path) = path {
                    info!("music: fetched {id}");
                    library.on_disk.insert(id, path);
                }
                library.asked = None;
            }
            Fetched::Failed => {
                library.asked = None;
                library.offline_for = OFFLINE_SECONDS;
            }
        }
    }
    library.offline_for = (library.offline_for - time.delta_secs()).max(0.0);
    if !settings.online_music || library.asked.is_some() || library.offline_for > 0.0 {
        return;
    }
    let now = wanted(*screen.get(), phase.as_deref(), game.as_deref());
    // The mood wanted first; then whichever mood has least.
    let mood = if now != Mood::Menu && library.kept(now) < ENOUGH { Some(now) } else { Mood::PLAYED.into_iter().filter(|m| library.kept(*m) < ENOUGH * 3).min_by_key(|m| library.kept(*m)) };
    let Some(mood) = mood else { return };
    if library.bytes_kept() > KEEP {
        return;
    }
    let seed = (time.elapsed_secs() * 1000.0) as usize;
    let missing: Vec<&Piece> = library.of_mood(mood).filter(|p| !library.on_disk.contains_key(&p.id)).collect();
    let Some(piece) = missing.get(seed % missing.len().max(1)).map(|p| (*p).clone()) else { return };
    let path = library.dir.join(format!("{}.{}", piece.id, if piece.url.ends_with(".ogg") { "ogg" } else { "mp3" }));
    if library.to_fetch.send((piece.id.clone(), piece.url.clone(), path)).is_ok() {
        library.asked = Some(piece.id);
    }
}

#[allow(clippy::too_many_arguments)]
fn play(
    mut commands: Commands,
    time: Res<Time>,
    bank: Res<Bank>,
    settings: Res<Settings>,
    muted: Res<Muted>,
    screen: Res<State<Screen>>,
    phase: Option<Res<Phase>>,
    game: Option<Res<Game>>,
    mut library: ResMut<Library>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut playing: Query<(Entity, &mut OnAir, Option<&mut AudioSink>)>,
) {
    let ceiling = if muted.0 { 0.0 } else { level(settings.music, MUSIC_CEILING) };
    let mood = wanted(*screen.get(), phase.as_deref(), game.as_deref());
    let step = FADE_PER_SECOND * time.delta_secs();
    let mut on_air = false;
    for (entity, mut piece, sink) in &mut playing {
        // A fetched piece that has ended gives way; a piece of another mood fades out.
        let ended = !piece.looping && sink.as_ref().is_some_and(|s| s.empty());
        if piece.mood == mood && !piece.leaving && !ended {
            on_air = true;
            piece.loudness = (piece.loudness + step).min(1.0);
        } else {
            piece.leaving = true;
            piece.loudness -= step * if ended { 4.0 } else { 1.0 };
            if piece.loudness <= 0.0 {
                commands.entity(entity).despawn();
                continue;
            }
        }
        if let Some(mut sink) = sink {
            sink.set_volume(Volume::Linear(piece.loudness.max(0.0) * ceiling));
        }
    }
    if on_air || ceiling <= 0.0 {
        return;
    }
    // Something fetched for the mood, not played lately; otherwise the built-in piece.
    let seed = (time.elapsed_secs() * 7919.0) as u64 ^ library.lately.len() as u64;
    let chosen = (mood != Mood::Menu).then(|| library.pick(mood, seed).map(|p| (p.id.clone(), library.on_disk[&p.id].clone()))).flatten();
    match chosen.and_then(|(id, path)| std::fs::read(&path).ok().map(|bytes| (id, bytes))) {
        Some((id, bytes)) => {
            library.lately.push_back(id.clone());
            while library.lately.len() > 30 {
                library.lately.pop_front();
            }
            info!("music: {mood:?} — {id}");
            let source = sources.add(AudioSource { bytes: bytes.into() });
            commands.spawn((AudioPlayer(source), PlaybackSettings::ONCE.with_volume(Volume::SILENT), OnAir { mood, loudness: 0.0, leaving: false, looping: false }));
        }
        None => {
            if let Some(source) = bank.music.get(&mood.fallback()) {
                commands.spawn((AudioPlayer(source.clone()), PlaybackSettings::LOOP.with_volume(Volume::SILENT), OnAir { mood, loudness: 0.0, leaving: false, looping: true }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_reads_and_every_mood_has_pieces() {
        let pieces: Vec<Piece> = serde_json::from_str(CATALOGUE).unwrap();
        assert!(pieces.len() > 300);
        for mood in Mood::PLAYED {
            assert!(pieces.iter().any(|p| p.moods.iter().any(|m| m == mood.tag())), "{mood:?}");
        }
        assert!(pieces.iter().all(|p| p.url.starts_with("https://")));
    }

    #[test]
    fn the_music_follows_the_screen_the_fight_the_danger_and_the_master() {
        assert_eq!(wanted(Screen::Main, None, None), Mood::Menu);
        assert_eq!(wanted(Screen::Settings, Some(&Phase::Fighting), None), Mood::Menu);
        assert_eq!(wanted(Screen::Play, Some(&Phase::Fighting), None), Mood::Battle);
        let draft: world::WorldDraft = serde_json::from_str(&world::fake::world("a")).unwrap();
        let mut w = world::build_world(&draft, "a", "English").unwrap().0;
        let game = |world: world::World| Game { world, world_id: "a".into() };
        assert_eq!(wanted(Screen::Play, Some(&Phase::Walking), Some(&game(w.clone()))), Mood::Explore);
        w.here_mut().creatures[0].attitude = world::Attitude::Hostile;
        assert_eq!(wanted(Screen::Play, Some(&Phase::Walking), Some(&game(w.clone()))), Mood::Tense);
        w.here_mut().mood = "horror".into();
        assert_eq!(wanted(Screen::Play, Some(&Phase::Walking), Some(&game(w))), Mood::Horror);
    }

    #[test]
    fn the_masters_words_name_moods() {
        assert_eq!(Mood::of("Horror"), Some(Mood::Horror));
        assert_eq!(Mood::of("danger"), Some(Mood::Tense));
        assert_eq!(Mood::of("tavern"), Some(Mood::Tavern));
        assert_eq!(Mood::of("whatever"), None);
    }
}
