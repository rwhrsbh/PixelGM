//! What the game remembers between launches: settings and the player's API keys.

use crate::i18n::Lang;
use crate::secret;
use bevy::prelude::*;
use llm::{SavedState, ThinkingSetting, Tier};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use world::{CreatureCard, World};

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
    /// The name this player goes by at a shared table; the account name when empty.
    pub player_name: String,
    /// Loudness of the music and of the effects, each from 0 (off) to 4.
    pub music: u8,
    pub sound: u8,
    /// The master reads tellings aloud.
    pub voice: bool,
    /// The colour grade and the effects laid over the picture.
    pub grade: crate::post::Grade,
    pub gfx: crate::post::Gfx,
    /// The window's size: 0 lets the screen choose, otherwise one of `screen::SIZES` from 1.
    pub window_size: u8,
    /// The picture fills the window at any scale, with no bars, rather than keeping every
    /// pixel the same size.
    pub fill: bool,
    /// The game has been put in fullscreen once: after that the player's choice stands.
    pub fullscreen_set: bool,
    /// How tall the window of the telling is: 0, 1 or 2.
    pub narration: u8,
    /// How near the map is drawn: a place in `view::ZOOMS`.
    pub zoom: u8,
    /// Fetch music from the network as it is wanted; off, only what is on disk plays.
    pub online_music: bool,
    /// A service of one's own speaking the OpenAI protocol, used instead of Gemini when on.
    /// The masters other than Gemini: NVIDIA NIM, OpenRouter, Cloudflare, one's own.
    pub providers: Vec<ProviderSetting>,
    pub custom_on: bool,
    pub custom_url: String,
    pub custom_model: String,
    /// The key, sealed to this user of this machine, in hex.
    pub custom_key: String,
}

/// One provider as it is kept: its keys sealed, its models in the order they are tried.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(default)]
pub struct ProviderSetting {
    pub kind: llm::providers::Kind,
    pub on: bool,
    /// Sealed, as hex.
    pub keys: Vec<String>,
    pub models: Vec<String>,
    /// Cloudflare's account id.
    pub account: String,
    /// The address of a service of the player's own.
    pub url: String,
    /// The models that speak, for the narrator, in the order they are tried.
    pub voices: Vec<String>,
    /// The voice asked for, where a model has several.
    pub voice: String,
}

impl Default for ProviderSetting {
    fn default() -> Self {
        Self { kind: llm::providers::Kind::Custom, on: false, keys: Vec::new(), models: Vec::new(), account: String::new(), url: String::new(), voices: Vec::new(), voice: String::new() }
    }
}

impl ProviderSetting {
    pub fn of(kind: llm::providers::Kind) -> Self {
        let url = if kind == llm::providers::Kind::Custom { "http://localhost:11434/v1".to_string() } else { String::new() };
        Self { kind, url, ..Self::default() }
    }

    /// The keys in the clear; those sealed on another account are left out.
    pub fn plain_keys(&self) -> Vec<String> {
        self.keys.iter().filter_map(|hex| crate::secret::from_hex(hex)).filter_map(|blob| crate::secret::unprotect(&blob).ok()).filter_map(|plain| String::from_utf8(plain).ok()).collect()
    }

    /// Seals a key and keeps it, unless it is kept already.
    pub fn add_key(&mut self, key: &str) -> bool {
        let key = key.trim();
        if key.is_empty() || self.plain_keys().iter().any(|k| k == key) {
            return false;
        }
        match crate::secret::protect(key.as_bytes()) {
            Ok(blob) => {
                self.keys.push(crate::secret::to_hex(&blob));
                true
            }
            Err(_) => false,
        }
    }

    /// As it is asked, with how hard to think.
    pub fn provider(&self, thinking: ThinkingSetting) -> llm::providers::Provider {
        let effort = match thinking {
            ThinkingSetting::Off => "",
            ThinkingSetting::Low => "low",
            ThinkingSetting::Medium => "medium",
            ThinkingSetting::High => "high",
        };
        llm::providers::Provider { kind: self.kind, keys: self.plain_keys(), models: self.models.clone(), account: self.account.clone(), url: self.url.clone(), effort: effort.to_string(), voices: self.voices.clone(), voice: self.voice.clone() }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Initiative {
    Off,
    #[default]
    Rare,
    Often,
}

impl Settings {
    /// The providers that are switched on and have enough to ask with, in the order they
    /// are tried. While any is, they are the master in place of Gemini.
    pub fn providers(&self) -> Vec<llm::providers::Provider> {
        self.providers.iter().filter(|p| p.on).map(|p| p.provider(self.thinking)).filter(|p| p.ready()).collect()
    }

    /// The providers that are switched on and have a speaking model to ask.
    pub fn narrators(&self) -> Vec<llm::providers::Provider> {
        self.providers.iter().filter(|p| p.on).map(|p| p.provider(self.thinking)).filter(|p| p.voice_ready()).collect()
    }

    /// The setting of one provider, made if it was not kept yet.
    pub fn provider_mut(&mut self, kind: llm::providers::Kind) -> &mut ProviderSetting {
        if !self.providers.iter().any(|p| p.kind == kind) {
            self.providers.push(ProviderSetting::of(kind));
        }
        self.providers.iter_mut().find(|p| p.kind == kind).expect("just made")
    }

    /// What the main menu calls the master of the moment.
    pub fn master_label(&self) -> crate::i18n::T {
        use crate::i18n::T;
        use llm::providers::Kind;
        match self.active_provider() {
            None => T::MasterGemini,
            Some(Kind::Nim) => T::MasterNim,
            Some(Kind::OpenRouter) => T::MasterOpenRouter,
            Some(Kind::Cloudflare) => T::MasterCloudflare,
            Some(Kind::Custom) => T::MasterCustom,
        }
    }

    /// Makes one provider the master, or Gemini with `None`: there is one at a time.
    pub fn set_master(&mut self, kind: Option<llm::providers::Kind>) {
        for provider in &mut self.providers {
            provider.on = Some(provider.kind) == kind;
        }
    }

    /// The provider that is on, the first of them: the one whose keys are the master's.
    pub fn active_provider(&self) -> Option<llm::providers::Kind> {
        self.providers.iter().find(|p| p.on).map(|p| p.kind)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self { lang: Lang::Ru, fullscreen: true, fullscreen_set: false, thinking: ThinkingSetting::Medium, lowest_tier: Tier::Gemma, predraw: true, initiative: Initiative::Rare, memory_budget: 30_000, player_name: String::new(), music: 2, sound: 3, voice: false, grade: crate::post::Grade::default(), gfx: crate::post::Gfx::default(), window_size: 0, fill: true, narration: 2, zoom: 2, online_music: true, providers: Vec::new(), custom_on: false, custom_url: "http://localhost:11434/v1".into(), custom_model: String::new(), custom_key: String::new() }
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
        let mut settings: Settings = fs::read_to_string(self.dir.join("settings.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        // A provider of one's own kept the old way becomes one of the providers.
        if !settings.custom_model.trim().is_empty() && !settings.providers.iter().any(|p| p.kind == llm::providers::Kind::Custom) {
            let old = ProviderSetting { kind: llm::providers::Kind::Custom, on: settings.custom_on, keys: Some(settings.custom_key.clone()).filter(|k| !k.is_empty()).into_iter().collect(), models: vec![settings.custom_model.trim().to_string()], account: String::new(), url: settings.custom_url.clone(), voices: Vec::new(), voice: String::new() };
            settings.providers.push(old);
            settings.custom_model.clear();
            settings.custom_on = false;
            let _ = self.save_settings(&settings);
        }
        // One master at a time: of several switched on the old way, the first stays.
        if settings.providers.iter().filter(|p| p.on).count() > 1 {
            let first = settings.active_provider();
            settings.set_master(first);
            let _ = self.save_settings(&settings);
        }
        // The game starts in fullscreen. Settings saved before that was so are switched
        // once; a player who then goes back to a window is left with a window.
        if !settings.fullscreen_set {
            settings.fullscreen = true;
            settings.fullscreen_set = true;
            let _ = self.save_settings(&settings);
        }
        settings
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

    fn translations_file(&self, lang: Lang) -> PathBuf {
        self.dir.join(match lang {
            Lang::Ru => "translations.ru.json",
            Lang::En => "translations.en.json",
        })
    }

    /// What the master has already translated into the player's language.
    pub fn load_translations(&self, lang: Lang) -> HashMap<String, String> {
        fs::read_to_string(self.translations_file(lang)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save_translations(&self, lang: Lang, known: &HashMap<String, String>) -> io::Result<()> {
        write_whole(&self.translations_file(lang), serde_json::to_string(known)?.as_bytes())
    }

    fn library_dir(&self) -> PathBuf {
        self.dir.join("library")
    }

    /// Keeps a creature in this player's own library, under its id. The library belongs to
    /// the player, not to a world: a master brings its creatures into any game.
    pub fn save_card(&self, card: &CreatureCard) -> io::Result<()> {
        let name: String = card.id.chars().map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
        let name = if name.is_empty() { format!("creature-{}", unix_now()) } else { name };
        write_whole(&self.library_dir().join(format!("{name}.json")), serde_json::to_string_pretty(card)?.as_bytes())
    }

    /// The library, by name. A card that cannot be read is left out.
    pub fn load_cards(&self) -> Vec<CreatureCard> {
        let mut cards: Vec<CreatureCard> =
            fs::read_dir(self.library_dir()).into_iter().flatten().flatten().filter(|f| f.path().extension().is_some_and(|e| e == "json")).filter_map(|f| fs::read_to_string(f.path()).ok()).filter_map(|s| serde_json::from_str(&s).ok()).collect();
        cards.sort_by(|a, b| a.name.cmp(&b.name));
        cards
    }

    /// Puts what is needed to look into a problem in one new folder: the logs, the world
    /// being played and its saves, the settings, the spent quotas and the master's latest
    /// answers. Keys are never part of it.
    pub fn collect_report(&self, world_id: &str, world: Option<&World>) -> io::Result<PathBuf> {
        let report = self.dir.join("reports").join(format!("report-{}", unix_now()));
        fs::create_dir_all(&report)?;
        let copy_dir = |from: PathBuf, to: PathBuf| -> io::Result<()> {
            for file in fs::read_dir(from).into_iter().flatten().flatten().filter(|f| f.path().is_file()) {
                fs::create_dir_all(&to)?;
                fs::copy(file.path(), to.join(file.file_name()))?;
            }
            Ok(())
        };
        copy_dir(self.dir.join("logs"), report.join("logs"))?;
        copy_dir(self.dir.join("answers"), report.join("answers"))?;
        for name in ["settings.json", "quota.json"] {
            if self.dir.join(name).is_file() {
                fs::copy(self.dir.join(name), report.join(name))?;
            }
        }
        // The world as it is this moment, not as it was last saved.
        if let Some(world) = world {
            fs::write(report.join("world.json"), serde_json::to_string(world)?)?;
        }
        copy_dir(self.saves_dir().join(world_id), report.join("saves"))?;
        Ok(report)
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
        let settings = Settings { lang: Lang::En, fullscreen: true, fullscreen_set: true, thinking: ThinkingSetting::Off, lowest_tier: Tier::Flash, predraw: false, initiative: Initiative::Often, memory_budget: 9_000, player_name: "Ann".into(), music: 0, sound: 4, voice: true, grade: crate::post::Grade::Crt, gfx: crate::post::Gfx { glass: true, ..Default::default() }, window_size: 3, fill: false, narration: 2, zoom: 1, online_music: false, providers: Vec::new(), custom_on: true, custom_url: "http://localhost:1234/v1".into(), custom_model: "qwen".into(), custom_key: String::new() };
        store.save_settings(&settings).unwrap();
        // A provider of one's own kept the old way comes back as one of the providers,
        // with its address, its model and its key; the rest is as it was saved.
        let loaded = store.load_settings();
        let own = loaded.providers.iter().find(|p| p.kind == llm::providers::Kind::Custom).expect("carried over");
        assert_eq!((own.on, own.url.as_str(), own.models.len(), own.keys.len()), (settings.custom_on, settings.custom_url.as_str(), 1, usize::from(!settings.custom_key.is_empty())));
        assert_eq!(own.models[0], settings.custom_model);
        assert_eq!(Settings { providers: Vec::new(), custom_model: settings.custom_model.clone(), custom_on: settings.custom_on, ..loaded.clone() }, settings);
        // Keys are sealed, take turns in the order added, and a key is kept once.
        let mut nim = ProviderSetting::of(llm::providers::Kind::Nim);
        assert!(nim.add_key("nvapi-one") && nim.add_key(" nvapi-two ") && !nim.add_key("nvapi-one"));
        assert_eq!(nim.plain_keys(), ["nvapi-one", "nvapi-two"]);
        assert!(nim.keys.iter().all(|k| !k.contains("nvapi")));
        // Switched on with a model, it is the master; with none it is not asked.
        let mut with = Settings::default();
        with.providers.push(ProviderSetting { on: true, ..nim.clone() });
        assert!(with.providers().is_empty());
        with.providers[0].models.push("meta/llama-3.3-70b-instruct".into());
        assert_eq!(with.providers().len(), 1);
        assert_eq!((with.providers()[0].effort.as_str(), with.active_provider()), ("medium", Some(llm::providers::Kind::Nim)));
    }

    #[test]
    fn missing_and_broken_settings_give_defaults() {
        let store = temp_store("broken");
        assert_eq!(store.load_settings(), Settings { fullscreen_set: true, ..Settings::default() });
        fs::create_dir_all(&store.dir).unwrap();
        fs::write(store.dir.join("settings.json"), "{ not json").unwrap();
        assert_eq!(store.load_settings(), Settings { fullscreen_set: true, ..Settings::default() });
    }

    #[test]
    fn settings_from_an_older_version_keep_what_they_have() {
        let store = temp_store("older");
        fs::create_dir_all(&store.dir).unwrap();
        fs::write(store.dir.join("settings.json"), r#"{ "lang": "En" }"#).unwrap();
        // What was saved before the game started in fullscreen is switched to it, once.
        assert_eq!(store.load_settings(), Settings { lang: Lang::En, fullscreen: true, fullscreen_set: true, ..Settings::default() });
        let windowed = Settings { fullscreen: false, ..store.load_settings() };
        store.save_settings(&windowed).unwrap();
        assert!(!store.load_settings().fullscreen, "a player who went back to a window keeps the window");
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

    #[test]
    fn the_library_keeps_creatures_by_id_and_lists_them_by_name() {
        let store = temp_store("library");
        assert!(store.load_cards().is_empty());
        let troll = CreatureCard { id: "bog_troll".into(), name: "Bog troll".into(), rows: vec!["11".into()], ..CreatureCard::default() };
        let ant = CreatureCard { id: "ant".into(), name: "Ant".into(), ..CreatureCard::default() };
        store.save_card(&troll).unwrap();
        store.save_card(&ant).unwrap();
        // Saving the same id again replaces the card.
        store.save_card(&CreatureCard { description: "Wet.".into(), ..troll.clone() }).unwrap();
        let cards = store.load_cards();
        assert_eq!(cards.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Ant", "Bog troll"]);
        assert_eq!(cards[1].description, "Wet.");
    }


    #[test]
    fn a_report_gathers_logs_answers_and_settings_but_never_keys() {
        let store = temp_store("report");
        store.log_answer("model-x", "{}");
        store.save_settings(&Settings::default()).unwrap();
        store.save_keys(&["AIzaSecretSecretSecret".to_string()]).unwrap();
        fs::create_dir_all(store.dir.join("logs")).unwrap();
        fs::write(store.dir.join("logs").join("pixelgm.log"), "hello").unwrap();
        let report = store.collect_report("w1", None).unwrap();
        assert!(report.join("logs").join("pixelgm.log").is_file());
        assert!(report.join("settings.json").is_file());
        assert_eq!(fs::read_dir(report.join("answers")).unwrap().count(), 1);
        assert!(!report.join("keys.json").exists());
    }

}
