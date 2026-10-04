//! Game masters other than Gemini, all speaking the OpenAI chat completions protocol:
//! NVIDIA NIM, OpenRouter, Cloudflare Workers AI, and any service of the player's own (a
//! local Ollama or LM Studio among them). Each has several keys and a list of models in the
//! order the player wants them tried. Keys take turns; a key that is refused rests, and the
//! next one, then the next model, then the next provider is tried.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Nim,
    OpenRouter,
    Cloudflare,
    /// Any address of the player's own.
    Custom,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Nim, Kind::OpenRouter, Kind::Cloudflare, Kind::Custom];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Nim => "NVIDIA NIM",
            Kind::OpenRouter => "OpenRouter",
            Kind::Cloudflare => "Cloudflare",
            Kind::Custom => "OpenAI API",
        }
    }
}

/// One provider as it is asked: its keys in the clear and its models in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provider {
    pub kind: Kind,
    /// Empty for a local server that asks for none.
    pub keys: Vec<String>,
    /// Tried in this order: the first is the one the player wants most.
    pub models: Vec<String>,
    /// Cloudflare's account id.
    pub account: String,
    /// The address of a service of the player's own, up to and including the version.
    pub url: String,
    /// The models that speak, for the narrator's voice, tried in this order.
    pub voices: Vec<String>,
    /// The voice asked for, where a model has several ("alloy", "af_heart", "angus"…).
    pub voice: String,
    /// How hard the model is asked to think: "low", "medium", "high", or empty to say
    /// nothing. A model that refuses the request for it is asked again without.
    pub effort: String,
}

impl Provider {
    /// The address up to and including the version.
    pub fn base(&self) -> String {
        match self.kind {
            Kind::Nim => "https://integrate.api.nvidia.com/v1".to_string(),
            Kind::OpenRouter => "https://openrouter.ai/api/v1".to_string(),
            Kind::Cloudflare => format!("https://api.cloudflare.com/client/v4/accounts/{}/ai/v1", self.account.trim()),
            Kind::Custom => self.url.trim().trim_end_matches('/').trim_end_matches("/chat/completions").to_string(),
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base())
    }

    /// Whether there is enough to ask with.
    pub fn ready(&self) -> bool {
        let addressed = match self.kind {
            Kind::Cloudflare => !self.account.trim().is_empty(),
            Kind::Custom => !self.url.trim().is_empty(),
            _ => true,
        };
        // Only a service of one's own may go without a key.
        addressed && !self.models.is_empty() && (self.kind == Kind::Custom || !self.keys.is_empty())
    }

    /// Whether there is enough to ask a speaking model with.
    pub fn voice_ready(&self) -> bool {
        let addressed = match self.kind {
            Kind::Cloudflare => !self.account.trim().is_empty(),
            Kind::Custom => !self.url.trim().is_empty(),
            _ => true,
        };
        addressed && !self.voices.is_empty() && (self.kind == Kind::Custom || !self.keys.is_empty())
    }

    /// Where a speaking model is asked, and with what. Cloudflare runs its models at an
    /// address of their own, each with its own words for the text; the rest take the
    /// OpenAI shape.
    fn speech_request(&self, model: &str, text: &str, russian: bool) -> (String, String) {
        match self.kind {
            Kind::Cloudflare => {
                let url = format!("https://api.cloudflare.com/client/v4/accounts/{}/ai/run/{model}", self.account.trim());
                let body = if model.contains("melotts") {
                    // MeloTTS speaks a handful of languages; Russian is not one, so English it is.
                    let _ = russian;
                    serde_json::json!({ "prompt": text, "lang": "en" })
                } else if self.voice.trim().is_empty() {
                    serde_json::json!({ "text": text })
                } else {
                    serde_json::json!({ "text": text, "speaker": self.voice.trim() })
                };
                (url, body.to_string())
            }
            _ => {
                let voice = if self.voice.trim().is_empty() { "alloy" } else { self.voice.trim() };
                (format!("{}/audio/speech", self.base()), serde_json::json!({ "model": model, "input": text, "voice": voice, "response_format": "mp3" }).to_string())
            }
        }
    }

    /// The speaking models the provider offers.
    pub fn list_voices(&self) -> Result<Vec<String>, String> {
        if self.kind == Kind::Cloudflare {
            return self.list_from(&format!("https://api.cloudflare.com/client/v4/accounts/{}/ai/models/search?task=Text-to-Speech&per_page=100", self.account.trim()));
        }
        // Elsewhere the speaking models are told from the rest by their names.
        let speaks = |id: &str| ["tts", "speech", "audio", "voice", "kokoro", "piper", "orpheus", "magpie", "fastpitch"].iter().any(|word| id.to_lowercase().contains(word));
        Ok(self.list_models()?.into_iter().filter(|id| speaks(id)).collect())
    }

    /// The models the provider offers, for the player to choose from.
    pub fn list_models(&self) -> Result<Vec<String>, String> {
        let url = match self.kind {
            // Cloudflare lists its models elsewhere than the OpenAI-shaped address.
            Kind::Cloudflare => format!("https://api.cloudflare.com/client/v4/accounts/{}/ai/models/search?task=Text%20Generation&per_page=200", self.account.trim()),
            _ => format!("{}/models", self.base()),
        };
        self.list_from(&url)
    }

    fn list_from(&self, url: &str) -> Result<Vec<String>, String> {
        let key = self.keys.first().map(String::as_str).unwrap_or("");
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout(Duration::from_secs(40)).build();
        let mut request = agent.get(url);
        if !key.trim().is_empty() {
            request = request.set("Authorization", &format!("Bearer {}", key.trim()));
        }
        let body = match request.call() {
            Ok(answer) => answer.into_string().map_err(|e| e.to_string())?,
            Err(ureq::Error::Status(status, answer)) => return Err(format!("{status}: {}", answer.into_string().unwrap_or_default().chars().take(200).collect::<String>())),
            Err(e) => return Err(e.to_string()),
        };
        read_models(&body)
    }
}

/// The ids in a list of models, whichever of the shapes it comes in.
pub fn read_models(body: &str) -> Result<Vec<String>, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| format!("not JSON: {e}"))?;
    let list = value.get("data").or_else(|| value.get("result")).and_then(|l| l.as_array()).ok_or("the answer lists no models")?;
    let mut ids: Vec<String> = list.iter().filter_map(|m| m.get("id").or_else(|| m.get("name")).and_then(|id| id.as_str())).filter(|id| id.starts_with('@') || !id.contains(' ')).map(str::to_string).collect();
    // Cloudflare gives both an id and a name; the name is what is asked with.
    if list.iter().any(|m| m.get("name").and_then(|n| n.as_str()).is_some_and(|n| n.starts_with("@cf/") || n.starts_with("@hf/"))) {
        ids = list.iter().filter_map(|m| m.get("name").and_then(|n| n.as_str())).map(str::to_string).collect();
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// What came of a request that was not answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The HTTP status, or 0 when the service was not reached.
    pub status: u16,
    /// Seconds the service asked to be left alone for.
    pub retry_after: Option<u64>,
    pub text: String,
}

/// How requests are sent: the real network, or a stand-in in tests.
pub trait Transport {
    fn post(&self, url: &str, key: &str, body: &str, timeout: Duration) -> Result<String, Refusal>;

    /// As `post`, for an answer that is sound: the bytes, and what kind they are said to be.
    fn post_bytes(&self, _url: &str, _key: &str, _body: &str, _timeout: Duration) -> Result<(Vec<u8>, String), Refusal> {
        Err(Refusal { status: 0, retry_after: None, text: "no sound here".to_string() })
    }
}

/// The sound in an answer: as it came, or taken out of the JSON some services wrap it in.
pub fn read_sound(bytes: Vec<u8>, kind: &str) -> Result<Vec<u8>, String> {
    if !kind.contains("json") && !bytes.starts_with(b"{") {
        return if bytes.len() > 200 { Ok(bytes) } else { Err("the answer had no sound".to_string()) };
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| format!("not JSON: {e}"))?;
    let coded = value["result"]["audio"].as_str().or_else(|| value["audio"].as_str()).or_else(|| value["data"].as_str());
    match coded {
        Some(coded) if coded.len() > 200 => Ok(crate::client::base64_decode(coded)),
        _ => Err(value.get("errors").or_else(|| value.get("error")).map_or_else(|| "the answer had no sound".to_string(), |e| e.to_string().chars().take(200).collect())),
    }
}

pub struct Network;

impl Transport for Network {
    fn post(&self, url: &str, key: &str, body: &str, timeout: Duration) -> Result<String, Refusal> {
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout(timeout).build();
        let mut request = agent.post(url).set("Content-Type", "application/json");
        if !key.trim().is_empty() {
            request = request.set("Authorization", &format!("Bearer {}", key.trim()));
        }
        match request.send_string(body) {
            Ok(answer) => answer.into_string().map_err(|e| Refusal { status: 0, retry_after: None, text: e.to_string() }),
            Err(ureq::Error::Status(status, answer)) => {
                let retry_after = answer.header("Retry-After").and_then(|v| v.trim().parse().ok());
                Err(Refusal { status, retry_after, text: answer.into_string().unwrap_or_default().chars().take(300).collect() })
            }
            Err(e) => Err(Refusal { status: 0, retry_after: None, text: e.to_string() }),
        }
    }

    fn post_bytes(&self, url: &str, key: &str, body: &str, timeout: Duration) -> Result<(Vec<u8>, String), Refusal> {
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout(timeout).build();
        let mut request = agent.post(url).set("Content-Type", "application/json");
        if !key.trim().is_empty() {
            request = request.set("Authorization", &format!("Bearer {}", key.trim()));
        }
        match request.send_string(body) {
            Ok(answer) => {
                let kind = answer.content_type().to_string();
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut std::io::Read::take(answer.into_reader(), 20_000_000), &mut bytes).map_err(|e| Refusal { status: 0, retry_after: None, text: e.to_string() })?;
                Ok((bytes, kind))
            }
            Err(ureq::Error::Status(status, answer)) => {
                let retry_after = answer.header("Retry-After").and_then(|v| v.trim().parse().ok());
                Err(Refusal { status, retry_after, text: answer.into_string().unwrap_or_default().chars().take(300).collect() })
            }
            Err(e) => Err(Refusal { status: 0, retry_after: None, text: e.to_string() }),
        }
    }
}

/// Which keys and models are resting, and whose turn it is: kept for as long as the game runs.
#[derive(Default)]
pub struct Rotation {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// A key with a model, or with "" for the whole key, and when it may be asked again.
    resting: HashMap<(Kind, String, String), Instant>,
    /// A model no key of the provider could be asked for, and until when.
    models: HashMap<(Kind, String), Instant>,
    /// The key each provider starts with next time, so that the keys take turns.
    next: HashMap<Kind, usize>,
    /// Models that would not be told how hard to think.
    plain: std::collections::HashSet<(Kind, String)>,
}

/// The end of a key, to tell keys apart in the log without showing them.
fn tail(key: &str) -> String {
    let key = key.trim();
    let n = key.chars().count();
    if n == 0 { "no key".to_string() } else { format!("…{}", key.chars().skip(n.saturating_sub(4)).collect::<String>()) }
}

impl Rotation {
    /// Asks the providers in order: each one's models in the player's order, each model with
    /// every key that is not resting, starting from the key whose turn it is. Gives the text
    /// and what answered ("provider: model"). The error tells what every attempt came to.
    pub fn complete(&self, providers: &[Provider], system: &str, prompt: &str, transport: &dyn Transport) -> Result<(String, String), String> {
        let mut tried = Vec::new();
        for provider in providers.iter().filter(|p| p.ready()) {
            let no_key = [String::new()];
            let keys: &[String] = if provider.keys.is_empty() { &no_key } else { &provider.keys };
            let start = {
                let mut state = self.state.lock().unwrap();
                let next = state.next.entry(provider.kind).or_insert(0);
                let start = *next % keys.len();
                *next = (start + 1) % keys.len();
                start
            };
            let timeout = Duration::from_secs(if provider.kind == Kind::Custom { 600 } else { 180 });
            for model in &provider.models {
                let model = model.trim();
                if self.model_rests(provider.kind, model) {
                    continue;
                }
                for step in 0..keys.len() {
                    let key = &keys[(start + step) % keys.len()];
                    if self.rests(provider.kind, key, model) {
                        continue;
                    }
                    let body = |effort: bool| {
                        let mut body = serde_json::json!({
                            "model": model,
                            "messages": [{ "role": "system", "content": system }, { "role": "user", "content": prompt }],
                            "temperature": 0.9,
                            "stream": false,
                        });
                        if effort {
                            body["reasoning_effort"] = serde_json::json!(provider.effort.trim());
                        }
                        body.to_string()
                    };
                    let label = format!("{}: {model}", provider.kind.name());
                    let effort = !provider.effort.trim().is_empty() && !self.state.lock().unwrap().plain.contains(&(provider.kind, model.to_string()));
                    let mut answer = transport.post(&provider.endpoint(), key, &body(effort), timeout);
                    // Refused with the thinking asked for: it may be the asking. Once more, plainly.
                    if effort && answer.as_ref().is_err_and(|r| matches!(r.status, 400 | 422)) {
                        answer = transport.post(&provider.endpoint(), key, &body(false), timeout);
                        if answer.is_ok() {
                            self.state.lock().unwrap().plain.insert((provider.kind, model.to_string()));
                        }
                    }
                    match answer {
                        Ok(answer) => match crate::openai::read_answer(&answer) {
                            Ok(text) => return Ok((text, label)),
                            Err(why) => tried.push(format!("{label} ({}): {why}", tail(key))),
                        },
                        Err(refusal) => {
                            tried.push(format!("{label} ({}): {} {}", tail(key), refusal.status, refusal.text.chars().take(120).collect::<String>()));
                            self.note(provider.kind, key, model, &refusal);
                            // A model that is not there is not there for any key.
                            if matches!(refusal.status, 400 | 404 | 422) {
                                break;
                            }
                        }
                    }
                }
            }
        }
        Err(if tried.is_empty() { "every key and model of the providers is resting".to_string() } else { tried.join(" | ") })
    }

    /// Has the text read aloud: the providers in order, each one's speaking models in the
    /// player's order, the keys in turn, as for a telling. Gives the sound (an MP3 or a WAV,
    /// as the service made it) and what spoke.
    pub fn speak(&self, providers: &[Provider], text: &str, russian: bool, transport: &dyn Transport) -> Result<(Vec<u8>, String), String> {
        let mut tried = Vec::new();
        for provider in providers.iter().filter(|p| p.voice_ready()) {
            let no_key = [String::new()];
            let keys: &[String] = if provider.keys.is_empty() { &no_key } else { &provider.keys };
            let start = {
                let mut state = self.state.lock().unwrap();
                let next = state.next.entry(provider.kind).or_insert(0);
                let start = *next % keys.len();
                *next = (start + 1) % keys.len();
                start
            };
            for model in &provider.voices {
                let model = model.trim();
                if self.model_rests(provider.kind, model) {
                    continue;
                }
                for step in 0..keys.len() {
                    let key = &keys[(start + step) % keys.len()];
                    if self.rests(provider.kind, key, model) {
                        continue;
                    }
                    let (url, body) = provider.speech_request(model, text, russian);
                    let label = format!("{}: {model}", provider.kind.name());
                    match transport.post_bytes(&url, key, &body, Duration::from_secs(120)) {
                        Ok((bytes, kind)) => match read_sound(bytes, &kind) {
                            Ok(sound) => return Ok((sound, label)),
                            Err(why) => tried.push(format!("{label} ({}): {why}", tail(key))),
                        },
                        Err(refusal) => {
                            tried.push(format!("{label} ({}): {} {}", tail(key), refusal.status, refusal.text.chars().take(120).collect::<String>()));
                            self.note(provider.kind, key, model, &refusal);
                            if matches!(refusal.status, 400 | 404 | 405 | 422) {
                                break;
                            }
                        }
                    }
                }
            }
        }
        Err(if tried.is_empty() { "no speaking model of the providers could be asked".to_string() } else { tried.join(" | ") })
    }

    fn rests(&self, kind: Kind, key: &str, model: &str) -> bool {
        let state = self.state.lock().unwrap();
        let now = Instant::now();
        [model, ""].iter().any(|m| state.resting.get(&(kind, key.to_string(), m.to_string())).is_some_and(|until| *until > now))
    }

    fn model_rests(&self, kind: Kind, model: &str) -> bool {
        self.state.lock().unwrap().models.get(&(kind, model.to_string())).is_some_and(|until| *until > Instant::now())
    }

    /// What a refusal means for the key and the model it was given to.
    fn note(&self, kind: Kind, key: &str, model: &str, refusal: &Refusal) {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        let secs = |s: u64| now + Duration::from_secs(s);
        match refusal.status {
            // Too many requests: this key with this model rests for as long as asked.
            429 => {
                state.resting.insert((kind, key.to_string(), model.to_string()), secs(refusal.retry_after.unwrap_or(60).clamp(5, 3600)));
            }
            // Out of credit, a bad key or no leave: the whole key rests.
            402 => {
                state.resting.insert((kind, key.to_string(), String::new()), secs(1800));
            }
            401 | 403 => {
                state.resting.insert((kind, key.to_string(), String::new()), secs(6 * 3600));
            }
            // No such model, or it cannot take this request: the model rests for every key.
            400 | 404 | 405 | 422 => {
                state.models.insert((kind, model.to_string()), secs(1800));
            }
            // The service is unwell or was not reached: a short rest, then it is tried again.
            _ => {
                state.resting.insert((kind, key.to_string(), model.to_string()), secs(20));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Answers by a rule, and keeps what it was asked.
    struct Scripted<F: Fn(&str, &str, &str) -> Result<String, Refusal>> {
        rule: F,
        asked: RefCell<Vec<(String, String, String)>>,
    }

    impl<F: Fn(&str, &str, &str) -> Result<String, Refusal>> Transport for Scripted<F> {
        fn post(&self, url: &str, key: &str, body: &str, _: Duration) -> Result<String, Refusal> {
            let model = serde_json::from_str::<serde_json::Value>(body).unwrap()["model"].as_str().unwrap().to_string();
            self.asked.borrow_mut().push((url.to_string(), key.to_string(), model.clone()));
            (self.rule)(url, key, &model)
        }
    }

    fn scripted<F: Fn(&str, &str, &str) -> Result<String, Refusal>>(rule: F) -> Scripted<F> {
        Scripted { rule, asked: RefCell::new(Vec::new()) }
    }

    const OK: &str = r#"{"choices":[{"message":{"content":"{\"ok\":true}"}}]}"#;

    fn refused(status: u16) -> Result<String, Refusal> {
        Err(Refusal { status, retry_after: None, text: "no".into() })
    }

    fn nim(keys: &[&str], models: &[&str]) -> Provider {
        Provider { kind: Kind::Nim, keys: keys.iter().map(|k| k.to_string()).collect(), models: models.iter().map(|m| m.to_string()).collect(), account: String::new(), url: String::new(), effort: String::new(), voices: Vec::new(), voice: String::new() }
    }

    #[test]
    fn each_provider_has_its_address() {
        assert_eq!(nim(&["k"], &["m"]).endpoint(), "https://integrate.api.nvidia.com/v1/chat/completions");
        let cloud = Provider { kind: Kind::Cloudflare, account: " abc ".into(), ..nim(&["k"], &["m"]) };
        assert_eq!(cloud.endpoint(), "https://api.cloudflare.com/client/v4/accounts/abc/ai/v1/chat/completions");
        let router = Provider { kind: Kind::OpenRouter, ..nim(&["k"], &["m"]) };
        assert_eq!(router.endpoint(), "https://openrouter.ai/api/v1/chat/completions");
        let own = Provider { kind: Kind::Custom, url: "http://localhost:11434/v1/".into(), keys: vec![], ..nim(&[], &["m"]) };
        assert_eq!(own.endpoint(), "http://localhost:11434/v1/chat/completions");
        // A local server needs no key; the others do; Cloudflare needs its account too.
        assert!(own.ready() && !nim(&[], &["m"]).ready() && !nim(&["k"], &[]).ready());
        assert!(!Provider { account: String::new(), ..cloud }.ready());
    }

    #[test]
    fn keys_take_turns_and_the_first_model_is_asked_first() {
        let rotation = Rotation::default();
        let transport = scripted(|_, _, _| Ok(OK.to_string()));
        let providers = [nim(&["k1", "k2", "k3"], &["best", "second"])];
        for _ in 0..4 {
            let (text, label) = rotation.complete(&providers, "s", "p", &transport).unwrap();
            assert_eq!((text.as_str(), label.as_str()), ("{\"ok\":true}", "NVIDIA NIM: best"));
        }
        let asked = transport.asked.borrow();
        assert_eq!(asked.iter().map(|a| a.1.as_str()).collect::<Vec<_>>(), ["k1", "k2", "k3", "k1"]);
        assert!(asked.iter().all(|a| a.2 == "best"));
    }

    #[test]
    fn a_refused_key_rests_and_the_next_key_model_and_provider_are_tried() {
        let rotation = Rotation::default();
        // The first key is out of requests; the best model is unknown to the second provider.
        let transport = scripted(|url, key, model| match (url.contains("nvidia"), key, model) {
            (true, "k1", _) => refused(429),
            (true, _, "best") => Ok(OK.to_string()),
            (false, _, "gone") => refused(404),
            (false, _, _) => Ok(OK.to_string()),
            _ => refused(500),
        });
        let router = Provider { kind: Kind::OpenRouter, ..nim(&["r1", "r2"], &["gone", "kept"]) };
        let providers = [nim(&["k1", "k2"], &["best"]), router.clone()];
        assert_eq!(rotation.complete(&providers, "s", "p", &transport).unwrap().1, "NVIDIA NIM: best");
        // The key that was refused is not asked again while it rests: the second answers.
        transport.asked.borrow_mut().clear();
        rotation.complete(&providers, "s", "p", &transport).unwrap();
        assert_eq!(transport.asked.borrow().iter().map(|a| a.1.as_str()).collect::<Vec<_>>(), ["k2"]);
        // A model that is not there is given up for every key at once, and the next is asked.
        transport.asked.borrow_mut().clear();
        assert_eq!(rotation.complete(&[router.clone()], "s", "p", &transport).unwrap().1, "OpenRouter: kept");
        assert_eq!(transport.asked.borrow().iter().map(|a| a.2.as_str()).collect::<Vec<_>>(), ["gone", "kept"]);
        transport.asked.borrow_mut().clear();
        rotation.complete(&[router], "s", "p", &transport).unwrap();
        assert_eq!(transport.asked.borrow().iter().map(|a| a.2.as_str()).collect::<Vec<_>>(), ["kept"]);
        // With everything refused the error says what each attempt came to.
        let dead = scripted(|_, _, _| refused(401));
        let why = Rotation::default().complete(&[nim(&["k1", "k2"], &["best"])], "s", "p", &dead).unwrap_err();
        assert!(why.contains("…k1") || why.contains("k1"), "{why}");
        assert_eq!(dead.asked.borrow().len(), 2);
    }

    #[test]
    fn a_model_that_will_not_be_told_how_to_think_is_asked_plainly_from_then_on() {
        let rotation = Rotation::default();
        let bodies = RefCell::new(Vec::new());
        struct Picky<'a>(&'a RefCell<Vec<bool>>);
        impl Transport for Picky<'_> {
            fn post(&self, _: &str, _: &str, body: &str, _: Duration) -> Result<String, Refusal> {
                let asked = body.contains("reasoning_effort");
                self.0.borrow_mut().push(asked);
                if asked { refused(400) } else { Ok(OK.to_string()) }
            }
        }
        let provider = Provider { effort: "high".into(), ..nim(&["k"], &["m"]) };
        rotation.complete(&[provider.clone()], "s", "p", &Picky(&bodies)).unwrap();
        rotation.complete(&[provider], "s", "p", &Picky(&bodies)).unwrap();
        assert_eq!(*bodies.borrow(), [true, false, false]);
    }

    #[test]
    fn the_narrator_is_asked_of_the_speaking_models_in_order_with_the_keys_in_turn() {
        struct Speaker(RefCell<Vec<(String, String, String)>>);
        impl Transport for Speaker {
            fn post(&self, _: &str, _: &str, _: &str, _: Duration) -> Result<String, Refusal> {
                refused(500)
            }
            fn post_bytes(&self, url: &str, key: &str, body: &str, _: Duration) -> Result<(Vec<u8>, String), Refusal> {
                self.0.borrow_mut().push((url.to_string(), key.to_string(), body.to_string()));
                if url.contains("melotts") {
                    return Ok((format!(r#"{{"result":{{"audio":"{}"}}}}"#, "QUJD".repeat(100)).into_bytes(), "application/json".into()));
                }
                if body.contains("broken") { Err(Refusal { status: 404, retry_after: None, text: "no".into() }) } else { Ok((vec![7u8; 500], "audio/mpeg".into())) }
            }
        }
        let own = Provider { kind: Kind::Custom, url: "http://localhost:8880/v1".into(), keys: vec![], voices: vec!["broken".into(), "kokoro".into()], voice: "af_heart".into(), ..nim(&[], &[]) };
        let speaker = Speaker(RefCell::new(Vec::new()));
        let rotation = Rotation::default();
        let (sound, who) = rotation.speak(&[own.clone()], "Hello.", false, &speaker).unwrap();
        assert_eq!((sound.len(), who.as_str()), (500, "OpenAI API: kokoro"));
        let asked = speaker.0.borrow().clone();
        assert_eq!(asked[0].0, "http://localhost:8880/v1/audio/speech");
        assert!(asked[1].2.contains("\"voice\":\"af_heart\"") && asked[1].2.contains("\"input\":\"Hello.\""));
        // The model that is not there is not asked again.
        speaker.0.borrow_mut().clear();
        rotation.speak(&[own], "Again.", false, &speaker).unwrap();
        assert_eq!(speaker.0.borrow().len(), 1);
        // Cloudflare's models are run at their own address, and wrap the sound in JSON.
        let cloud = Provider { kind: Kind::Cloudflare, account: "acc".into(), voices: vec!["@cf/myshell-ai/melotts".into()], ..nim(&["k"], &[]) };
        speaker.0.borrow_mut().clear();
        let (sound, _) = rotation.speak(&[cloud], "Hi.", true, &speaker).unwrap();
        assert_eq!(&sound[..3], b"ABC");
        assert_eq!(speaker.0.borrow()[0].0, "https://api.cloudflare.com/client/v4/accounts/acc/ai/run/@cf/myshell-ai/melotts");
        // A provider with no speaking model is passed over.
        assert!(rotation.speak(&[nim(&["k"], &["chat"])], "x", false, &speaker).is_err());
    }

    #[test]
    fn model_lists_are_read_in_either_shape() {
        assert_eq!(read_models(r#"{"data":[{"id":"meta/llama-3.3-70b-instruct"},{"id":"a/b"}]}"#).unwrap(), ["a/b", "meta/llama-3.3-70b-instruct"]);
        assert_eq!(read_models(r#"{"result":[{"id":"uuid-1","name":"@cf/meta/llama-3.1-8b-instruct"}]}"#).unwrap(), ["@cf/meta/llama-3.1-8b-instruct"]);
        assert!(read_models("{}").is_err());
    }
}
