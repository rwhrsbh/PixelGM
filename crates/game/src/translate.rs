//! A world is told in one language: the host's, when it was made. A player who reads
//! another has what the master wrote translated on their own machine, with their own keys:
//! tellings, choices, places and things. A translation is asked for once and kept, so a
//! sheet first appears as it was written and changes when its translation arrives.

use crate::gm::Gm;
use crate::i18n::Lang;
use crate::master::{Ask, Master, language_name};
use crate::menu::Screen;
use crate::play::Game;
use crate::store::{Settings, Store};
use bevy::prelude::*;
use std::collections::HashMap;
use world::World;

/// As much as one request carries.
const BATCH_ITEMS: usize = 40;
const BATCH_CHARS: usize = 6_000;
/// A moment to let the texts of one sheet gather into one request.
const GATHER_SECONDS: f32 = 0.4;
/// After a refusal (no keys, quota spent) the master is left alone for a while.
const REST_SECONDS: f32 = 600.0;
const MOST_WANTED: usize = 400;

#[derive(Resource, Default)]
pub struct Translator {
    /// The language the cache holds translations into.
    lang: Option<Lang>,
    cache: HashMap<String, String>,
    wanted: Vec<String>,
    asked: bool,
    rest: f32,
    since: f32,
}

/// Whether this player reads another language than the world is told in.
pub fn foreign(world: &World, lang: Lang) -> bool {
    !world.language.is_empty() && world.language != language_name(lang)
}

impl Translator {
    /// The text as the player should read it: its translation when there is one; otherwise
    /// as written, and the translation is asked for.
    pub fn read(&mut self, foreign: bool, text: &str) -> String {
        if !foreign || text.trim().is_empty() {
            return text.to_string();
        }
        if let Some(known) = self.cache.get(text) {
            return known.clone();
        }
        if self.wanted.len() < MOST_WANTED && !self.wanted.iter().any(|w| w == text) {
            self.wanted.push(text.to_string());
            self.since = 0.0;
        }
        text.to_string()
    }

    /// The next texts to send, as many as one request carries. They stay wanted until the
    /// answer comes.
    fn batch(&self) -> Vec<String> {
        let mut chars = 0;
        self.wanted
            .iter()
            .take(BATCH_ITEMS)
            .take_while(|text| {
                chars += text.chars().count();
                chars <= BATCH_CHARS || chars == text.chars().count()
            })
            .cloned()
            .collect()
    }

    /// Takes an answer: each original with its translation.
    pub fn learn(&mut self, originals: &[String], translated: Vec<String>) {
        self.asked = false;
        for (original, translation) in originals.iter().zip(translated) {
            if !translation.trim().is_empty() {
                self.cache.insert(original.clone(), translation);
            }
        }
        self.wanted.retain(|w| !originals.contains(w));
    }

    /// The master would not translate: what was wanted is forgotten, and nothing is asked
    /// for a while.
    pub fn refused(&mut self) {
        self.asked = false;
        self.wanted.clear();
        self.rest = REST_SECONDS;
    }

    pub fn cache(&self) -> &HashMap<String, String> {
        &self.cache
    }
}

fn pump(time: Res<Time>, mut translator: ResMut<Translator>, game: Res<Game>, gm: Res<Gm>, store: Res<Store>, settings: Res<Settings>, master: Res<Master>) {
    let translator = translator.bypass_change_detection();
    if translator.lang != Some(settings.lang) {
        *translator = Translator { lang: Some(settings.lang), cache: store.load_translations(settings.lang), ..Translator::default() };
    }
    let dt = time.delta_secs();
    translator.rest = (translator.rest - dt).max(0.0);
    translator.since += dt;
    if translator.asked || translator.rest > 0.0 || translator.wanted.is_empty() || translator.since < GATHER_SECONDS || !foreign(&game.world, settings.lang) {
        return;
    }
    translator.asked = true;
    master.ask(&gm, &store, &settings, Ask::Translate { texts: translator.batch(), language: language_name(settings.lang).to_string() });
}

pub struct TranslatePlugin;

impl Plugin for TranslatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Translator>().add_systems(Update, pump.run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_is_shown_as_written_until_its_translation_is_learned() {
        let mut t = Translator::default();
        assert_eq!(t.read(false, "Hello"), "Hello");
        assert!(t.wanted.is_empty(), "a player who reads the world's language asks for nothing");
        assert_eq!(t.read(true, "Hello"), "Hello");
        assert_eq!(t.read(true, "Hello"), "Hello");
        assert_eq!(t.read(true, "  "), "  ");
        assert_eq!(t.wanted, ["Hello"], "asked for once");
        let batch = t.batch();
        t.learn(&batch, vec!["Привет".into()]);
        assert_eq!(t.read(true, "Hello"), "Привет");
        assert!(t.wanted.is_empty());
    }

    #[test]
    fn a_batch_is_bounded_and_a_single_long_text_still_goes() {
        let mut t = Translator::default();
        for n in 0..100 {
            t.read(true, &format!("text {n}"));
        }
        assert_eq!(t.batch().len(), BATCH_ITEMS);
        let mut long = Translator::default();
        long.read(true, &"x".repeat(BATCH_CHARS + 10));
        long.read(true, "short");
        assert_eq!(long.batch().len(), 1);
    }

    #[test]
    fn a_refusal_forgets_what_was_wanted_and_an_empty_translation_is_not_kept() {
        let mut t = Translator::default();
        t.read(true, "a");
        t.refused();
        assert!(t.wanted.is_empty() && t.rest > 0.0);
        t.read(true, "b");
        t.learn(&["b".to_string()], vec![" ".into()]);
        assert_eq!(t.read(true, "b"), "b");
    }

    #[test]
    fn only_a_player_of_another_language_needs_translating() {
        let draft: world::WorldDraft = serde_json::from_str(&world::fake::world("t")).unwrap();
        let w = world::build_world(&draft, "t", "Russian").unwrap().0;
        assert!(!foreign(&w, Lang::Ru));
        assert!(foreign(&w, Lang::En));
    }
}
