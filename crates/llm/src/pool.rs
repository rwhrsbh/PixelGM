//! Chooses the next key and model to try. Quotas are counted per model for each key, so the
//! resting state is kept per (key, model) pair, never per key.

use crate::catalog::{Model, Tier};
use serde::{Deserialize, Serialize};
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
    /// The model took the connection and then said nothing at all. Not the key's fault, and
    /// asking again soon would cost the same silence, so it rests longer than an overload.
    Silent,
    /// This key cannot see the model: it was removed, or this key's project never had it.
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

/// One model across all live keys, for the quota meter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Availability {
    pub free_keys: u32,
    pub live_keys: u32,
    pub next_free: Option<Millis>,
}

/// A spent quota as it is written to disk, so a restart does not forget it and burn requests
/// rediscovering it. The key is named by a fingerprint, never by its secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedRest {
    pub key: String,
    pub model: String,
    pub until: Millis,
    pub limit: Option<u32>,
}

/// A short stable name for a key that does not reveal it (FNV-1a, 64 bits).
pub fn fingerprint(secret: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in secret.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

const PAIR_BACKOFF: (Millis, Millis) = (30_000, 3_600_000);
const MODEL_BACKOFF: (Millis, Millis) = (20_000, 300_000);
const SILENT_BACKOFF: (Millis, Millis) = (5 * 60_000, 3_600_000);
const GONE_REST: Millis = 24 * 3_600_000;

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
    /// Saved rests of keys that are not entered right now. They wait here so the key gets
    /// them back whenever it is added, and so saving does not forget them.
    waiting_rests: Vec<SavedRest>,
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
        let print = fingerprint(secret);
        let (mine, others): (Vec<SavedRest>, Vec<SavedRest>) = std::mem::take(&mut self.waiting_rests).into_iter().partition(|r| r.key == print);
        self.waiting_rests = others;
        for rest in mine {
            self.rest_pair(id, rest);
        }
        id
    }

    fn rest_pair(&mut self, id: KeyId, rest: SavedRest) {
        let pair = (id, rest.model);
        self.pair_rest.insert(pair.clone(), rest.until);
        let usage = self.usage.entry(pair).or_default();
        usage.resting_until = Some(rest.until);
        usage.limit = rest.limit;
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

    /// Every key in the order it was added, rejected ones included: id, secret, rejected.
    pub fn keys(&self) -> impl Iterator<Item = (KeyId, &str, bool)> {
        self.keys.iter().map(|k| (k.id, k.secret.as_str(), k.dead))
    }

    /// How many live keys can call `model` right now, out of how many, and when the next one
    /// frees up if none can.
    pub fn availability(&self, now: Millis, model: &str) -> Availability {
        let model_rest = self.model_rest.get(model).copied().unwrap_or(0);
        let mut out = Availability::default();
        for key in self.keys.iter().filter(|k| !k.dead) {
            out.live_keys += 1;
            let free_at = model_rest.max(self.pair_rest.get(&(key.id, model.to_string())).copied().unwrap_or(0));
            if free_at <= now {
                out.free_keys += 1;
            } else {
                out.next_free = Some(out.next_free.map_or(free_at, |t| t.min(free_at)));
            }
        }
        out
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

    /// The quota rests still running at `now`, for saving.
    pub fn export_rests(&self, now: Millis) -> Vec<SavedRest> {
        let mut rests: Vec<SavedRest> = self
            .pair_rest
            .iter()
            .filter(|(_, until)| **until > now)
            .filter_map(|((id, model), until)| {
                let key = self.keys.iter().find(|k| k.id == *id)?;
                let limit = self.usage.get(&(*id, model.clone())).and_then(|u| u.limit);
                Some(SavedRest { key: fingerprint(&key.secret), model: model.clone(), until: *until, limit })
            })
            .collect();
        rests.extend(self.waiting_rests.iter().filter(|r| r.until > now).cloned());
        rests.sort_by(|a, b| (&a.key, &a.model).cmp(&(&b.key, &b.model)));
        rests
    }

    /// Restores saved rests. Expired ones are dropped; rests of keys that are not entered yet
    /// wait for the key to be added.
    pub fn import_rests(&mut self, rests: &[SavedRest], now: Millis) {
        for rest in rests.iter().filter(|r| r.until > now) {
            match self.keys.iter().find(|k| fingerprint(&k.secret) == rest.key).map(|k| k.id) {
                Some(id) => self.rest_pair(id, rest.clone()),
                None => self.waiting_rests.push(rest.clone()),
            }
        }
    }

    /// Best model that can be called now, no worse than `lowest`, with a key that still has
    /// quota for it. Models are tried in chain order and keys in the order they were added.
    pub fn pick(&self, now: Millis, lowest: Tier) -> Pick {
        self.pick_but(now, lowest, &[])
    }

    /// As `pick`, leaving out the models named: those that would not answer this request.
    pub fn pick_but(&self, now: Millis, lowest: Tier, skip: &[String]) -> Pick {
        let mut soonest: Option<Millis> = None;
        let mut note = |t: Millis| soonest = Some(soonest.map_or(t, |s: Millis| s.min(t)));
        let mut any_key = false;
        for model in self.chain.iter().filter(|m| m.tier <= lowest && !skip.contains(&m.id)) {
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

    /// As `pick`, but from the cheap end: the lite and the open models first, so that small
    /// talk does not eat the quota the story needs. Anything that is free, failing those.
    pub fn pick_cheap(&self, now: Millis) -> Pick {
        for model in self.chain.iter().filter(|m| m.tier != Tier::Flash) {
            let model_rest = self.model_rest.get(&model.id).copied().unwrap_or(0);
            for key in self.keys.iter().filter(|k| !k.dead) {
                let pair_rest = self.pair_rest.get(&(key.id, model.id.clone())).copied().unwrap_or(0);
                if model_rest.max(pair_rest) <= now {
                    return Pick::Ready { model: model.clone(), key: key.id, secret: key.secret.clone() };
                }
            }
        }
        self.pick(now, Tier::Gemma)
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
            Outcome::Silent => {
                let strikes = self.model_strikes.entry(model.to_string()).or_insert(0);
                *strikes += 1;
                self.model_rest.insert(model.to_string(), now + backoff(SILENT_BACKOFF, *strikes));
            }
            Outcome::ModelGone => {
                // Model lists differ between keys, so a 404 speaks for this key only. The pair
                // rests for long: a relist may keep naming the model, and without the rest the
                // same request would come straight back.
                self.pair_rest.insert(pair, now + GONE_REST);
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
    fn a_model_that_goes_quiet_rests_longer_than_an_overloaded_one() {
        let (mut p, a, _) = pool();
        p.report(0, "big", a, Outcome::Silent);
        // An overload is admitted to and cleared in twenty seconds; silence is not.
        assert_eq!(picked(&p, 20_000, Tier::Gemma), ("small".into(), a));
        assert_eq!(picked(&p, 300_000, Tier::Gemma), ("big".into(), a));
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
    fn model_missing_for_one_key_is_still_served_by_another() {
        let (mut p, a, b) = pool();
        p.report(0, "big", a, Outcome::ModelGone);
        assert!(p.needs_relist());
        assert_eq!(picked(&p, 0, Tier::Gemma), ("big".into(), b));
        p.report(0, "big", b, Outcome::ModelGone);
        assert_eq!(picked(&p, 0, Tier::Gemma), ("small".into(), a));
        // A relist that still names the model must not bring the same 404 back.
        let chain = p.chain().to_vec();
        p.set_chain(chain);
        assert_eq!(picked(&p, 1, Tier::Gemma).0, "small");
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
    fn availability_counts_free_keys_and_names_the_next_release() {
        let (mut p, a, b) = pool();
        assert_eq!(p.availability(0, "big"), Availability { free_keys: 2, live_keys: 2, next_free: None });
        p.report(0, "big", a, limited(600));
        p.report(0, "big", b, limited(900));
        assert_eq!(p.availability(1, "big"), Availability { free_keys: 0, live_keys: 2, next_free: Some(600_000) });
        assert_eq!(p.availability(1, "small").free_keys, 2);
        p.report(1, "small", b, Outcome::KeyRejected);
        assert_eq!(p.availability(1, "small"), Availability { free_keys: 1, live_keys: 1, next_free: None });
        assert_eq!(p.keys().map(|(id, _, dead)| (id, dead)).collect::<Vec<_>>(), [(a, false), (b, true)]);
    }

    #[test]
    fn spent_quota_survives_a_restart_without_storing_the_key() {
        let (mut p, a, _) = pool();
        p.report(0, "big", a, limited(600));
        p.report(0, "small", a, limited(5));
        let saved = p.export_rests(40_000);
        // The short rest (stretched to the 30 s minimum) is already over at the moment of saving.
        assert_eq!(saved, [SavedRest { key: fingerprint("key-a"), model: "big".into(), until: 600_000, limit: Some(20) }]);
        assert!(!format!("{saved:?}").contains("key-a"));

        let (mut fresh, a2, b2) = pool();
        fresh.import_rests(&saved, 40_000);
        assert_eq!(picked(&fresh, 40_000, Tier::Gemma), ("big".into(), b2));
        assert_eq!(fresh.usage(a2, "big"), PairUsage { used: 0, limit: Some(20), resting_until: Some(600_000) });
        assert_eq!(fresh.availability(40_000, "big").free_keys, 1);
    }

    #[test]
    fn stale_rests_are_dropped_and_an_absent_keys_rest_waits_for_the_key() {
        let (mut p, a, _) = pool();
        let late = SavedRest { key: fingerprint("key-late"), model: "big".into(), until: 9_000_000, limit: Some(20) };
        p.import_rests(&[SavedRest { key: fingerprint("key-a"), model: "big".into(), until: 500, limit: None }, late.clone()], 1_000);
        assert_eq!(picked(&p, 1_000, Tier::Gemma), ("big".into(), a));
        assert_eq!(p.availability(1_000, "big").free_keys, 2);
        // Saving before the key comes back must not lose its rest.
        assert_eq!(p.export_rests(1_000), [late.clone()]);
        let c = p.add_key("key-late");
        assert_eq!(p.usage(c, "big").resting_until, Some(9_000_000));
        assert_eq!(p.availability(1_000, "big"), Availability { free_keys: 2, live_keys: 3, next_free: Some(9_000_000) });
        assert_eq!(p.export_rests(1_000), [late]);
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
