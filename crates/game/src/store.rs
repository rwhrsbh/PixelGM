//! What the game remembers between launches: settings and the player's API keys.

use crate::i18n::Lang;
use crate::secret;
use bevy::prelude::*;
use llm::{SavedState, ThinkingSetting, Tier};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use world::World;

#[derive(Resource, Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Settings {
    pub lang: Lang,
    pub fullscreen: bool,
    /// How much the game master's model thinks before answering.
    pub thinking: ThinkingSetting,
    /// The weakest tier the game master may fall back to when better models are spent.
    pub lowest_tier: Tier,
    /// Draw neighbouring places before the party walks into them.
    pub predraw: bool,
    /// Whether the game master makes things happen when the players are idle.
    pub initiative: Initiative,
    /// Tokens of word-for-word story the master is sent; past four fifths of it the older
    /// part is written up as a chapter. Far below any model's window on purpose: every
    /// request carries it, and the smallest fallback model must fit it too.
    pub memory_budget: usize,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Initiative {
    Off,
    #[default]
    Rare,
    Often,
}

impl Default for Settings {
    fn default() -> Self {
        Self { lang: Lang::Ru, fullscreen: false, thinking: ThinkingSetting::Medium, lowest_tier: Tier::Gemma, predraw: true, initiative: Initiative::Rare, memory_budget: 30_000 }
    }
}

/// What a save says about itself, shown in the list of saves.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct SaveMeta {
    /// The world this save belongs to; all saves of one world share a folder.
    pub world_id: String,
    pub title: String,
    /// The place the party was in.
    pub place: String,
    /// Seconds since the Unix epoch.
    pub saved_at: u64,
    /// Made by the player, as opposed to the automatic save.
    pub manual: bool,
}

#[derive(Serialize, Deserialize)]
struct SaveFile {
    meta: SaveMeta,
    world: World,
}

/// A save found on disk: where it is and what it says about itself.
#[derive(Clone, PartialEq, Debug)]
pub struct SaveEntry {
    pub path: PathBuf,
    pub meta: SaveMeta,
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[derive(Serialize, Deserialize, Default)]
struct KeyFile {
    /// Each key encrypted on its own, as hex.
    keys: Vec<String>,
}

/// The folder the game keeps its files in. `PIXELGM_HOME` overrides it, which is how several
/// instances run side by side on one machine without sharing settings.
pub fn home() -> PathBuf {
    if let Some(dir) = std::env::var_os("PIXELGM_HOME") {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("PixelGM")
}

#[derive(Resource, Clone)]
pub struct Store {
    dir: PathBuf,
}

/// Writes through a temporary file so a crash mid-write cannot leave a half-written file.
fn write_whole(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

impl Store {
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Missing or unreadable settings fall back to defaults: a broken file must not stop the
    /// game from starting.
    pub fn load_settings(&self) -> Settings {
        fs::read_to_string(self.dir.join("settings.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) -> io::Result<()> {
        write_whole(&self.dir.join("settings.json"), serde_json::to_string_pretty(settings)?.as_bytes())
    }

    /// Keys that cannot be decrypted (file copied from another account) are skipped.
    pub fn load_keys(&self) -> Vec<String> {
        let file: KeyFile = fs::read_to_string(self.dir.join("keys.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        file.keys
            .iter()
            .filter_map(|hex| secret::from_hex(hex))
            .filter_map(|blob| secret::unprotect(&blob).ok())
            .filter_map(|plain| String::from_utf8(plain).ok())
            .collect()
    }

    /// Keeps what the game master answered, word for word, for looking into a place that
    /// came out wrong. Only the newest few answers are kept.
    pub fn log_answer(&self, model: &str, text: &str) {
        const KEPT: usize = 20;
        let dir = self.dir.join("answers");
        let name = format!("{}-{}.txt", unix_now(), model.replace(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '.', "_"));
        if let Err(e) = write_whole(&dir.join(name), text.as_bytes()) {
            warn!("could not keep the game master's answer: {e}");
            return;
        }
        let mut files: Vec<PathBuf> = fs::read_dir(&dir).into_iter().flatten().flatten().map(|f| f.path()).filter(|p| p.extension().is_some_and(|e| e == "txt")).collect();
        files.sort();
        for old in files.iter().rev().skip(KEPT) {
            let _ = fs::remove_file(old);
        }
    }

    fn saves_dir(&self) -> PathBuf {
        self.dir.join("saves")
    }

    /// Writes the world. The automatic save of a world is one file that is overwritten;
    /// each manual save is a new file.
    pub fn save_game(&self, world_id: &str, world: &World, manual: bool) -> io::Result<SaveMeta> {
        let meta = SaveMeta { world_id: world_id.to_string(), title: world.title.clone(), place: world.here().name.clone(), saved_at: unix_now(), manual };
        let name = if manual { format!("manual-{}.json", meta.saved_at) } else { "auto.json".to_string() };
        let file = SaveFile { meta: meta.clone(), world: world.clone() };
        write_whole(&self.saves_dir().join(world_id).join(name), serde_json::to_string(&file)?.as_bytes())?;
        Ok(meta)
    }

    /// Every readable save, newest first. A file that cannot be read is left out, not fatal.
    pub fn list_saves(&self) -> Vec<SaveEntry> {
        let mut saves = Vec::new();
        for world_dir in fs::read_dir(self.saves_dir()).into_iter().flatten().flatten() {
            for file in fs::read_dir(world_dir.path()).into_iter().flatten().flatten() {
                let path = file.path();
                if path.extension().is_none_or(|e| e != "json") {
                    continue;
                }
                if let Some(save) = fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str::<SaveFile>(&s).ok()) {
                    saves.push(SaveEntry { path, meta: save.meta });
                }
            }
        }
        saves.sort_by(|a, b| b.meta.saved_at.cmp(&a.meta.saved_at).then_with(|| a.path.cmp(&b.path)));
        saves
    }

    pub fn load_game(&self, path: &Path) -> io::Result<(SaveMeta, World)> {
        let file: SaveFile = serde_json::from_str(&fs::read_to_string(path)?)?;
        Ok((file.meta, file.world))
    }

    /// Spent quotas and model quirks learned in earlier sessions; empty when there is no file.
    pub fn load_quota(&self) -> SavedState {
        fs::read_to_string(self.dir.join("quota.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save_quota(&self, state: &SavedState) -> io::Result<()> {
        write_whole(&self.dir.join("quota.json"), serde_json::to_string_pretty(state)?.as_bytes())
    }

    pub fn save_keys(&self, keys: &[String]) -> io::Result<()> {
        let mut file = KeyFile::default();
        for key in keys {
            file.keys.push(secret::to_hex(&secret::protect(key.as_bytes())?));
        }
        write_whole(&self.dir.join("keys.json"), serde_json::to_string_pretty(&file)?.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(name: &str) -> Store {
        let dir = std::env::temp_dir().join(format!("pixelgm-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        Store::at(dir)
    }

    #[test]
    fn settings_survive_a_restart() {
        let store = temp_store("settings");
        let settings = Settings { lang: Lang::En, fullscreen: true, thinking: ThinkingSetting::Off, lowest_tier: Tier::Flash, predraw: false, initiative: Initiative::Often, memory_budget: 9_000 };
        store.save_settings(&settings).unwrap();
        assert_eq!(store.load_settings(), settings);
    }

    #[test]
    fn missing_and_broken_settings_give_defaults() {
        let store = temp_store("broken");
        assert_eq!(store.load_settings(), Settings::default());
        fs::create_dir_all(&store.dir).unwrap();
        fs::write(store.dir.join("settings.json"), "{ not json").unwrap();
        assert_eq!(store.load_settings(), Settings::default());
    }

    #[test]
    fn settings_from_an_older_version_keep_what_they_have() {
        let store = temp_store("older");
        fs::create_dir_all(&store.dir).unwrap();
        fs::write(store.dir.join("settings.json"), r#"{ "lang": "En" }"#).unwrap();
        assert_eq!(store.load_settings(), Settings { lang: Lang::En, ..Settings::default() });
    }

    #[test]
    fn keys_survive_a_restart_and_are_not_stored_in_the_clear() {
        let store = temp_store("keys");
        let keys = vec!["AIza-first".to_string(), "AQ.second".to_string()];
        store.save_keys(&keys).unwrap();
        assert_eq!(store.load_keys(), keys);
        if cfg!(windows) {
            let on_disk = fs::read_to_string(store.dir.join("keys.json")).unwrap();
            assert!(!on_disk.contains("AIza-first"));
        }
    }

    fn test_world() -> World {
        let draft: world::WorldDraft = serde_json::from_str(&world::fake::world("saves")).unwrap();
        world::build_world(&draft, "saves", "English").unwrap().0
    }

    #[test]
    fn only_the_newest_answers_are_kept() {
        let store = temp_store("answers");
        let dir = store.dir.join("answers");
        fs::create_dir_all(&dir).unwrap();
        for n in 0..25 {
            fs::write(dir.join(format!("{:04}-old.txt", n)), "x").unwrap();
        }
        store.log_answer("gemini-9/flash", "{}");
        let mut names: Vec<String> = fs::read_dir(&dir).unwrap().flatten().map(|f| f.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names.len(), 20);
        assert!(names.last().unwrap().ends_with("-gemini-9_flash.txt"));
        assert_eq!(names[0], "0006-old.txt");
    }

    #[test]
    fn a_saved_game_loads_back_exactly() {
        let store = temp_store("game");
        let world = test_world();
        let meta = store.save_game("w1", &world, false).unwrap();
        assert_eq!((meta.title.as_str(), meta.place.as_str(), meta.manual), ("Test Hollow", "The Camp", false));
        let saves = store.list_saves();
        assert_eq!(saves.len(), 1);
        assert_eq!(store.load_game(&saves[0].path).unwrap(), (meta, world));
    }

    #[test]
    fn the_automatic_save_is_overwritten_and_manual_saves_pile_up() {
        let store = temp_store("slots");
        let world = test_world();
        store.save_game("w1", &world, false).unwrap();
        store.save_game("w1", &world, false).unwrap();
        store.save_game("w1", &world, true).unwrap();
        store.save_game("w2", &world, false).unwrap();
        let saves = store.list_saves();
        assert_eq!(saves.len(), 3);
        assert_eq!(saves.iter().filter(|s| s.meta.manual).count(), 1);
        assert_eq!(saves.iter().filter(|s| s.meta.world_id == "w1").count(), 2);
    }

    #[test]
    fn a_broken_save_is_left_out_of_the_list_and_no_saves_is_an_empty_list() {
        let store = temp_store("badsave");
        assert!(store.list_saves().is_empty());
        store.save_game("w1", &test_world(), false).unwrap();
        fs::write(store.dir.join("saves/w1/manual-1.json"), "{ truncated").unwrap();
        assert_eq!(store.list_saves().len(), 1);
    }

    #[test]
    fn quota_state_survives_a_restart_and_a_missing_file_is_empty() {
        let store = temp_store("quota");
        assert_eq!(store.load_quota(), SavedState::default());
        let mut state = SavedState::default();
        state.rests.push(llm::SavedRest { key: llm::fingerprint("k"), model: "m".into(), until: 42, limit: Some(20) });
        state.thinking.insert("m".into(), llm::ThinkingMode::Budget);
        store.save_quota(&state).unwrap();
        assert_eq!(store.load_quota(), state);
    }

    #[test]
    fn a_damaged_key_entry_is_dropped_and_the_rest_load() {
        let store = temp_store("damaged");
        store.save_keys(&["good".to_string()]).unwrap();
        let path = store.dir.join("keys.json");
        let mut file: KeyFile = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        file.keys.insert(0, "nothex".into());
        fs::write(&path, serde_json::to_string(&file).unwrap()).unwrap();
        assert_eq!(store.load_keys(), ["good"]);
    }
}
