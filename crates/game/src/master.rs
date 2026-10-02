//! Asking the game master to draw: a new world, or one more place in the current one. The
//! request runs on its own thread; the answer is read, repaired and handed back through a
//! queue, so the picture never freezes while a model thinks.

use crate::gm::{Gm, Trouble};
use crate::i18n::Lang;
use crate::store::Settings;
use bevy::prelude::*;
use llm::{GenRequest, ThinkingSetting, Tier};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use world::{LocationDraft, World, WorldDraft, build_world, extract_json, fake, prompt};

pub enum Ask {
    NewWorld { request: String },
    /// Draw the stub `id`; the party walks in from `from`. The world is a snapshot for the prompt.
    Location { world: Box<World>, id: String, from: String },
}

pub enum Answer {
    World(Result<(Box<World>, Vec<String>), Trouble>),
    Location { id: String, from: String, draft: Result<Box<LocationDraft>, Trouble> },
}

/// How many times an answer that cannot be read is asked for again before giving up. Each
/// attempt costs a turn of quota, so this stays small.
const READ_ATTEMPTS: u32 = 2;

#[derive(Resource, Clone)]
pub struct Master {
    /// Answer by rule instead of asking a model: for tests and for trying the game without keys.
    pub fake: bool,
    answers: Arc<Mutex<Vec<Answer>>>,
}

pub fn language_name(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Russian",
        Lang::En => "English",
    }
}

struct Call {
    gm: Gm,
    language: &'static str,
    thinking: ThinkingSetting,
    lowest: Tier,
}

impl Call {
    /// Sends `prompt` and reads the answer as `T`, asking again when it cannot be read.
    fn read<T: serde::de::DeserializeOwned>(&self, prompt: &str) -> Result<T, Trouble> {
        let mut request = GenRequest::prompt(prompt);
        request.system = Some(prompt::system_prompt(self.language));
        request.json = true;
        request.thinking = self.thinking;
        let mut last = String::new();
        for _ in 0..READ_ATTEMPTS {
            let (reply, model) = self.gm.dispatcher.generate(&request, self.lowest)?;
            match extract_json(&reply.text).map(serde_json::from_str::<T>) {
                Some(Ok(value)) => return Ok(value),
                Some(Err(e)) => last = format!("{}: {e}", model.id),
                None => last = format!("{}: no JSON in the answer", model.id),
            }
            warn!("the game master's answer could not be read ({last})");
        }
        Err(Trouble::Unreadable(last))
    }
}

impl Master {
    pub fn new(fake: bool) -> Self {
        Self { fake, answers: Arc::default() }
    }

    pub fn ask(&self, gm: &Gm, settings: &Settings, ask: Ask) {
        let call = Call { gm: gm.clone(), language: language_name(settings.lang), thinking: settings.thinking, lowest: settings.lowest_tier };
        let (fake, answers) = (self.fake, self.answers.clone());
        std::thread::spawn(move || {
            let answer = match ask {
                Ask::NewWorld { request } => {
                    let draft: Result<WorldDraft, Trouble> = if fake {
                        std::thread::sleep(Duration::from_millis(600));
                        Ok(serde_json::from_str(&fake::world(&request)).expect("the fake master writes valid JSON"))
                    } else {
                        call.read(&prompt::new_world(&request))
                    };
                    Answer::World(draft.and_then(|d| build_world(&d, &request, call.language).map(|(w, notes)| (Box::new(w), notes)).map_err(Trouble::Unreadable)))
                }
                Ask::Location { world, id, from } => {
                    let draft: Result<LocationDraft, Trouble> = if fake {
                        std::thread::sleep(Duration::from_millis(600));
                        let name = world.place_name(&id).to_string();
                        Ok(serde_json::from_str(&fake::location(&id, &name, &from)).expect("the fake master writes valid JSON"))
                    } else {
                        call.read(&prompt::new_location(&world, &id, &from))
                    };
                    Answer::Location { id, from, draft: draft.map(Box::new) }
                }
            };
            answers.lock().unwrap().push(answer);
        });
    }

    /// The answers that arrived since the last call.
    pub fn take(&self) -> Vec<Answer> {
        std::mem::take(&mut *self.answers.lock().unwrap())
    }
}
