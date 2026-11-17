//! What OpenCode Go asks of a client besides the key, done the way OpenCode's own client
//! does it (`packages/core/src/session/model-request.ts` in github.com/sst/opencode):
//!
//! - every request carries the session it belongs to, under the names OpenCode reads it by.
//!   Go keeps a model's cache of the prompt by that session; a request without it is not
//!   cached, and every one is paid for in full. Since September 2026 one without
//!   `x-opencode-session` is refused.
//! - which models there are and where each is asked comes from OpenCode's own catalog,
//!   `models.opencode.ai`, looked at again every few minutes as OpenCode's client does, so a
//!   model added or moved there is known here without a new build.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// OpenCode's catalog of providers and models: the one its client reads.
pub const CATALOG: &str = "https://models.opencode.ai/experimental/v2.0/api.json?type=chat";
/// The provider's name in the catalog.
const PROVIDER: &str = "opencode-go";
/// How long the catalog is believed before it is asked for again: OpenCode's own client
/// waits as long.
const FRESH_FOR: Duration = Duration::from_secs(5 * 60);
/// Where a model takes the OpenAI shape when the catalog has not been read.
pub const CHAT_BASE: &str = "https://opencode.ai/inference/go/openai/v1";

/// A model the game can ask: one that takes chat completions, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub id: String,
    pub base: String,
}

/// The models of OpenCode Go in a catalog that take the OpenAI shape of chat. Those that
/// take only Anthropic's messages, OpenAI's responses or Google's shape are left out: the
/// game speaks chat completions.
pub fn read_catalog(json: &str) -> Result<Vec<Model>, String> {
    let catalog: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("the catalog is not JSON: {e}"))?;
    let models = catalog[PROVIDER]["models"].as_object().ok_or("the catalog has no OpenCode Go")?;
    let mut out: Vec<Model> = models
        .iter()
        .filter(|(_, model)| model["status"].as_str() != Some("deprecated"))
        .filter_map(|(id, model)| {
            let base = model["api"]["chat-completions"]["base_url"].as_str()?.trim().trim_end_matches('/');
            (!base.is_empty()).then(|| Model { id: id.clone(), base: base.to_string() })
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn cache() -> &'static Mutex<Option<(Instant, Vec<Model>)>> {
    static CACHE: OnceLock<Mutex<Option<(Instant, Vec<Model>)>>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

/// The models as the catalog has them now: read again when what was read is older than a
/// few minutes. If it cannot be read, what was read before is kept.
pub fn models() -> Result<Vec<Model>, String> {
    if let Some((when, models)) = cache().lock().unwrap().as_ref()
        && when.elapsed() < FRESH_FOR
    {
        return Ok(models.clone());
    }
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout(Duration::from_secs(30)).build();
    let fetched = agent.get(CATALOG).set("User-Agent", &user_agent()).call().map_err(|e| e.to_string()).and_then(|r| r.into_string().map_err(|e| e.to_string())).and_then(|json| read_catalog(&json));
    match fetched {
        Ok(models) => {
            *cache().lock().unwrap() = Some((Instant::now(), models.clone()));
            Ok(models)
        }
        Err(why) => match cache().lock().unwrap().as_ref() {
            Some((_, models)) => Ok(models.clone()),
            None => Err(why),
        },
    }
}

/// Where a model is asked: as the catalog says, or the usual address.
pub fn base_for(model: &str) -> String {
    models().ok().and_then(|models| models.into_iter().find(|m| m.id == model)).map_or_else(|| CHAT_BASE.to_string(), |m| m.base)
}

/// The game as it names itself to OpenCode.
fn user_agent() -> String {
    format!("pixelgm/{}", env!("CARGO_PKG_VERSION"))
}

/// A session of the shape OpenCode's client makes (`ses_` and 64 hex digits), new for every
/// run of the game: the requests of one sitting share a cache.
pub fn session() -> &'static str {
    static SESSION: OnceLock<String> = OnceLock::new();
    SESSION.get_or_init(|| {
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()) ^ ((std::process::id() as u128) << 64);
        session_of(&seed.to_string())
    })
}

/// The session for something the player keeps coming back to, such as one world: the same
/// words always give the same session.
pub fn session_of(words: &str) -> String {
    // Four rounds of FNV-1a with different starts give 256 bits.
    let mut hex = String::from("ses_");
    for round in 0u64..4 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325 ^ round.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        for byte in words.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hex.push_str(&format!("{hash:016x}"));
    }
    hex
}

/// The headers OpenCode's client sends with every request of a session, the same names and
/// the same values, but for its own name and version.
pub fn headers(session: &str) -> Vec<(&'static str, String)> {
    vec![
        ("x-opencode-session-id", session.to_string()),
        ("x-session-affinity", session.to_string()),
        ("X-Session-Id", session.to_string()),
        ("User-Agent", user_agent()),
        ("x-opencode-project", "global".to_string()),
        ("x-opencode-session", session.to_string()),
        ("x-opencode-client", "pixelgm".to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_gives_the_models_that_take_chat_and_where() {
        let json = r#"{
            "opencode-go": { "models": {
                "kimi-k3": { "api": { "chat-completions": { "base_url": "https://opencode.ai/inference/go/openai/v1/" } } },
                "qwen3.8-max": { "api": { "messages": { "base_url": "https://opencode.ai/inference/go/anthropic/v1" } } },
                "gpt-6-luna": { "api": { "responses": { "base_url": "https://opencode.ai/inference/go/openai/v1" } } },
                "glm-old": { "status": "deprecated", "api": { "chat-completions": { "base_url": "https://x/v1" } } },
                "deepseek-v4-pro": { "api": { "chat-completions": { "base_url": "https://opencode.ai/inference/go/openai/v1" } } }
            }},
            "other": { "models": {} }
        }"#;
        let models = read_catalog(json).unwrap();
        assert_eq!(models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["deepseek-v4-pro", "kimi-k3"]);
        assert_eq!(models[1].base, "https://opencode.ai/inference/go/openai/v1");
        assert!(read_catalog("{}").is_err());
    }

    #[test]
    fn a_session_is_shaped_as_opencode_shapes_it_and_stays_the_same() {
        let session = session_of("world-1");
        assert!(session.starts_with("ses_") && session.len() == 68 && session[4..].chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(session, session_of("world-1"));
        assert_ne!(session, session_of("world-2"));
        assert_eq!(super::session(), super::session());
    }

    #[test]
    fn every_request_names_its_session_as_opencode_does() {
        let headers = headers("ses_1");
        for name in ["x-opencode-session", "x-opencode-session-id", "x-session-affinity", "X-Session-Id"] {
            assert!(headers.iter().any(|(n, v)| *n == name && v == "ses_1"), "{name}");
        }
        assert!(headers.iter().any(|(n, v)| *n == "x-opencode-project" && v == "global"));
    }
}
