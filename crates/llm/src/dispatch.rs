//! Sends one request through the pool: tries the best free key and model, and on a failure
//! that rotation can fix moves on until something answers or nothing is left.

use crate::catalog::{ApiVersion, ListedModel, Model, Tier, build_chain};
use crate::client::{Failure, GenRequest, Reply, Transport, parse_generate, parse_list};
use crate::pool::{KeyId, Millis, Outcome, Pick, Pool};
use crate::thinking::ThinkingMode;
use std::collections::HashMap;
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

#[derive(Default)]
struct State {
    pool: Pool,
    thinking: HashMap<String, ThinkingMode>,
}

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

    /// Rebuilds the chain from both model lists, using the first key that can list them.
    pub fn refresh(&self) -> Result<(), GenError> {
        let keys: Vec<(KeyId, String)> = self.state.lock().unwrap().pool.live_keys().map(|(id, s)| (id, s.to_string())).collect();
        let mut last = GenError::NoKeys;
        for (id, secret) in keys {
            let lists = self.list(ApiVersion::V1, &secret).and_then(|v1| Ok((v1, self.list(ApiVersion::V1Beta, &secret)?)));
            match lists {
                Ok((v1, v1beta)) => {
                    self.state.lock().unwrap().pool.set_chain(build_chain(&v1, &v1beta));
                    return Ok(());
                }
                Err(Ok(Failure::KeyRejected)) => {
                    self.state.lock().unwrap().pool.report((self.clock)(), "", id, Outcome::KeyRejected);
                }
                Err(Ok(failure)) => last = GenError::Network(format!("model list failed: {failure:?}")),
                Err(Err(unreachable)) => last = GenError::Network(unreachable),
            }
        }
        Err(last)
    }

    /// Runs the request on the best model available no lower than `lowest`. Returns the answer
    /// together with the model that gave it.
    pub fn generate(&self, request: &GenRequest, lowest: Tier) -> Result<(Reply, Model), GenError> {
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
                match state.pool.pick(now, lowest) {
                    Pick::Ready { model, key, secret } => {
                        let mode = state.thinking.get(&model.id).copied().unwrap_or_default();
                        (model, key, secret, mode)
                    }
                    Pick::Wait { until } => return Err(GenError::Exhausted { retry_at: until }),
                    Pick::Nothing if state.pool.live_keys().next().is_none() => return Err(GenError::NoKeys),
                    Pick::Nothing => return Err(GenError::Exhausted { retry_at: now + 60_000 }),
                }
            };
            let body = request.body(mode.config(request.thinking)).to_string();
            let url = format!("{}/{}/models/{}:generateContent", self.base, model.api.path(), model.id);
            let response = self.transport.post(&url, &secret, &body).map_err(GenError::Network)?;
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
                Err(Failure::Blocked(reason)) => return Err(GenError::Blocked(reason)),
                Err(Failure::BadRequest(message)) => return Err(GenError::BadRequest(message)),
                Err(Failure::RateLimited { retry_after, limit }) => Outcome::RateLimited { retry_after, limit },
                Err(Failure::Overloaded) => Outcome::Overloaded,
                Err(Failure::ModelGone) => Outcome::ModelGone,
                Err(Failure::KeyRejected) => Outcome::KeyRejected,
            };
            state.pool.report(now, &model.id, key, outcome);
        }
        Err(GenError::Exhausted { retry_at: (self.clock)() + 60_000 })
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
            let models: Vec<Value> = self.models.iter().map(|m| json!({ "name": format!("models/{m}"), "supportedGenerationMethods": ["generateContent"] })).collect();
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
        let fake = Fake { models: vec!["gemini-2.0-flash", "gemini-3.0-flash", "gemini-3.0-flash-lite", "gemma-4-31b-it"], script: Box::new(script), calls: calls.clone(), lists: lists.clone() };
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
    fn blocked_and_malformed_requests_stop_without_burning_other_models() {
        let blocked = rig(&["a", "b"], |_| Ok(HttpResponse { status: 200, body: json!({ "promptFeedback": { "blockReason": "PROHIBITED_CONTENT" } }).to_string() }));
        assert_eq!(blocked.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma), Err(GenError::Blocked("PROHIBITED_CONTENT".into())));
        assert_eq!(blocked.trail().len(), 1);
        let bad = rig(&["a", "b"], |_| status(400, "Request contains an invalid argument."));
        assert!(matches!(bad.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma), Err(GenError::BadRequest(_))));
        assert_eq!(bad.trail().len(), 1);
    }

    #[test]
    fn unreachable_server_is_a_network_error_not_an_exhausted_pool() {
        let r = rig(&["a"], |_| Err("dns failure".into()));
        assert_eq!(r.dispatcher.generate(&GenRequest::prompt("hi"), Tier::Gemma), Err(GenError::Network("dns failure".into())));
    }
}
