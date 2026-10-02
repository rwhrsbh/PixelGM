//! Asking the game master: to draw a new world or one more place, to tell what happens when
//! the party acts, to make something happen unprompted, to write the story up as a chapter.
//! Each request runs on its own thread; the answer is read and handed back through a queue,
//! so the picture never freezes while a model thinks.

use crate::gm::{Gm, Trouble};
use crate::i18n::Lang;
use crate::store::{Settings, Store};
use bevy::prelude::*;
use llm::{GenRequest, ThinkingSetting, Tier};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use world::story::KEPT_LINES;
use world::{ChapterDraft, LocationDraft, TurnDraft, World, WorldDraft, build_world, extract_json, fake, prompt};

pub enum Ask {
    NewWorld {
        request: String,
    },
    /// Draw the stub `id`; the party walks in from `from`. `ahead` marks a place drawn before
    /// anyone asked to go there: such a request may only use the best models, because it is
    /// a convenience and must not eat the quota the story itself needs.
    Location {
        world: Box<World>,
        id: String,
        from: String,
        ahead: bool,
    },
    /// The party does something with no prepared outcome.
    Turn {
        world: Box<World>,
        said: String,
        near: Option<String>,
    },
    /// The master may make something happen of its own accord.
    Event {
        world: Box<World>,
    },
    /// The older story is to be written up.
    Chapter {
        world: Box<World>,
    },
}

pub enum Answer {
    World(Result<(Box<World>, Vec<String>), Trouble>),
    Location { id: String, from: String, ahead: bool, draft: Result<Box<LocationDraft>, Trouble> },
    Turn { said: String, near: Option<String>, draft: Result<Box<TurnDraft>, Trouble> },
    Event(Result<Box<TurnDraft>, Trouble>),
    Chapter(Result<ChapterDraft, Trouble>),
}

/// How many times an answer that cannot be read is asked for again before giving up. Each
/// attempt costs a turn of quota, so this stays small.
const READ_ATTEMPTS: u32 = 2;
/// How long the fake master pretends to think, so that waiting screens can be seen.
const FAKE_THINKING: Duration = Duration::from_millis(600);

#[derive(Resource, Clone)]
pub struct Master {
    /// Answer by rule instead of asking a model: for tests and for trying the game without keys.
    pub fake: bool,
    answers: Arc<Mutex<Vec<Answer>>>,
    /// Places being drawn right now, so that none is asked for twice.
    drawing: Arc<Mutex<BTreeSet<String>>>,
}

pub fn language_name(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Russian",
        Lang::En => "English",
    }
}

struct Call {
    gm: Gm,
    store: Store,
    thinking: ThinkingSetting,
    lowest: Tier,
    attempts: u32,
}

impl Call {
    /// Sends a request and reads the answer as `T`, asking again when it cannot be read.
    fn read<T: serde::de::DeserializeOwned>(&self, system: String, prompt: &str) -> Result<T, Trouble> {
        let mut request = GenRequest::prompt(prompt);
        request.system = Some(system);
        request.json = true;
        request.thinking = self.thinking;
        let mut last = String::new();
        for _ in 0..self.attempts {
            let (reply, model) = self.gm.dispatcher.generate(&request, self.lowest)?;
            self.store.log_answer(&model.id, &reply.text);
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

fn faked<T: serde::de::DeserializeOwned>(json: String) -> Result<T, Trouble> {
    std::thread::sleep(FAKE_THINKING);
    Ok(serde_json::from_str(&json).expect("the fake master writes valid JSON"))
}

impl Master {
    pub fn new(fake: bool) -> Self {
        Self { fake, answers: Arc::default(), drawing: Arc::default() }
    }

    /// Whether the place `id` is being drawn right now.
    pub fn is_drawing(&self, id: &str) -> bool {
        self.drawing.lock().unwrap().contains(id)
    }

    pub fn is_drawing_anything(&self) -> bool {
        !self.drawing.lock().unwrap().is_empty()
    }

    pub fn ask(&self, gm: &Gm, store: &Store, settings: &Settings, ask: Ask) {
        // Work nobody is waiting for uses only the best tier and is not retried.
        let unhurried = matches!(ask, Ask::Location { ahead: true, .. } | Ask::Event { .. });
        let call = Call {
            gm: gm.clone(),
            store: store.clone(),
            thinking: settings.thinking,
            lowest: if unhurried { Tier::Flash } else { settings.lowest_tier },
            attempts: if unhurried { 1 } else { READ_ATTEMPTS },
        };
        let language = language_name(settings.lang);
        let (fake, answers, drawing) = (self.fake, self.answers.clone(), self.drawing.clone());
        if let Ask::Location { id, .. } = &ask {
            drawing.lock().unwrap().insert(id.clone());
        }
        std::thread::spawn(move || {
            let answer = match ask {
                Ask::NewWorld { request } => {
                    let draft: Result<WorldDraft, Trouble> = if fake { faked(fake::world(&request)) } else { call.read(prompt::system_prompt(language), &prompt::new_world(&request)) };
                    Answer::World(draft.and_then(|d| build_world(&d, &request, language).map(|(w, notes)| (Box::new(w), notes)).map_err(Trouble::Unreadable)))
                }
                Ask::Location { world, id, from, ahead } => {
                    let draft: Result<LocationDraft, Trouble> = if fake {
                        faked(fake::location(&id, world.place_name(&id), &from))
                    } else {
                        call.read(prompt::system_prompt(&world.language), &prompt::new_location(&world, &id, &from))
                    };
                    drawing.lock().unwrap().remove(&id);
                    Answer::Location { id, from, ahead, draft: draft.map(Box::new) }
                }
                Ask::Turn { world, said, near } => {
                    let draft: Result<TurnDraft, Trouble> = if fake { faked(fake::turn(&said)) } else { call.read(prompt::play_prompt(&world), &prompt::turn(&world, &said, near.as_deref())) };
                    Answer::Turn { said, near, draft: draft.map(Box::new) }
                }
                Ask::Event { world } => {
                    let draft: Result<TurnDraft, Trouble> =
                        if fake { faked(fake::turn("(nothing)")) } else { call.read(prompt::play_prompt(&world), &prompt::event(&world)) };
                    Answer::Event(draft.map(Box::new))
                }
                Ask::Chapter { world } => {
                    let lines = world.story.log.len().saturating_sub(KEPT_LINES);
                    Answer::Chapter(if fake { faked(fake::chapter(lines)) } else { call.read(prompt::play_prompt(&world), &prompt::chapter(&world)) })
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
