//! Asking the game master: to draw a new world or one more place, to tell what happens when
//! the party acts, to make something happen unprompted, to write the story up as a chapter.
//! Each request runs on its own thread; the answer is read and handed back through a queue,
//! so the picture never freezes while a model thinks.

use crate::gm::{Gm, Trouble};
use crate::i18n::Lang;
use crate::store::{Settings, Store};
use bevy::prelude::*;
use llm::{GenError, GenRequest, ThinkingSetting, Tier};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use world::story::KEPT_LINES;
use world::{ChapterDraft, CreatureCard, FeatsDraft, LocationDraft, PortraitDraft, StuntDraft, TranslationDraft, TurnDraft, World, WorldDraft, build_world, extract_json, fake, prompt};

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
        /// What a human master told the model to make of it.
        direction: Option<String>,
    },
    /// Abilities of a hero's own: three to choose from, or one secret skill.
    Feats {
        world: Box<World>,
        hero: usize,
        secret: bool,
    },
    /// A hero tries something in a fight that the rules have no button for. `spell` is
    /// the spell and slot when the something is a spell the rules cannot carry out.
    Stunt {
        world: Box<World>,
        said: String,
        spell: Option<(String, u8)>,
    },
    /// Put what the master wrote into this player's language.
    Translate {
        texts: Vec<String>,
        language: String,
    },
    /// Draw a hero as the player described it.
    Portrait {
        world: Box<World>,
        hero: usize,
    },
    /// A human master asks for a creature to keep in their library.
    Invent {
        world: Box<World>,
        request: String,
        /// Whose library it is for.
        for_player: String,
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
    Card { for_player: String, card: Result<Box<CreatureCard>, Trouble> },
    Feats { hero: usize, secret: bool, feats: Result<Vec<rules::Feat>, Trouble> },
    Stunt { spell: Option<(String, u8)>, draft: Result<Box<StuntDraft>, Trouble> },
    Portrait { hero: usize, rows: Result<Vec<String>, Trouble> },
    Translated { texts: Vec<String>, translated: Result<Vec<String>, Trouble> },
}

/// How many times an answer that cannot be read is asked for again before giving up. Each
/// attempt costs a turn of quota, so this stays small.
const READ_ATTEMPTS: u32 = 2;
/// How long the fake master pretends to think, so that waiting screens can be seen.
const FAKE_THINKING: Duration = Duration::from_millis(600);

/// A request the host's own keys could not serve, handed to a guest. The guest's key never
/// leaves the guest's machine: the guest asks the model and sends back what it said.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Relayed {
    pub id: u64,
    pub system: String,
    pub prompt: String,
    pub thinking: ThinkingSetting,
    pub lowest: Tier,
}

/// What a guest's keys brought back: the model that answered and its text.
pub type Served = Result<(String, String), Trouble>;

/// How long a relayed request waits for a guest before the host gives up on it.
const RELAY_PATIENCE: Duration = Duration::from_secs(400);

#[derive(Default)]
struct RelayState {
    next: u64,
    /// Guests at the table who could be asked.
    helpers: usize,
    asked: Vec<Relayed>,
    served: HashMap<u64, Served>,
}

/// The way from the threads that ask the master to the guests' keys and back.
#[derive(Clone, Default)]
pub struct Relay(Arc<Mutex<RelayState>>);

impl Relay {
    pub fn set_helpers(&self, helpers: usize) {
        self.0.lock().unwrap().helpers = helpers;
    }

    /// Requests waiting to be handed to a guest.
    pub fn take_asked(&self) -> Vec<Relayed> {
        std::mem::take(&mut self.0.lock().unwrap().asked)
    }

    pub fn serve(&self, id: u64, result: Served) {
        self.0.lock().unwrap().served.insert(id, result);
    }

    /// Waits for a guest to answer. `None` when there is no guest to ask, or none answered
    /// in time.
    fn ask(&self, system: &str, prompt: &str, thinking: ThinkingSetting, lowest: Tier) -> Option<Served> {
        let id = {
            let mut state = self.0.lock().unwrap();
            if state.helpers == 0 {
                return None;
            }
            state.next += 1;
            let id = state.next;
            state.asked.push(Relayed { id, system: system.to_string(), prompt: prompt.to_string(), thinking, lowest });
            id
        };
        let started = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(50));
            let mut state = self.0.lock().unwrap();
            if let Some(result) = state.served.remove(&id) {
                return Some(result);
            }
            if state.helpers == 0 || started.elapsed() > RELAY_PATIENCE {
                state.asked.retain(|a| a.id != id);
                return None;
            }
        }
    }
}

fn request_for(system: &str, prompt: &str, thinking: ThinkingSetting) -> GenRequest {
    let mut request = GenRequest::prompt(prompt);
    request.system = Some(system.to_string());
    request.json = true;
    request.thinking = thinking;
    request
}

/// A guest's side of a relayed request: asks with the guest's own keys.
pub fn serve_for_host(gm: &Gm, asked: &Relayed, fake: bool) -> Served {
    if fake {
        return Err(Trouble::NoKeys);
    }
    let (reply, model) = gm.dispatcher.generate(&request_for(&asked.system, &asked.prompt, asked.thinking), asked.lowest)?;
    Ok((model.id, reply.text))
}

#[derive(Resource, Clone)]
pub struct Master {
    /// Answer by rule instead of asking a model: for tests and for trying the game without keys.
    pub fake: bool,
    pub relay: Relay,
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
    relay: Relay,
}

impl Call {
    /// Sends a request and reads the answer as `T`, asking again when it cannot be read.
    fn read<T: serde::de::DeserializeOwned>(&self, system: String, prompt: &str) -> Result<T, Trouble> {
        let request = request_for(&system, prompt, self.thinking);
        let mut last = String::new();
        for _ in 0..self.attempts {
            let started = Instant::now();
            let (text, model) = match self.gm.dispatcher.generate(&request, self.lowest) {
                Ok((reply, model)) => {
                    info!("the master answered: {} in {:.1} s, {} characters for a prompt of {}", model.id, started.elapsed().as_secs_f32(), reply.text.chars().count(), prompt.chars().count());
                    (reply.text, model.id)
                }
                // The host's own keys are missing or spent: the guests' keys are next.
                Err(error @ (GenError::NoKeys | GenError::Exhausted { .. })) => match self.relay.ask(&system, prompt, self.thinking, self.lowest) {
                    Some(Ok((model, text))) => (text, model),
                    // What the guests' keys ran into says more than "the host has none".
                    Some(Err(trouble)) => return Err(trouble),
                    None => return Err(error.into()),
                },
                Err(error) => {
                    warn!("the master did not answer after {:.1} s: {error:?}", started.elapsed().as_secs_f32());
                    return Err(error.into());
                }
            };
            self.store.log_answer(&model, &text);
            match extract_json(&text).map(serde_json::from_str::<T>) {
                Some(Ok(value)) => return Ok(value),
                Some(Err(e)) => last = format!("{model}: {e}"),
                None => last = format!("{model}: no JSON in the answer"),
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
        Self { fake, relay: Relay::default(), answers: Arc::default(), drawing: Arc::default() }
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
        let unhurried = matches!(ask, Ask::Location { ahead: true, .. } | Ask::Event { .. } | Ask::Portrait { .. });
        let call = Call {
            gm: gm.clone(),
            store: store.clone(),
            thinking: settings.thinking,
            lowest: if unhurried { Tier::Flash } else { settings.lowest_tier },
            attempts: if unhurried { 1 } else { READ_ATTEMPTS },
            relay: self.relay.clone(),
        };
        let language = language_name(settings.lang);
        let (fake, answers, drawing) = (self.fake, self.answers.clone(), self.drawing.clone());
        info!(
            "the master is asked: {}{}",
            match &ask {
                Ask::NewWorld { .. } => "a new world".to_string(),
                Ask::Location { id, ahead, .. } => format!("the place '{id}'{}", if *ahead { " (ahead)" } else { "" }),
                Ask::Turn { said, direction, .. } => format!("a turn, {} characters said{}", said.chars().count(), if direction.is_some() { ", directed" } else { "" }),
                Ask::Event { .. } => "an event of its own".to_string(),
                Ask::Chapter { .. } => "a chapter".to_string(),
                Ask::Feats { hero, secret, .. } => format!("abilities for hero {hero}{}", if *secret { " (secret)" } else { "" }),
                Ask::Stunt { .. } => "a stunt in a fight".to_string(),
                Ask::Portrait { hero, .. } => format!("a portrait of hero {hero}"),
                Ask::Translate { texts, language } => format!("{} texts into {language}", texts.len()),
                Ask::Invent { .. } => "a creature for a library".to_string(),
            },
            if fake { " (fake master)" } else { "" }
        );
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
                Ask::Turn { world, said, near, direction } => {
                    let asked = match &direction {
                        Some(direction) => prompt::directive(&world, &said, direction),
                        None => prompt::turn(&world, &said, near.as_deref()),
                    };
                    let draft: Result<TurnDraft, Trouble> = if fake { faked(fake::turn(direction.as_deref().unwrap_or(&said))) } else { call.read(prompt::play_prompt(&world), &asked) };
                    Answer::Turn { said, near, draft: draft.map(Box::new) }
                }
                Ask::Feats { world, hero, secret } => {
                    let draft: Result<FeatsDraft, Trouble> = if fake { faked(fake::feats(secret)) } else { call.read(prompt::play_prompt(&world), &prompt::feats(&world, hero, secret)) };
                    Answer::Feats { hero, secret, feats: draft.map(|d| d.read()) }
                }
                Ask::Stunt { world, said, spell } => {
                    let draft: Result<StuntDraft, Trouble> = if fake { faked(fake::stunt(&said)) } else { call.read(prompt::play_prompt(&world), &prompt::stunt(&world, &said)) };
                    Answer::Stunt { spell, draft: draft.map(Box::new) }
                }
                Ask::Translate { texts, language } => {
                    let draft: Result<TranslationDraft, Trouble> =
                        if fake { faked(fake::translation(&texts, &language)) } else { call.read("You translate the text of a game, faithfully and naturally.".to_string(), &prompt::translate(&texts, &language)) };
                    // A list of another length cannot be matched to what was sent.
                    let translated = draft.and_then(|d| if d.texts.len() == texts.len() { Ok(d.texts) } else { Err(Trouble::Unreadable(format!("{} translations for {} texts", d.texts.len(), texts.len()))) });
                    Answer::Translated { texts, translated }
                }
                Ask::Portrait { world, hero } => {
                    let draft: Result<PortraitDraft, Trouble> = if fake { faked(fake::portrait()) } else { call.read(prompt::system_prompt(&world.language), &prompt::portrait(&world, hero)) };
                    Answer::Portrait { hero, rows: draft.map(|d| d.rows) }
                }
                Ask::Invent { world, request, for_player } => {
                    let card: Result<CreatureCard, Trouble> = if fake { faked(fake::creature(&request)) } else { call.read(prompt::play_prompt(&world), &prompt::invent(&world, &request)) };
                    Answer::Card { for_player, card: card.map(Box::new) }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_no_guest_to_ask_a_relayed_request_fails_at_once() {
        assert_eq!(Relay::default().ask("s", "p", ThinkingSetting::Off, Tier::Flash), None);
    }

    #[test]
    fn a_guest_answer_reaches_the_thread_that_asked() {
        let relay = Relay::default();
        relay.set_helpers(1);
        let asking = relay.clone();
        let waiter = std::thread::spawn(move || asking.ask("system", "prompt", ThinkingSetting::Low, Tier::Gemma));
        let asked = loop {
            if let Some(asked) = relay.take_asked().pop() {
                break asked;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!((asked.system.as_str(), asked.prompt.as_str(), asked.lowest), ("system", "prompt", Tier::Gemma));
        relay.serve(asked.id, Ok(("model".into(), "{}".into())));
        assert_eq!(waiter.join().unwrap(), Some(Ok(("model".to_string(), "{}".to_string()))));
    }

    #[test]
    fn when_the_last_guest_leaves_the_waiting_request_gives_up() {
        let relay = Relay::default();
        relay.set_helpers(1);
        let asking = relay.clone();
        let waiter = std::thread::spawn(move || asking.ask("s", "p", ThinkingSetting::Off, Tier::Flash));
        std::thread::sleep(Duration::from_millis(120));
        relay.set_helpers(0);
        assert_eq!(waiter.join().unwrap(), None);
    }
}
