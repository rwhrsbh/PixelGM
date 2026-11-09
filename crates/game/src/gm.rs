//! The game's side of Gemini access: the shared dispatcher, the player's keys and their
//! health. Network calls run on their own threads and report back through a queue, so a slow
//! answer never freezes the picture.

use crate::i18n::{Lang, T, wait_text};
use crate::store::{Settings, Store};
use bevy::prelude::*;
use llm::{Dispatcher, GenError, GenRequest, KeyHealth, KeyId, SavedState, UreqTransport};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Health {
    Checking,
    Working,
    Rejected,
    Unreachable,
}

#[derive(Clone, Debug)]
pub struct KeyEntry {
    pub id: KeyId,
    pub secret: String,
    pub health: Health,
}

/// The keys as the interface shows them.
#[derive(Resource, Default)]
pub struct KeyBook {
    pub entries: Vec<KeyEntry>,
}

/// Shows enough of a key to tell two apart, never the whole of it.
pub fn masked(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    if chars.len() <= 10 {
        return "…".repeat(3);
    }
    format!("{}…{}", chars[..4].iter().collect::<String>(), chars[chars.len() - 4..].iter().collect::<String>())
}

/// Why the game master did not answer, in the terms a player can act on.
#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Trouble {
    NoKeys,
    Exhausted { retry_at: u64 },
    Network,
    Blocked(String),
    Rejected(String),
    /// A model answered, but not with anything that could be used.
    Unreadable(String),
}

/// A sentence for the player about what went wrong.
pub fn trouble_text(trouble: &Trouble, now: u64, lang: Lang) -> String {
    match trouble {
        Trouble::NoKeys => T::TestNoKeys.text(lang).to_string(),
        Trouble::Exhausted { retry_at } => format!("{} {}.", T::TestExhausted.text(lang), wait_text(retry_at.saturating_sub(now), lang)),
        Trouble::Network => T::TestNetwork.text(lang).to_string(),
        Trouble::Blocked(reason) => format!("{} {reason}", T::TestBlocked.text(lang)),
        Trouble::Rejected(reason) => format!("{} {reason}", T::TestRejected.text(lang)),
        Trouble::Unreadable(reason) => format!("{} {reason}", T::TestUnreadable.text(lang)),
    }
}

/// One real request through the rotation, started from the key screen, so the player can see
/// that the game master answers with the current keys and settings before starting a game.
#[derive(Resource, Clone, PartialEq, Debug, Default)]
pub enum GmTest {
    #[default]
    Idle,
    Running,
    Answered {
        model: String,
        seconds: f32,
        text: String,
    },
    Failed(Trouble),
}

impl From<GenError> for Trouble {
    fn from(error: GenError) -> Self {
        match error {
            GenError::NoKeys => Trouble::NoKeys,
            GenError::Exhausted { retry_at } => Trouble::Exhausted { retry_at },
            GenError::Network(_) => Trouble::Network,
            GenError::Blocked(reason) => Trouble::Blocked(reason),
            GenError::BadRequest(reason) => Trouble::Rejected(reason),
        }
    }
}

enum Report {
    Probed(KeyId, KeyHealth),
    ChainRefreshed,
    Tested(GmTest),
}

#[derive(Resource, Clone)]
pub struct Gm {
    pub dispatcher: Arc<Dispatcher<UreqTransport>>,
    reports: Arc<Mutex<Vec<Report>>>,
}

impl Default for Gm {
    fn default() -> Self {
        // A model that takes the connection and then says nothing is only found out by
        // waiting: this is how long one may go quiet before the rotation asks another. A
        // working flash model answers a whole turn well inside it.
        Self { dispatcher: Arc::new(Dispatcher::new(UreqTransport::new(Duration::from_secs(120)))), reports: Arc::default() }
    }
}

impl Gm {
    /// Checks the key in the background and relists models, so a fresh key is usable at once.
    fn probe(&self, id: KeyId) {
        let gm = self.clone();
        std::thread::spawn(move || {
            let health = gm.dispatcher.probe_key(id);
            let working = health == KeyHealth::Working;
            gm.reports.lock().unwrap().push(Report::Probed(id, health));
            if working && gm.dispatcher.with_pool(|p| p.needs_relist()) && gm.dispatcher.refresh().is_ok() {
                gm.reports.lock().unwrap().push(Report::ChainRefreshed);
            }
        });
    }

    /// Adds a key and starts checking it. Returns false when the key is already in the book.
    pub fn add_key(&self, book: &mut KeyBook, secret: &str) -> bool {
        let secret = secret.trim();
        if secret.is_empty() || book.entries.iter().any(|e| e.secret == secret) {
            return false;
        }
        let id = self.dispatcher.add_key(secret);
        book.entries.push(KeyEntry { id, secret: secret.to_string(), health: Health::Checking });
        self.probe(id);
        true
    }

    pub fn remove_key(&self, book: &mut KeyBook, id: KeyId) {
        self.dispatcher.remove_key(id);
        book.entries.retain(|e| e.id != id);
    }

    /// Sends one greeting request with the player's thinking level and fallback tier. A test
    /// already running is left to finish: each one costs a turn of quota.
    pub fn test(&self, test: &mut GmTest, settings: &Settings) {
        if *test == GmTest::Running {
            return;
        }
        *test = GmTest::Running;
        let language = match settings.lang {
            Lang::Ru => "Russian",
            Lang::En => "English",
        };
        let mut request = GenRequest::prompt(format!("Greet the players who have just sat down at your table. One short sentence, in {language}."));
        request.system = Some("You are the game master of a tabletop role-playing game.".into());
        request.thinking = settings.thinking;
        let lowest = settings.lowest_tier;
        let gm = self.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            let outcome = match gm.dispatcher.generate(&request, lowest) {
                Ok((reply, model)) => GmTest::Answered { model: model.id, seconds: started.elapsed().as_secs_f32(), text: reply.text.trim().to_string() },
                Err(error) => GmTest::Failed(error.into()),
            };
            gm.reports.lock().unwrap().push(Report::Tested(outcome));
        });
    }

    /// Checks again a key that could not be reached, for example after the network came back.
    pub fn recheck(&self, book: &mut KeyBook, id: KeyId) {
        if let Some(entry) = book.entries.iter_mut().find(|e| e.id == id) {
            entry.health = Health::Checking;
            self.probe(id);
        }
    }
}

/// The key list as it stands on disk, so only a real change rewrites the file.
#[derive(Resource, Default)]
struct KeysOnDisk(Vec<String>);

fn load_keys(gm: Res<Gm>, store: Res<Store>, mut book: ResMut<KeyBook>, mut on_disk: ResMut<KeysOnDisk>) {
    on_disk.0 = store.load_keys();
    for secret in &on_disk.0 {
        gm.add_key(&mut book, secret);
    }
    // After the keys: saved rests are matched to the keys that are present.
    gm.dispatcher.import_state(&store.load_quota());
}

const QUOTA_SAVE_PERIOD: f32 = 2.0;
/// New models appear and old ones are retired while the game runs for days.
const RELIST_PERIOD: f32 = 30.0 * 60.0;

/// Writes spent quotas down soon after they change, so closing the game does not forget them.
fn save_quota(gm: Res<Gm>, store: Res<Store>, time: Res<Time>, mut since: Local<f32>, mut saved: Local<Option<SavedState>>) {
    *since += time.delta_secs();
    if *since < QUOTA_SAVE_PERIOD {
        return;
    }
    *since = 0.0;
    let now = gm.dispatcher.export_state();
    if saved.as_ref() == Some(&now) {
        return;
    }
    // The first look only notes what was loaded; an unchanged state is not rewritten.
    if saved.is_some() || now != store.load_quota() {
        if let Err(e) = store.save_quota(&now) {
            error!("could not save quota state: {e}");
        }
    }
    *saved = Some(now);
}

fn relist_models(gm: Res<Gm>, time: Res<Time>, mut since: Local<f32>) {
    *since += time.delta_secs();
    if *since < RELIST_PERIOD {
        return;
    }
    *since = 0.0;
    let gm = gm.clone();
    std::thread::spawn(move || {
        if gm.dispatcher.refresh().is_ok() {
            gm.reports.lock().unwrap().push(Report::ChainRefreshed);
        }
    });
}

fn collect_reports(gm: Res<Gm>, mut book: ResMut<KeyBook>, mut test: ResMut<GmTest>) {
    let reports: Vec<Report> = std::mem::take(&mut *gm.reports.lock().unwrap());
    for report in reports {
        match report {
            Report::Probed(id, health) => {
                if let Some(entry) = book.entries.iter_mut().find(|e| e.id == id) {
                    entry.health = match health {
                        KeyHealth::Working => Health::Working,
                        KeyHealth::Rejected => Health::Rejected,
                        KeyHealth::Unreachable(_) => Health::Unreachable,
                    };
                }
            }
            // Nothing to copy: the chain lives in the dispatcher. Touching the book makes the
            // screens that show it redraw.
            Report::ChainRefreshed => book.set_changed(),
            Report::Tested(outcome) => {
                *test = outcome;
                // A request may have put models to rest; the meter should show it at once.
                book.set_changed();
            }
        }
    }
}

/// Saves the keys whenever the list itself changes; health changes alone do not rewrite the file.
fn save_keys(store: Res<Store>, book: Res<KeyBook>, mut on_disk: ResMut<KeysOnDisk>) {
    if !book.is_changed() {
        return;
    }
    let now: Vec<String> = book.entries.iter().map(|e| e.secret.clone()).collect();
    if on_disk.0 == now {
        return;
    }
    match store.save_keys(&now) {
        Ok(()) => on_disk.0 = now,
        Err(e) => error!("could not save keys: {e}"),
    }
}

pub struct GmPlugin;

impl Plugin for GmPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Gm>().init_resource::<KeyBook>().init_resource::<GmTest>().init_resource::<KeysOnDisk>().add_systems(Startup, load_keys).add_systems(Update, ((collect_reports, save_keys).chain(), save_quota, relist_models));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_shows_only_the_ends() {
        assert_eq!(masked("AIzaSyD_Gz-8Jx6xAzEK5kiftcxmkF1JL04io9g"), "AIza…io9g");
    }

    #[test]
    fn short_strings_are_hidden_whole() {
        assert_eq!(masked("abc"), "………");
        assert_eq!(masked("0123456789"), "………");
    }
}
