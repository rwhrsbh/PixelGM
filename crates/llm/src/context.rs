//! How much each model can read at once, as its provider lists it: filled in whenever a list
//! of models is read, so that the game knows how long a request may grow before the story
//! has to be written up shorter.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Assumed for a model whose provider does not say: small enough to be safe on most.
pub const UNKNOWN: u32 = 32_000;

fn known() -> &'static Mutex<HashMap<String, u32>> {
    static KNOWN: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    KNOWN.get_or_init(Mutex::default)
}

/// Remembers how many tokens a model reads at most. Nothing for 0.
pub fn remember(model: &str, tokens: u32) {
    if tokens > 0 && !model.is_empty() {
        known().lock().unwrap().insert(model.to_string(), tokens);
    }
}

/// How many tokens a model reads at most, if its provider said.
pub fn of(model: &str) -> Option<u32> {
    known().lock().unwrap().get(model).copied()
}

/// What one answer cost: the prompt, how much of it came from the provider's cache, the
/// answer and the thinking, in tokens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Spent {
    pub prompt: u32,
    pub cached: u32,
    pub output: u32,
    pub thinking: u32,
}

impl Spent {
    /// From the usage of an OpenAI-shaped answer; nothing where it gives none.
    pub fn of_openai(answer: &str) -> Spent {
        let value: serde_json::Value = serde_json::from_str(answer).unwrap_or_default();
        let usage = &value["usage"];
        let n = |p: &str| usage.pointer(p).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        Spent {
            prompt: n("/prompt_tokens"),
            // OpenAI, OpenRouter and OpenCode say it one way, DeepSeek another.
            cached: n("/prompt_tokens_details/cached_tokens").max(n("/prompt_cache_hit_tokens")).max(n("/cache_read_input_tokens")),
            output: n("/completion_tokens"),
            thinking: n("/completion_tokens_details/reasoning_tokens"),
        }
    }

    /// The share of the prompt that came from the cache, in percent.
    pub fn cache_share(&self) -> u32 {
        if self.prompt == 0 { 0 } else { (self.cached as u64 * 100 / self.prompt as u64) as u32 }
    }
}

fn last() -> &'static Mutex<Option<(String, Spent)>> {
    static LAST: OnceLock<Mutex<Option<(String, Spent)>>> = OnceLock::new();
    LAST.get_or_init(Mutex::default)
}

/// Keeps what the last answer cost, and by which model, for the player to see.
pub fn spent(model: &str, spent: &Spent) {
    if spent.prompt > 0 {
        *last().lock().unwrap() = Some((model.to_string(), *spent));
    }
}

/// What the last answer cost, if any said.
pub fn last_spent() -> Option<(String, Spent)> {
    last().lock().unwrap().clone()
}

/// A refusal that says the request was longer than the model reads teaches how much it
/// does read, where the message says: "the maximum number of tokens allowed (1048576)",
/// "maximum context length is 8192 tokens". Gives whether something was learned.
pub fn learn(model: &str, message: &str) -> bool {
    let text = message.to_lowercase();
    let about_length = ["context length", "context window", "maximum number of tokens", "too many tokens", "input token count", "prompt is too long", "maximum context"].iter().any(|w| text.contains(w));
    if !about_length {
        return false;
    }
    // The limit is the number after the words that name it, or else the largest one.
    let numbers: Vec<u64> = text.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse().ok()).filter(|n| *n >= 1_000).collect();
    let after = ["allowed", "maximum context length is", "context length is", "limit of", "maximum of"].iter().find_map(|w| text.find(w).and_then(|at| text[at..].split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse::<u64>().ok()).find(|n| *n >= 1_000)));
    match after.or_else(|| numbers.iter().copied().min()) {
        Some(tokens) => {
            remember(model, tokens.min(u32::MAX as u64) as u32);
            true
        }
        None => false,
    }
}

/// The context of a model in a list of models, whichever of the names providers give it.
pub fn read(entry: &serde_json::Value) -> Option<u32> {
    let number = |v: &serde_json::Value| v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()));
    let direct = ["context_length", "context_window", "max_context_length", "max_model_len", "inputTokenLimit", "max_input_tokens"].iter().find_map(|k| entry.get(*k).and_then(number));
    let nested = entry.pointer("/top_provider/context_length").and_then(number).or_else(|| entry.pointer("/limit/context").and_then(number));
    // Cloudflare lists its numbers as properties.
    let property = entry.get("properties").and_then(|p| p.as_array()).and_then(|list| list.iter().find(|p| p.get("property_id").and_then(|i| i.as_str()) == Some("context_window")).and_then(|p| p.get("value")).and_then(number));
    direct.or(nested).or(property).filter(|n| *n > 0).map(|n| n.min(u32::MAX as u64) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_context_is_read_from_every_providers_list() {
        assert_eq!(read(&json!({ "id": "a", "context_length": 131072 })), Some(131_072));
        assert_eq!(read(&json!({ "id": "b", "context_window": 8192 })), Some(8_192));
        assert_eq!(read(&json!({ "id": "c", "top_provider": { "context_length": 200000 } })), Some(200_000));
        assert_eq!(read(&json!({ "id": "d", "limit": { "context": 256000, "output": 32000 } })), Some(256_000));
        assert_eq!(read(&json!({ "name": "@cf/x", "properties": [{ "property_id": "context_window", "value": "24000" }] })), Some(24_000));
        assert_eq!(read(&json!({ "id": "e" })), None);
        assert!(learn("g", "The input token count (1200000) exceeds the maximum number of tokens allowed (1048576)."));
        assert_eq!(of("g"), Some(1_048_576));
        assert!(learn("o", "This model's maximum context length is 8192 tokens. However, you requested 9000 tokens."));
        assert_eq!(of("o"), Some(8_192));
        assert!(!learn("q", "Resource has been exhausted (e.g. check quota)."));
        remember("model-x", 64_000);
        assert_eq!(of("model-x"), Some(64_000));
    }

    #[test]
    fn what_a_streamed_answer_cost_and_what_the_cache_saved_are_kept() {
        let lines = [
            r#"data: {"choices":[{"delta":{"content":"{\"a\":"}}]}"#,
            r#"data: {"choices":[{"delta":{"content":"1}"}}]}"#,
            r#"data: {"choices":[],"usage":{"prompt_tokens":6000,"completion_tokens":300,"prompt_tokens_details":{"cached_tokens":4500}}}"#,
            "data: [DONE]",
        ];
        let answer = crate::providers::gather_stream(lines.iter().map(|l| Ok(l.to_string()))).unwrap();
        assert_eq!(crate::openai::read_answer(&answer).unwrap(), "{\"a\":1}");
        let spent = Spent::of_openai(&answer);
        assert_eq!((spent.prompt, spent.cached, spent.output, spent.cache_share()), (6000, 4500, 300, 75));
        super::spent("OpenCode Go: kimi", &spent);
        assert_eq!(last_spent().map(|(m, s)| (m, s.cached)), Some(("OpenCode Go: kimi".to_string(), 4500)));
    }
}
