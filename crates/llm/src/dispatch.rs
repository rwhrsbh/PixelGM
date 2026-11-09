//! Sends one request through the pool: tries the best free key and model, and on a failure
//! that rotation can fix moves on until something answers or nothing is left.

use crate::catalog::{ApiVersion, ListedModel, Model, Tier, build_chain, build_voice_chain};
use crate::client::{Failure, GenRequest, Reply, Speech, Transport, parse_generate, parse_list, parse_speech, speech_body};
use crate::pool::{KeyId, Millis, Outcome, Pick, Pool, SavedRest};
use crate::thinking::ThinkingMode;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_BASE: &str = "https://generativelanguage.googleapis.com";
/// A guard against a rotation that never settles; real runs end long before it.
const MAX_STEPS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenError {
    /// No usable key: none entered, or every one was rejected.
    NoKeys,
    /// Every allowed key and model is resting. `retry_at` is when the first one frees up.
    Exhausted { retry_at: Millis },
    /// The server could not be reached at all.
    Network(String),
    Blocked(String),
    BadRequest(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyHealth {
    Working,
    Rejected,
    /// The answer says nothing about the key: no network, or the service is failing.
    Unreachable(String),
}

/// What is worth remembering between launches: spent quotas and how each model takes its
/// thinking setting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedState {
    pub rests: Vec<SavedRest>,
    pub thinking: BTreeMap<String, ThinkingMode>,
}

#[derive(Default)]
struct State {
    pool: Pool,
    thinking: HashMap<String, ThinkingMode>,
    /// The models that speak, best first.
    voices: Vec<Model>,
    /// When each key may ask each voice again.
    voice_rests: HashMap<(KeyId, String), Millis>,
}

/// How long a voice is left alone after it failed for a reason of its own.
const VOICE_REST: Millis = 60_000;
const DAY: Millis = 24 * 60 * 60 * 1000;

pub struct Dispatcher<T: Transport> {
    transport: T,
    base: String,
    clock: Box<dyn Fn() -> Millis + Send + Sync>,
    state: Mutex<State>,
}

fn wall_clock() -> Millis {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as Millis).unwrap_or(0)
}

impl<T: Transport> Dispatcher<T> {
    pub fn new(transport: T) -> Self {
        Self::with_clock(transport, Box::new(wall_clock))
    }

    pub fn with_clock(transport: T, clock: Box<dyn Fn() -> Millis + Send + Sync>) -> Self {
        Self { transport, base: DEFAULT_BASE.to_string(), clock, state: Mutex::new(State { pool: Pool::new(), ..State::default() }) }
    }

    pub fn add_key(&self, secret: &str) -> KeyId {
        self.state.lock().unwrap().pool.add_key(secret)
    }

    pub fn remove_key(&self, id: KeyId) {
        self.state.lock().unwrap().pool.remove_key(id);
    }

    /// Read access to the pool, for the quota meter.
    pub fn with_pool<R>(&self, f: impl FnOnce(&Pool) -> R) -> R {
        f(&self.state.lock().unwrap().pool)
    }

    fn list(&self, api: ApiVersion, key: &str) -> Result<Vec<ListedModel>, Result<Failure, String>> {
        let mut all = Vec::new();
        let mut page: Option<String> = None;
        loop {
            let token = page.as_deref().map(|t| format!("&pageToken={t}")).unwrap_or_default();
            let url = format!("{}/{}/models?pageSize=1000{token}", self.base, api.path());
            let response = self.transport.get(&url, key).map_err(Err)?;
            let (models, next) = parse_list(&response).map_err(Ok)?;
            all.extend(models);
            match next {
                Some(n) => page = Some(n),
                None => return Ok(all),
            }
        }
    }

    pub fn export_state(&self) -> SavedState {
        let state = self.state.lock().unwrap();
        SavedState { rests: state.pool.export_rests((self.clock)()), thinking: state.thinking.iter().map(|(m, t)| (m.clone(), *t)).collect() }
    }

    /// Call after the keys are added: rests are matched to the keys present.
    pub fn import_state(&self, saved: &SavedState) {
        let mut state = self.state.lock().unwrap();
        state.pool.import_rests(&saved.rests, (self.clock)());
        state.thinking.extend(saved.thinking.iter().map(|(m, t)| (m.clone(), *t)));
    }

    /// The clock the pool's rest times are measured on.
    /// Whether there is any key that has not been found dead.
    pub fn has_keys(&self) -> bool {
        self.state.lock().unwrap().pool.live_keys().next().is_some()
    }

    pub fn now(&self) -> Millis {
        (self.clock)()
    }

    /// Asks the API whether a key is accepted, without spending generation quota.
    pub fn probe_key(&self, id: KeyId) -> KeyHealth {
        let secret = self.state.lock().unwrap().pool.live_keys().find(|(k, _)| *k == id).map(|(_, s)| s.to_string());
        let Some(secret) = secret else { return KeyHealth::Rejected };
        match self.list(ApiVersion::V1Beta, &secret) {
            Ok(_) => KeyHealth::Working,
            Err(Ok(Failure::KeyRejected)) => {
                self.state.lock().unwrap().pool.report((self.clock)(), "", id, Outcome::KeyRejected);
                KeyHealth::Rejected
            }
            Err(Ok(failure)) => KeyHealth::Unreachable(format!("{failure:?}")),
            Err(Err(unreachable)) => KeyHealth::Unreachable(unreachable),
        }
    }

    /// Rebuilds the chain from the model lists of every live key. Keys of different projects
    /// can see different models, so the chain is the union; a key that lacks a model answers
    /// 404 for it and the pool stops pairing them.
    pub fn refresh(&self) -> Result<(), GenError> {
        let keys: Vec<(KeyId, String)> = self.state.lock().unwrap().pool.live_keys().map(|(id, s)| (id, s.to_string())).collect();
        let (mut v1, mut v1beta) = (Vec::new(), Vec::new());
        let mut listed = false;
        let mut last = GenError::NoKeys;
        for (id, secret) in keys {
            let lists = self.list(ApiVersion::V1, &secret).and_then(|v1| Ok((v1, self.list(ApiVersion::V1Beta, &secret)?)));
            match lists {
                Ok((a, b)) => {
                    v1.extend(a);
                    v1beta.extend(b);
                    listed = true;
                }
                Err(Ok(Failure::KeyRejected)) => {
                    self.state.lock().unwrap().pool.report((self.clock)(), "", id, Outcome::KeyRejected);
                }
                Err(Ok(failure)) => last = GenError::Network(format!("model list failed: {failure:?}")),
                Err(Err(unreachable)) => last = GenError::Network(unreachable),
            }
        }
        if !listed {
            return Err(last);
        }
        let mut state = self.state.lock().unwrap();
        state.voices = build_voice_chain(&v1, &v1beta);
        state.pool.set_chain(build_chain(&v1, &v1beta));
        Ok(())
    }

    /// The models that can speak, as last listed.
    pub fn voices(&self) -> Vec<Model> {
        self.state.lock().unwrap().voices.clone()
    }

    /// Has `text` spoken in the named voice: the best speaking model first, each key in
    /// turn, moving on when a key's quota for a model is spent, exactly as for text.
    pub fn speak(&self, text: &str, voice: &str) -> Result<(Speech, Model), GenError> {
        if self.state.lock().unwrap().voices.is_empty() {
            self.refresh()?;
        }
        let (voices, keys): (Vec<Model>, Vec<(KeyId, String)>) = {
            let state = self.state.lock().unwrap();
            (state.voices.clone(), state.pool.live_keys().map(|(id, secret)| (id, secret.to_string())).collect())
        };
        if keys.is_empty() {
            return Err(GenError::NoKeys);
        }
        if voices.is_empty() {
            return Err(GenError::BadRequest("no model that speaks is listed for these keys".into()));
        }
        let body = speech_body(text, voice).to_string();
        let mut soonest: Option<Millis> = None;
        for model in &voices {
            for (key, secret) in &keys {
                let now = (self.clock)();
                let pair = (*key, model.id.clone());
                if let Some(until) = self.state.lock().unwrap().voice_rests.get(&pair).copied().filter(|until| *until > now) {
                    soonest = Some(soonest.map_or(until, |s| s.min(until)));
                    continue;
                }
                let url = format!("{}/{}/models/{}:generateContent", self.base, model.api.path(), model.id);
                let response = self.transport.post(&url, secret, &body).map_err(GenError::Network)?;
                let now = (self.clock)();
                let rest = match parse_speech(&response) {
                    Ok(speech) => return Ok((speech, model.clone())),
                    Err(Failure::Blocked(reason)) => return Err(GenError::Blocked(reason)),
                    Err(Failure::BadRequest(message)) => return Err(GenError::BadRequest(message)),
                    Err(Failure::RateLimited { retry_after, .. }) => retry_after.unwrap_or(VOICE_REST),
                    Err(Failure::ModelGone) => DAY,
                    Err(Failure::KeyRejected) => {
                        self.state.lock().unwrap().pool.report(now, "", *key, Outcome::KeyRejected);
                        DAY
                    }
                    Err(Failure::Overloaded | Failure::ThinkingRejected) => VOICE_REST,
                };
                self.state.lock().unwrap().voice_rests.insert(pair, now + rest);
                soonest = Some(soonest.map_or(now + rest, |s| s.min(now + rest)));
            }
        }
        Err(GenError::Exhausted { retry_at: soonest.unwrap_or((self.clock)() + VOICE_REST) })
    }

    /// Runs the request on the best model available no lower than `lowest`. Returns the answer
    /// together with the model that gave it.
    pub fn generate(&self, request: &GenRequest, lowest: Tier) -> Result<(Reply, Model), GenError> {
        self.generate_in(request, lowest, false)
    }

    /// As `generate`; with `cheap`, the lite and open models are tried before the best one.
    pub fn generate_in(&self, request: &GenRequest, lowest: Tier, cheap: bool) -> Result<(Reply, Model), GenError> {
        // Models that refused this request for what it says. What one will not answer
        // another may: the refusal is given only when every model has refused.
        let mut refused: Vec<String> = Vec::new();
        let mut why = String::new();
        // The last model that accepted the connection and then said nothing. One silent model
        // is not the whole network, so it is not reported as one until nothing has answered.
        let mut silent: Option<String> = None;
        // How many models have gone quiet in this round. Two in a row is the link failing,
        // not the models, and a long chain of them must not be walked one silence at a time.
        let mut silences = 0;
        for _ in 0..MAX_STEPS {
            if self.state.lock().unwrap().pool.needs_relist() {
                // With an old chain still in hand a failed relist is not fatal.
                if let Err(e) = self.refresh() {
                    if self.state.lock().unwrap().pool.chain().is_empty() {
                        return Err(e);
                    }
                }
            }
            let now = (self.clock)();
            let (model, key, secret, mode) = {
                let state = self.state.lock().unwrap();
                let picked = if !refused.is_empty() {
                    state.pool.pick_but(now, Tier::Gemma, &refused)
                } else if cheap {
                    state.pool.pick_cheap(now)
                } else {
                    state.pool.pick(now, lowest)
                };
                match picked {
                    Pick::Wait { .. } | Pick::Nothing if !refused.is_empty() => return Err(GenError::Blocked(why)),
                    Pick::Ready { model, key, secret } => {
                        let mode = state.thinking.get(&model.id).copied().unwrap_or_default();
                        (model, key, secret, mode)
                    }
                    Pick::Wait { until } => return Err(silent.map_or(GenError::Exhausted { retry_at: until }, GenError::Network)),
                    Pick::Nothing if state.pool.live_keys().next().is_none() => return Err(GenError::NoKeys),
                    Pick::Nothing => return Err(silent.map_or(GenError::Exhausted { retry_at: now + 60_000 }, GenError::Network)),
                }
            };
            let body = request.body(mode.config(request.thinking)).to_string();
            let url = format!("{}/{}/models/{}:generateContent", self.base, model.api.path(), model.id);
            let response = match self.transport.post(&url, &secret, &body) {
                Ok(response) => response,
                Err(error) => {
                    // No answer came back at all. A model that accepts the connection and then
                    // goes quiet is the usual cause, so it rests like an overloaded one and the
                    // next model is asked: the turn is not lost to one model's silence. Two in
                    // a row, though, say the link itself is gone.
                    silences += 1;
                    if silences >= 2 {
                        return Err(GenError::Network(error));
                    }
                    silent = Some(error);
                    self.state.lock().unwrap().pool.report(now, &model.id, key, Outcome::Silent);
                    continue;
                }
            };
            let now = (self.clock)();
            let mut state = self.state.lock().unwrap();
            let outcome = match parse_generate(&response) {
                Ok(reply) => {
                    state.pool.report(now, &model.id, key, Outcome::Ok);
                    return Ok((reply, model));
                }
                Err(Failure::ThinkingRejected) => match mode.downgrade() {
                    Some(next) => {
                        state.thinking.insert(model.id.clone(), next);
                        continue;
                    }
                    None => return Err(GenError::BadRequest("thinking config rejected with none sent".into())),
                },
                Err(Failure::Blocked(reason)) => {
                    refused.push(model.id.clone());
                    why = reason;
                    continue;
                }
                Err(Failure::BadRequest(message)) => return Err(GenError::BadRequest(message)),
                Err(Failure::RateLimited { retry_after, limit }) => Outcome::RateLimited { retry_after, limit },
                Err(Failure::Overloaded) => Outcome::Overloaded,
                Err(Failure::ModelGone) => Outcome::ModelGone,
                Err(Failure::KeyRejected) => Outcome::KeyRejected,
            };
            state.pool.report(now, &model.id, key, outcome);
        }
        Err(silent.map_or(GenError::Exhausted { retry_at: (self.clock)() + 60_000 }, GenError::Network))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::HttpResponse;
    use serde_json::{Value, json};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// What one POST looked like: model, key, and the thinking config that was sent.
    #[derive(Debug, Clone, PartialEq)]
    struct Call {
        model: String,
        key: String,
        thinking: Value,
    }

    type Script = Box<dyn Fn(&Call) -> Result<HttpResponse, String> + Send + Sync>;

    struct Fake {
        models: Vec<&'static str>,
        script: Script,
        calls: Arc<Mutex<Vec<Call>>>,
        lists: Arc<AtomicU64>,
    }

    impl Transport for Fake {
        fn get(&self, _url: &str, key: &str) -> Result<HttpResponse, String> {
            if key == "revoked" {
                return Ok(HttpResponse { status: 400, body: json!({ "error": { "message": "API key not valid." } }).to_string() });
            }
            self.lists.fetch_add(1, Ordering::SeqCst);
            // A key from a project that only has the oldest model.
            let visible = if key == "narrow" { &self.models[..1] } else { &self.models[..] };
            let models: Vec<Value> = visible.iter().map(|m| json!({ "name": format!("models/{m}"), "supportedGenerationMethods": ["generateContent"] })).collect();
            Ok(HttpResponse { status: 200, body: json!({ "models": models }).to_string() })
        }

        fn post(&self, url: &str, key: &str, body: &str) -> Result<HttpResponse, String> {
            let model = url.rsplit('/').next().unwrap().trim_end_matches(":generateContent").to_string();
            let body: Value = serde_json::from_str(body).unwrap();
            let call = Call { model, key: key.to_string(), thinking: body["generationConfig"]["thinkingConfig"].clone() };
            self.calls.lock().unwrap().push(call.clone());
            (self.script)(&call)
        }
    }

    fn ok(text: &str) -> Result<HttpResponse, String> {
        Ok(HttpResponse { status: 200, body: json!({ "candidates": [{ "finishReason": "STOP", "content": { "parts": [{ "text": text }] } }] }).to_string() })
    }

    fn status(code: u16, message: &str) -> Result<HttpResponse, String> {
        Ok(HttpResponse { status: code, body: json!({ "error": { "message": message } }).to_string() })
    }

    struct Rig {
        dispatcher: Dispatcher<Fake>,
        calls: Arc<Mutex<Vec<Call>>>,
        lists: Arc<AtomicU64>,
        time: Arc<AtomicU64>,
    }

    impl Rig {
        fn trail(&self) -> Vec<(String, String)> {
            self.calls.lock().unwrap().iter().map(|c| (c.model.clone(), c.key.clone())).collect()
        }
    }

    fn rig(keys: &[&str], script: impl Fn(&Call) -> Result<HttpResponse, String> + Send + Sync + 'static) -> Rig {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let lists = Arc::new(AtomicU64::new(0));
        let time = Arc::new(AtomicU64::new(1_000));
        let fake = Fake { models: vec!["gemini-2.0-flash", "gemini-3.0-flash", "gemini-3.0-flash-lite", "gemma-4-31b-it", "gemini-3.0-flash-tts", "gemini-2.5-flash-preview-tts"], script: Box::new(script), calls: calls.clone(), lists: lists.clone() };
        let clock = time.clone();
        let dispatcher = Dispatcher::with_clock(fake, Box::new(move || clock.load(Ordering::SeqCst)));
        for k in keys {
            dispatcher.add_key(k);
        }
        Rig { dispatcher, calls, lists, time }
    }

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items.iter().map(|(m, k)| (m.to_string(), k.to_string())).collect()
    }

    #[test]
    fn newest_flash_answers_first() {
        let r = rig(&["a"], |_| ok("hello"));
        let (reply, model) = r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma).unwrap();
        assert_eq!(reply.text, "hello");
        assert_eq!(model.id, "gemini-3.0-flash");
    }

    #[test]
    fn quota_walks_keys_then_models() {
        let r = rig(&["a", "b"], |c| if c.model == "gemini-3.0-flash" { status(429, "quota") } else { ok("from the older one") });
        let (_, model) = r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma).unwrap();
        assert_eq!(model.id, "gemini-2.0-flash");
        assert_eq!(r.trail(), pairs(&[("gemini-3.0-flash", "a"), ("gemini-3.0-flash", "b"), ("gemini-2.0-flash", "a")]));
    }

    #[test]
    fn overload_skips_the_model_without_trying_other_keys() {
        let r = rig(&["a", "b"], |c| if c.model == "gemini-3.0-flash" { status(503, "high demand") } else { ok("x") });
        r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma).unwrap();
        assert_eq!(r.trail(), pairs(&[("gemini-3.0-flash", "a"), ("gemini-2.0-flash", "a")]));
    }

    #[test]
    fn a_model_that_never_answers_is_rested_and_another_is_asked() {
        let r = rig(&["a"], |c| if c.model == "gemini-3.0-flash" { Err("the connection was closed".to_string()) } else { ok("from another model") });
        let (reply, model) = r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma).unwrap();
        assert_eq!(reply.text, "from another model");
        assert_ne!(model.id, "gemini-3.0-flash");
        // The silent model is resting, so the next turn goes straight to the one that answered.
        let asked = r.trail().len();
        r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma).unwrap();
        assert_eq!(r.trail().len(), asked + 1);
        assert!(r.trail()[asked..].iter().all(|(m, _)| m != "gemini-3.0-flash"));
    }

    #[test]
    fn a_dead_link_stops_after_two_silent_models_instead_of_walking_the_chain() {
        let r = rig(&["a", "b"], |_| Err("the connection was closed".to_string()));
        assert!(matches!(r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma), Err(GenError::Network(_))));
        assert_eq!(r.trail().len(), 2);
    }

    #[test]
    fn thinking_dialect_is_found_once_and_remembered() {
        let r = rig(&["a"], |c| if c.thinking.get("thinkingLevel").is_some() { status(400, "Thinking level is not supported for this model.") } else { ok("x") });
        r.dispatcher.generate(&GenRequest::prompt("one"), Tier::Gemma).unwrap();
        r.dispatcher.generate(&GenRequest::prompt("two"), Tier::Gemma).unwrap();
        let sent: Vec<Value> = r.calls.lock().unwrap().iter().map(|c| c.thinking.clone()).collect();
        assert_eq!(
            sent,
            [json!({ "thinkingLevel": "medium" }), json!({ "thinkingLevel": "high" }), json!({ "thinkingBudget": 4096 }), json!({ "thinkingBudget": 4096 })]
        );
    }

    #[test]
    fn everything_spent_reports_when_to_come_back() {
        let r = rig(&["a"], |_| {
            Ok(HttpResponse { status: 429, body: json!({ "error": { "details": [{ "retryDelay": "120s" }] } }).to_string() })
        });
        assert_eq!(r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma), Err(GenError::Exhausted { retry_at: 121_000 }));
        assert_eq!(r.trail().len(), 4);
        // Nothing is sent while everything rests, and play resumes once the delay has passed.
        assert!(r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma).is_err());
        assert_eq!(r.trail().len(), 4);
        r.time.store(121_000, Ordering::SeqCst);
        let _ = r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma);
        assert_eq!(r.trail().len(), 8);
    }

    #[test]
    fn flash_only_floor_stops_before_lite() {
        let r = rig(&["a"], |c| if c.model.contains("lite") || c.model.contains("gemma") { ok("x") } else { status(429, "quota") });
        assert!(matches!(r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Flash), Err(GenError::Exhausted { .. })));
        let (_, model) = r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma).unwrap();
        assert_eq!(model.id, "gemini-3.0-flash-lite");
    }

    #[test]
    fn revoked_key_is_dropped_and_the_next_one_serves() {
        let r = rig(&["revoked", "b"], |_| ok("x"));
        r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma).unwrap();
        assert_eq!(r.trail(), pairs(&[("gemini-3.0-flash", "b")]));
    }

    #[test]
    fn a_restart_remembers_spent_quota_and_thinking_dialects() {
        let first = rig(&["a", "b"], |c| {
            if c.thinking.get("thinkingLevel").is_some() {
                status(400, "Thinking level is not supported for this model.")
            } else if c.key == "a" {
                Ok(HttpResponse { status: 429, body: json!({ "error": { "details": [{ "retryDelay": "3600s" }] } }).to_string() })
            } else {
                ok("x")
            }
        });
        first.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma).unwrap();
        let saved = first.dispatcher.export_state();
        assert_eq!(serde_json::from_str::<SavedState>(&serde_json::to_string(&saved).unwrap()).unwrap(), saved);

        let second = rig(&["a", "b"], |_| ok("x"));
        second.dispatcher.import_state(&saved);
        second.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma).unwrap();
        let calls = second.calls.lock().unwrap();
        // Straight to the key with quota, in the dialect the model accepts: one request, no probing.
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].key, "b");
        assert_eq!(calls[0].thinking, json!({ "thinkingBudget": 4096 }));
    }

    #[test]
    fn probing_tells_working_rejected_and_removed_keys_apart() {
        let r = rig(&[], |_| ok("x"));
        let good = r.dispatcher.add_key("a");
        let bad = r.dispatcher.add_key("revoked");
        assert_eq!(r.dispatcher.probe_key(good), KeyHealth::Working);
        assert_eq!(r.dispatcher.probe_key(bad), KeyHealth::Rejected);
        assert!(r.dispatcher.with_pool(|p| p.is_dead(bad)));
        r.dispatcher.remove_key(good);
        assert_eq!(r.dispatcher.probe_key(good), KeyHealth::Rejected);
        // Probing never costs a generation request.
        assert!(r.trail().is_empty());
    }

    #[test]
    fn no_keys_and_only_rejected_keys_both_say_no_keys() {
        assert_eq!(rig(&[], |_| ok("x")).dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma), Err(GenError::NoKeys));
        assert_eq!(rig(&["revoked"], |_| ok("x")).dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma), Err(GenError::NoKeys));
    }

    #[test]
    fn key_rejected_mid_game_falls_to_the_other_key() {
        let r = rig(&["a", "b"], |c| if c.key == "a" { status(403, "Permission denied") } else { ok("x") });
        r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma).unwrap();
        assert_eq!(r.trail(), pairs(&[("gemini-3.0-flash", "a"), ("gemini-3.0-flash", "b")]));
    }

    #[test]
    fn vanished_model_triggers_a_relist_and_the_request_still_lands() {
        let r = rig(&["a"], |c| if c.model == "gemini-3.0-flash" { status(404, "not found") } else { ok("x") });
        let (_, model) = r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma).unwrap();
        assert_eq!(model.id, "gemini-2.0-flash");
        // Two lists (v1 and v1beta) at start, two more after the 404.
        assert_eq!(r.lists.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn chain_is_the_union_of_every_keys_models_and_a_key_skips_what_it_lacks() {
        let r = rig(&["narrow", "a"], |c| if c.key == "narrow" && c.model != "gemini-2.0-flash" { status(404, "not found") } else { ok("x") });
        let (_, model) = r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma).unwrap();
        assert_eq!(model.id, "gemini-3.0-flash");
        assert_eq!(r.trail(), pairs(&[("gemini-3.0-flash", "narrow"), ("gemini-3.0-flash", "a")]));
        // The second request goes straight to the key that has the model.
        r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma).unwrap();
        assert_eq!(r.trail().last().unwrap(), &("gemini-3.0-flash".to_string(), "a".to_string()));
        assert_eq!(r.trail().len(), 3);
    }

    #[test]
    fn a_request_one_model_refuses_is_put_to_the_others_and_malformed_ones_stop_at_once() {
        let no = || Ok(HttpResponse { status: 200, body: json!({ "promptFeedback": { "blockReason": "PROHIBITED_CONTENT" } }).to_string() });
        // Every model refuses: each is asked once, and only then is the refusal given.
        let blocked = rig(&["a", "b"], move |_| no());
        assert_eq!(blocked.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Flash), Err(GenError::Blocked("PROHIBITED_CONTENT".into())));
        let mut asked: Vec<String> = blocked.trail().into_iter().map(|(model, _)| model).collect();
        let times = asked.len();
        asked.dedup();
        assert!(times > 1 && asked.len() == times, "{asked:?}");
        // The first refuses, the next answers.
        let mixed = rig(&["a"], move |call| if call.model == "gemini-3.0-flash" { no() } else { ok("fine") });
        let (reply, model) = mixed.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Flash).unwrap();
        assert_eq!(reply.text, "fine");
        assert_ne!(model.id, "gemini-3.0-flash");
        let bad = rig(&["a", "b"], |_| status(400, "Request contains an invalid argument."));
        assert!(matches!(bad.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma), Err(GenError::BadRequest(_))));
        assert_eq!(bad.trail().len(), 1);
    }

    #[test]
    fn unreachable_server_is_a_network_error_not_an_exhausted_pool() {
        let r = rig(&["a"], |_| Err("dns failure".into()));
        assert_eq!(r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma), Err(GenError::Network("dns failure".into())));
    }

    fn sound() -> Result<HttpResponse, String> {
        Ok(HttpResponse { status: 200, body: json!({ "candidates": [{ "content": { "parts": [{ "inlineData": { "mimeType": "audio/L16;codec=pcm;rate=24000", "data": "AAABAAIAAwA=" } }] } }] }).to_string() })
    }

    #[test]
    fn the_newest_voice_speaks_and_a_spent_one_gives_way_to_the_next_key_then_the_next_voice() {
        let r = rig(&["a", "b"], |c| if c.model == "gemini-3.0-flash-tts" && c.key == "a" { status(429, "quota") } else { sound() });
        let (speech, model) = r.dispatcher.speak("Hello.", "Charon").unwrap();
        assert_eq!((speech.pcm.len(), model.id.as_str()), (8, "gemini-3.0-flash-tts"));
        assert_eq!(r.trail(), pairs(&[("gemini-3.0-flash-tts", "a"), ("gemini-3.0-flash-tts", "b")]));
        // The pair that was refused rests: the next request goes straight to the other key.
        r.dispatcher.speak("Again.", "Charon").unwrap();
        assert_eq!(r.trail().last().unwrap(), &("gemini-3.0-flash-tts".to_string(), "b".to_string()));
        assert_eq!(r.trail().len(), 3);

        let r = rig(&["a"], |c| if c.model == "gemini-3.0-flash-tts" { status(429, "quota") } else { sound() });
        let (_, model) = r.dispatcher.speak("Hello.", "Charon").unwrap();
        assert_eq!(model.id, "gemini-2.5-flash-preview-tts");
    }

    #[test]
    fn with_every_voice_spent_the_answer_says_when_to_come_back_and_the_game_master_is_untouched() {
        let r = rig(&["a"], |c| if c.model.contains("tts") { status(429, "quota") } else { ok("still here") });
        assert!(matches!(r.dispatcher.speak("Hello.", "Charon"), Err(GenError::Exhausted { .. })));
        assert_eq!(r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma).unwrap().0.text, "still here");
        assert!(matches!(rig(&[], |_| sound()).dispatcher.speak("Hello.", "Charon"), Err(GenError::NoKeys)));
    }

}
