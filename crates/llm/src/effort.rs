//! How hard a model is asked to think. Providers name the levels alike (none, minimal, low,
//! medium, high, xhigh, max) but each model takes only some of them; which ones is read from
//! OpenCode's catalog, which lists them for every provider's models, and the nearest one to
//! the player's setting is asked for.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Levels of thinking from least to most, as providers name them.
const LEVELS: [&str; 7] = ["none", "minimal", "low", "medium", "high", "xhigh", "max"];
/// How long the catalog is believed before it is read again.
const FRESH_FOR: Duration = Duration::from_secs(30 * 60);

/// The level to ask a model for: the one wanted when the model takes it, otherwise the
/// nearest it takes (the higher of two as near). Nothing wanted is "none".
pub fn level_among(takes: &[String], wanted: &str) -> String {
    let wanted = if wanted.trim().is_empty() { "none" } else { wanted.trim() };
    if takes.is_empty() || takes.iter().any(|t| t == wanted) {
        return wanted.to_string();
    }
    let rank = |l: &str| LEVELS.iter().position(|x| *x == l).unwrap_or(4) as i32;
    let want = rank(wanted);
    takes.iter().min_by_key(|t| ((rank(t) - want).abs(), -rank(t))).cloned().unwrap_or_else(|| wanted.to_string())
}

fn catalog() -> Option<serde_json::Value> {
    static CACHE: OnceLock<Mutex<Option<(Instant, serde_json::Value)>>> = OnceLock::new();
    let slot = CACHE.get_or_init(Mutex::default);
    if let Some((when, value)) = slot.lock().unwrap().as_ref()
        && when.elapsed() < FRESH_FOR
    {
        return Some(value.clone());
    }
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout(Duration::from_secs(30)).build();
    let read = agent.get(crate::opencode_go::CATALOG).call().ok().and_then(|r| r.into_string().ok()).and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    match read {
        Some(value) => {
            *slot.lock().unwrap() = Some((Instant::now(), value.clone()));
            Some(value)
        }
        // Not read: what was read before stands, and it is tried again later.
        None => slot.lock().unwrap().as_ref().map(|(_, v)| v.clone()),
    }
}

/// The levels a model of a provider takes, as the catalog lists them: under the provider's
/// name there ("google", "openrouter", "nvidia", "cloudflare-workers-ai", "opencode-go"),
/// by the model's id. Empty when the catalog does not say.
pub fn efforts(provider: &str, model: &str) -> Vec<String> {
    let Some(catalog) = catalog() else { return Vec::new() };
    efforts_in(&catalog, provider, model)
}

pub fn efforts_in(catalog: &serde_json::Value, provider: &str, model: &str) -> Vec<String> {
    let models = &catalog[provider]["models"];
    let entry = models.get(model).or_else(|| models.as_object().and_then(|all| all.values().find(|m| m["id"].as_str() == Some(model))));
    entry.and_then(|m| m["capabilities"]["reasoning"]["effort"].as_array()).map(|list| list.iter().filter_map(|e| e.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(levels: &[&str]) -> Vec<String> {
        levels.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_model_is_asked_for_the_nearest_level_it_takes() {
        let flash = list(&["low", "high", "max"]);
        assert_eq!(level_among(&flash, "medium"), "high", "no medium: the higher of the two as near");
        assert_eq!(level_among(&flash, "xhigh"), "max");
        assert_eq!(level_among(&flash, "none"), "low", "it cannot stop thinking: the least it takes");
        let gemini = list(&["low", "medium", "high", "xhigh"]);
        assert_eq!(level_among(&gemini, "max"), "xhigh");
        assert_eq!(level_among(&gemini, "none"), "low");
        let usual = list(&["low", "medium", "high"]);
        assert_eq!(level_among(&usual, "max"), "high");
        assert_eq!(level_among(&[], "xhigh"), "xhigh", "a model that does not say is asked as wanted");
    }

    #[test]
    fn the_levels_are_read_for_each_providers_model() {
        let catalog = serde_json::json!({
            "google": { "models": { "gemini-4-hydrogen": { "id": "gemini-4-hydrogen", "capabilities": { "reasoning": { "effort": ["low", "medium", "high", "xhigh"] } } } } },
            "openrouter": { "models": { "deepseek/deepseek-v4.1-flash": { "id": "deepseek/deepseek-v4.1-flash", "capabilities": { "reasoning": { "effort": ["none", "low", "high", "max"] } } } } }
        });
        assert_eq!(efforts_in(&catalog, "google", "gemini-4-hydrogen"), list(&["low", "medium", "high", "xhigh"]));
        assert_eq!(efforts_in(&catalog, "openrouter", "deepseek/deepseek-v4.1-flash").last().map(String::as_str), Some("max"));
        assert!(efforts_in(&catalog, "nvidia", "x").is_empty());
    }
}
