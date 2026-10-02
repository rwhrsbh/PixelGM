//! Chooses the next key and model to try. Quotas are counted per model for each key, so the
//! resting state is kept per (key, model) pair, never per key.

use crate::catalog::{Model, Tier};
use std::collections::HashMap;

/// Milliseconds on whatever clock the caller uses; the pool only compares and adds.
pub type Millis = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyId(pub u32);

#[derive(Debug, Clone)]
struct Key {
    id: KeyId,
    secret: String,
    dead: bool,
}

/// What happened to one request, as far as rotation is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    /// Quota for this key on this model is spent. `limit` is the quota size when the server named it.
    RateLimited { retry_after: Option<Millis>, limit: Option<u32> },
    /// The model itself is overloaded or failing; another key would meet the same answer.
    Overloaded,
    /// The model no longer exists.
    ModelGone,
    /// The key is invalid or revoked.
    KeyRejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    Ready { model: Model, key: KeyId, secret: String },
    /// Everything usable is resting; the earliest moment something frees up.
    Wait { until: Millis },
    /// No live keys or no models at all: waiting will not help.
    Nothing,
}

/// What the quota meter shows for one key on one model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PairUsage {
    /// Successful requests since this pair last came back from a rate limit.
    pub used: u32,
    pub limit: Option<u32>,
    pub resting_until: Option<Millis>,
}

const PAIR_BACKOFF: (Millis, Millis) = (30_000, 3_600_000);
const MODEL_BACKOFF: (Millis, Millis) = (20_000, 300_000);
const GONE_REST: Millis = 3_600_000;

#[derive(Debug, Default)]
pub struct Pool {
    keys: Vec<Key>,
    next_key: u32,
    chain: Vec<Model>,
    pair_rest: HashMap<(KeyId, String), Millis>,
    model_rest: HashMap<String, Millis>,
    pair_strikes: HashMap<(KeyId, String), u32>,
    model_strikes: HashMap<String, u32>,
    usage: HashMap<(KeyId, String), PairUsage>,
    needs_relist: bool,
}

fn backoff((base, cap): (Millis, Millis), strikes: u32) -> Millis {
    base.saturating_mul(1u64 << strikes.saturating_sub(1).min(20)).min(cap)
}

impl Pool {
    pub fn new() -> Self {
        Self { needs_relist: true, ..Self::default() }
    }

    /// Adds a key unless it is already present; either way returns its id.
    pub fn add_key(&mut self, secret: &str) -> KeyId {
        let secret = secret.trim();
        if let Some(k) = self.keys.iter_mut().find(|k| k.secret == secret) {
            k.dead = false;
            return k.id;
        }
        let id = KeyId(self.next_key);
        self.next_key += 1;
        self.keys.push(Key { id, secret: secret.to_string(), dead: false });
        id
    }

    pub fn remove_key(&mut self, id: KeyId) {
        self.keys.retain(|k| k.id != id);
        self.pair_rest.retain(|(k, _), _| *k != id);
        self.pair_strikes.retain(|(k, _), _| *k != id);
        self.usage.retain(|(k, _), _| *k != id);
    }

    pub fn live_keys(&self) -> impl Iterator<Item = (KeyId, &str)> {
        self.keys.iter().filter(|k| !k.dead).map(|k| (k.id, k.secret.as_str()))
    }

    pub fn is_dead(&self, id: KeyId) -> bool {
        self.keys.iter().any(|k| k.id == id && k.dead)
    }

    pub fn set_chain(&mut self, chain: Vec<Model>) {
        self.chain = chain;
        self.needs_relist = false;
    }

    pub fn chain(&self) -> &[Model] {
        &self.chain
    }

    /// True when the chain is missing or a model vanished since the last listing.
    pub fn needs_relist(&self) -> bool {
        self.needs_relist
    }

    pub fn usage(&self, key: KeyId, model: &str) -> PairUsage {
        self.usage.get(&(key, model.to_string())).copied().unwrap_or_default()
    }

    /// Best model that can be called now, no worse than `lowest`, with a key that still has
    /// quota for it. Models are tried in chain order and keys in the order they were added.
    pub fn pick(&self, now: Millis, lowest: Tier) -> Pick {
        let mut soonest: Option<Millis> = None;
        let mut note = |t: Millis| soonest = Some(soonest.map_or(t, |s: Millis| s.min(t)));
        let mut any_key = false;
        for model in self.chain.iter().filter(|m| m.tier <= lowest) {
            let model_rest = self.model_rest.get(&model.id).copied().unwrap_or(0);
            for key in self.keys.iter().filter(|k| !k.dead) {
                any_key = true;
                let pair_rest = self.pair_rest.get(&(key.id, model.id.clone())).copied().unwrap_or(0);
                let free_at = model_rest.max(pair_rest);
                if free_at <= now {
                    return Pick::Ready { model: model.clone(), key: key.id, secret: key.secret.clone() };
                }
                note(free_at);
            }
        }
        match soonest {
            Some(until) if any_key => Pick::Wait { until },
            _ => Pick::Nothing,
        }
    }

    pub fn report(&mut self, now: Millis, model: &str, key: KeyId, outcome: Outcome) {
        let pair = (key, model.to_string());
        match outcome {
            Outcome::Ok => {
                self.pair_strikes.remove(&pair);
                self.model_strikes.remove(model);
                let usage = self.usage.entry(pair).or_default();
                if usage.resting_until.is_some_and(|t| t <= now) {
                    *usage = PairUsage { limit: usage.limit, ..PairUsage::default() };
                }
                usage.used += 1;
            }
            Outcome::RateLimited { retry_after, limit } => {
                let strikes = self.pair_strikes.entry(pair.clone()).or_insert(0);
                *strikes += 1;
                let rest = retry_after.unwrap_or(0).max(backoff(PAIR_BACKOFF, *strikes));
                self.pair_rest.insert(pair.clone(), now + rest);
                let usage = self.usage.entry(pair).or_default();
                usage.resting_until = Some(now + rest);
                if limit.is_some() {
                    usage.limit = limit;
                }
            }
            Outcome::Overloaded => {
                let strikes = self.model_strikes.entry(model.to_string()).or_insert(0);
                *strikes += 1;
                self.model_rest.insert(model.to_string(), now + backoff(MODEL_BACKOFF, *strikes));
            }
            Outcome::ModelGone => {
                self.chain.retain(|m| m.id != model);
                // The list can keep naming a model that no longer answers; without this rest a
                // relist would hand it straight back.
                self.model_rest.insert(model.to_string(), now + GONE_REST);
                self.needs_relist = true;
            }
            Outcome::KeyRejected => {
                if let Some(k) = self.keys.iter_mut().find(|k| k.id == key) {
                    k.dead = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ApiVersion;

    fn model(id: &str, tier: Tier) -> Model {
        Model { id: id.to_string(), api: ApiVersion::V1Beta, tier, input_token_limit: 0, output_token_limit: 0 }
    }

    fn pool() -> (Pool, KeyId, KeyId) {
        let mut p = Pool::new();
        let a = p.add_key("key-a");
        let b = p.add_key("key-b");
        p.set_chain(vec![model("big", Tier::Flash), model("small", Tier::Flash), model("lite", Tier::FlashLite), model("gemma", Tier::Gemma)]);
        (p, a, b)
    }

    fn picked(p: &Pool, now: Millis, lowest: Tier) -> (String, KeyId) {
        match p.pick(now, lowest) {
            Pick::Ready { model, key, .. } => (model.id, key),
            other => panic!("expected a ready pick, got {other:?}"),
        }
    }

    fn limited(seconds: u64) -> Outcome {
        Outcome::RateLimited { retry_after: Some(seconds * 1000), limit: Some(20) }
    }

    #[test]
    fn exhausted_model_moves_to_next_key_then_to_next_model() {
        let (mut p, a, b) = pool();
        assert_eq!(picked(&p, 0, Tier::Gemma), ("big".into(), a));
        p.report(0, "big", a, limited(600));
        assert_eq!(picked(&p, 1, Tier::Gemma), ("big".into(), b));
        p.report(1, "big", b, limited(600));
        // The same key that ran dry on "big" is fresh for "small": limits are per model.
        assert_eq!(picked(&p, 2, Tier::Gemma), ("small".into(), a));
    }

    #[test]
    fn pair_comes_back_after_the_servers_delay() {
        let (mut p, a, b) = pool();
        p.report(0, "big", a, limited(600));
        p.report(0, "big", b, limited(900));
        assert_eq!(picked(&p, 599_999, Tier::Gemma).0, "small");
        assert_eq!(picked(&p, 600_000, Tier::Gemma), ("big".into(), a));
    }

    #[test]
    fn overload_rests_the_model_for_every_key() {
        let (mut p, a, _) = pool();
        p.report(0, "big", a, Outcome::Overloaded);
        assert_eq!(picked(&p, 1, Tier::Gemma), ("small".into(), a));
        assert_eq!(picked(&p, 20_000, Tier::Gemma), ("big".into(), a));
    }

    #[test]
    fn repeated_overload_waits_longer_and_success_resets_it() {
        let (mut p, a, _) = pool();
        p.report(0, "big", a, Outcome::Overloaded);
        p.report(20_000, "big", a, Outcome::Overloaded);
        assert_eq!(picked(&p, 59_999, Tier::Gemma).0, "small");
        assert_eq!(picked(&p, 60_000, Tier::Gemma).0, "big");
        p.report(60_000, "big", a, Outcome::Ok);
        p.report(60_001, "big", a, Outcome::Overloaded);
        assert_eq!(picked(&p, 80_001, Tier::Gemma).0, "big");
    }

    #[test]
    fn tier_floor_refuses_lower_tiers_and_reports_when_flash_returns() {
        let (mut p, a, b) = pool();
        for (model, key) in [("big", a), ("big", b), ("small", a), ("small", b)] {
            p.report(0, model, key, limited(300));
        }
        assert_eq!(p.pick(1, Tier::Flash), Pick::Wait { until: 300_000 });
        assert_eq!(picked(&p, 1, Tier::FlashLite).0, "lite");
    }

    #[test]
    fn rejected_key_is_skipped_and_all_dead_means_nothing() {
        let (mut p, a, b) = pool();
        p.report(0, "big", a, Outcome::KeyRejected);
        assert_eq!(picked(&p, 0, Tier::Gemma), ("big".into(), b));
        p.report(0, "big", b, Outcome::KeyRejected);
        assert_eq!(p.pick(0, Tier::Gemma), Pick::Nothing);
    }

    #[test]
    fn vanished_model_leaves_the_chain_and_asks_for_a_relist() {
        let (mut p, a, _) = pool();
        p.report(0, "big", a, Outcome::ModelGone);
        assert!(p.needs_relist());
        assert_eq!(picked(&p, 0, Tier::Gemma).0, "small");
    }

    #[test]
    fn key_added_mid_game_is_used_at_once() {
        let (mut p, a, b) = pool();
        for model in ["big", "small", "lite", "gemma"] {
            p.report(0, model, a, limited(3600));
            p.report(0, model, b, limited(3600));
        }
        assert!(matches!(p.pick(1, Tier::Gemma), Pick::Wait { .. }));
        let c = p.add_key("key-c");
        assert_eq!(picked(&p, 1, Tier::Gemma), ("big".into(), c));
    }

    #[test]
    fn meter_counts_requests_and_remembers_the_limit() {
        let (mut p, a, _) = pool();
        p.report(0, "big", a, Outcome::Ok);
        p.report(1, "big", a, Outcome::Ok);
        p.report(2, "big", a, limited(100));
        assert_eq!(p.usage(a, "big"), PairUsage { used: 2, limit: Some(20), resting_until: Some(100_002) });
        p.report(100_002, "big", a, Outcome::Ok);
        assert_eq!(p.usage(a, "big"), PairUsage { used: 1, limit: Some(20), resting_until: None });
    }

    #[test]
    fn rate_limit_without_a_delay_backs_off_on_its_own() {
        let (mut p, a, b) = pool();
        p.report(0, "big", a, Outcome::RateLimited { retry_after: None, limit: None });
        p.report(0, "big", b, Outcome::RateLimited { retry_after: None, limit: None });
        assert_eq!(picked(&p, 29_999, Tier::Gemma).0, "small");
        assert_eq!(picked(&p, 30_000, Tier::Gemma).0, "big");
    }
}
