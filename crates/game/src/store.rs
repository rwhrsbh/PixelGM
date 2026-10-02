//! What the game remembers between launches: settings and the player's API keys.

use crate::i18n::Lang;
use crate::secret;
use bevy::prelude::*;
use llm::{SavedState, ThinkingSetting, Tier};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Resource, Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Settings {
    pub lang: Lang,
    pub fullscreen: bool,
    /// How much the game master's model thinks before answering.
    pub thinking: ThinkingSetting,
    /// The weakest tier the game master may fall back to when better models are spent.
    pub lowest_tier: Tier,
}

impl Default for Settings {
    fn default() -> Self {
        Self { lang: Lang::Ru, fullscreen: false, thinking: ThinkingSetting::Medium, lowest_tier: Tier::Gemma }
    }
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
        let settings = Settings { lang: Lang::En, fullscreen: true, thinking: ThinkingSetting::Off, lowest_tier: Tier::Flash };
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
