//! The game master's queues. A thing done that needs the master's word is always taken: it
//! waits in the queue of the place it was done in. A place is asked one question at a time,
//! and what gathered there goes as one question, so that two heroes acting together get one
//! scene. Places are asked side by side, up to as many at once as the table allows.

use crate::Millis;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ActId(pub u64);

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct RequestId(pub u64);

/// How the table wants its master asked.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(default)]
pub struct Rules {
    /// What several players of one place do within the window goes as one question.
    pub batch: bool,
    /// How long the first thing done waits for company, and the longest it may be kept
    /// waiting while somebody there is still typing.
    pub window: Millis,
    pub window_cap: Millis,
    /// How many places may be asked at once.
    pub parallel: usize,
}

impl Default for Rules {
    fn default() -> Self {
        Self { batch: true, window: 1_500, window_cap: 6_000, parallel: 5 }
    }
}

/// How long a player counts as typing after the last key.
const TYPING_LASTS: Millis = 3_000;

/// Something a player did that the master must answer.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Act {
    pub id: ActId,
    pub player: String,
    /// What the player wrote, or the choice they took.
    pub said: String,
    /// The thing they stand next to, when it matters.
    pub near: Option<String>,
    /// A choice whose outcome is already told: the master is asked what follows it.
    pub follows: bool,
    pub at: Millis,
    /// Sent back once already because the place had changed under it.
    returned: bool,
    /// The player let what was offered pass: closed it or skipped it. Told to the master
    /// with what the others did; asked of it on its own, never.
    #[serde(default)]
    pub passed: bool,
}

/// Where something done stands. Every change of it is told, so nothing done is ever lost
/// without a word.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub enum ActState {
    /// Waiting, with how many are ahead of it in its place.
    Queued { ahead: usize },
    /// With the master.
    Asked,
    /// Answered: the telling follows.
    Done,
    /// The master could not answer; the words say why. It may be sent again.
    Refused(String),
    /// Taken back by the player before it was asked.
    Withdrawn,
    /// Did not happen: the player left the place, or their hero fell.
    Void(String),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Out {
    /// Ask the master about a place: everything in `acts` at once, in this order.
    Ask { id: RequestId, place: String, acts: Vec<Act> },
    /// Tell a player where something of theirs stands.
    State { act: ActId, player: String, state: ActState },
}

/// A question that is with the master.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Flight {
    pub id: RequestId,
    pub place: String,
    pub acts: Vec<Act>,
    pub since: Millis,
}

#[derive(Default, Debug)]
struct Scene {
    waiting: Vec<Act>,
    /// When what waits is sent, if nothing is being asked here.
    window_until: Option<Millis>,
    /// When the oldest of what waits was taken: the window is never stretched past the cap.
    window_from: Millis,
    typing_until: Millis,
    asked: Option<RequestId>,
    /// Players who were offered something here and have not answered it yet: what is done
    /// here waits for each of them to choose, write, close it or skip it.
    expecting: Vec<String>,
}

#[derive(Default, Debug)]
pub struct Queues {
    pub rules: Rules,
    scenes: BTreeMap<String, Scene>,
    flights: BTreeMap<RequestId, Flight>,
    next: u64,
}

impl Queues {
    pub fn new(rules: Rules) -> Self {
        Self { rules, ..Self::default() }
    }

    fn id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    /// Takes something done. It is never refused: it waits its turn. `alone` when nobody
    /// else is in the place, so there is nobody to wait for.
    pub fn submit(&mut self, place: &str, player: &str, said: &str, near: Option<String>, follows: bool, alone: bool, now: Millis) -> (ActId, Vec<Out>) {
        let id = ActId(self.id());
        let (batch, window) = (self.rules.batch, self.rules.window);
        let scene = self.scenes.entry(place.to_string()).or_default();
        // A player who adds to what they have waiting is still doing one thing.
        if let Some(mine) = scene.waiting.iter_mut().find(|a| a.player == player && !a.follows && !follows) {
            mine.said = format!("{} {}", mine.said.trim(), said.trim());
            mine.near = near.or(mine.near.take());
            let ahead = scene.waiting.iter().position(|a| a.player == player).unwrap_or(0);
            let state = Out::State { act: scene.waiting[ahead].id, player: player.to_string(), state: ActState::Queued { ahead } };
            return (scene.waiting[ahead].id, vec![state]);
        }
        // Where players were offered something, they are waited for instead of a window.
        let waited = !scene.expecting.is_empty();
        if scene.waiting.is_empty() {
            scene.window_from = now;
            scene.window_until = Some(if batch && !alone && !waited { now + window } else { now });
        }
        scene.waiting.push(Act { id, player: player.to_string(), said: said.trim().to_string(), near, follows, at: now, returned: false, passed: false });
        scene.expecting.retain(|p| p != player);
        if waited {
            scene.window_until = Some(now);
        }
        let ahead = scene.waiting.len() - 1;
        let mut out = vec![Out::State { act: id, player: player.to_string(), state: ActState::Queued { ahead } }];
        out.extend(self.tick(now));
        (id, out)
    }

    /// These players were offered something here: what is done here is asked only once each
    /// of them has done something, or let it pass.
    pub fn expect(&mut self, place: &str, players: &[String]) {
        let scene = self.scenes.entry(place.to_string()).or_default();
        scene.expecting = players.to_vec();
    }

    /// A player let what was offered pass. When others did something meanwhile, the master is
    /// told it with theirs; alone it asks nothing.
    pub fn pass(&mut self, place: &str, player: &str, now: Millis) -> Vec<Out> {
        let Some(scene) = self.scenes.get_mut(place) else { return Vec::new() };
        if !scene.expecting.iter().any(|p| p == player) {
            return Vec::new();
        }
        scene.expecting.retain(|p| p != player);
        if !scene.waiting.is_empty() {
            let id = ActId(self.next + 1);
            self.next += 1;
            let scene = self.scenes.get_mut(place).expect("just found");
            scene.waiting.push(Act { id, player: player.to_string(), said: String::new(), near: None, follows: false, at: now, returned: false, passed: true });
        }
        self.tick(now)
    }

    /// Who the place is still waiting for.
    pub fn expecting(&self, place: &str) -> Vec<String> {
        self.scenes.get(place).map(|s| s.expecting.clone()).unwrap_or_default()
    }

    /// Somebody in the place is typing: what waits there waits a little longer for them.
    pub fn typing(&mut self, place: &str, now: Millis) {
        if let Some(scene) = self.scenes.get_mut(place) {
            scene.typing_until = now + TYPING_LASTS;
        }
    }

    /// Takes back something that has not been asked yet. `false` when it is with the master
    /// already, or is not this player's.
    pub fn withdraw(&mut self, act: ActId, player: &str) -> Vec<Out> {
        for scene in self.scenes.values_mut() {
            if let Some(at) = scene.waiting.iter().position(|a| a.id == act && a.player == player) {
                scene.waiting.remove(at);
                if scene.waiting.is_empty() {
                    scene.window_until = None;
                }
                return vec![Out::State { act, player: player.to_string(), state: ActState::Withdrawn }];
            }
        }
        Vec::new()
    }

    /// Whether the player has something waiting or with the master.
    pub fn pending_of(&self, player: &str) -> Option<(ActId, ActState)> {
        for flight in self.flights.values() {
            if let Some(act) = flight.acts.iter().find(|a| a.player == player) {
                return Some((act.id, ActState::Asked));
            }
        }
        self.scenes.values().find_map(|scene| scene.waiting.iter().position(|a| a.player == player).map(|ahead| (scene.waiting[ahead].id, ActState::Queued { ahead })))
    }

    /// The place is being asked right now.
    pub fn asking(&self, place: &str) -> bool {
        self.scenes.get(place).is_some_and(|s| s.asked.is_some())
    }

    /// Nothing waits and nothing is with the master, anywhere.
    pub fn idle(&self) -> bool {
        self.flights.is_empty() && self.scenes.values().all(|s| s.waiting.is_empty())
    }

    pub fn flights(&self) -> impl Iterator<Item = &Flight> {
        self.flights.values()
    }

    /// Sends what is due: the places whose window has closed, the one that has waited
    /// longest first, as many as may be asked at once.
    pub fn tick(&mut self, now: Millis) -> Vec<Out> {
        let mut out = Vec::new();
        loop {
            if self.flights.len() >= self.rules.parallel.max(1) {
                break;
            }
            let cap = self.rules.window_cap;
            let due = self
                .scenes
                .iter()
                .filter(|(_, s)| s.asked.is_none() && !s.waiting.is_empty())
                .filter(|(_, s)| {
                    // Those offered something are waited for, however long they take; once
                    // all have answered it goes at once.
                    if !s.expecting.is_empty() {
                        return false;
                    }
                    if s.waiting.iter().any(|a| a.passed) {
                        return true;
                    }
                    let until = s.window_until.unwrap_or(now);
                    // Someone still typing holds the window open, but never past the cap.
                    let held = s.typing_until > now && now < s.window_from + cap;
                    until <= now && !held
                })
                .min_by_key(|(_, s)| s.waiting.iter().map(|a| a.at).min().unwrap_or(now))
                .map(|(place, _)| place.clone());
            let Some(place) = due else { break };
            let id = RequestId(self.id());
            let scene = self.scenes.get_mut(&place).expect("just found");
            let acts: Vec<Act> = if self.rules.batch { std::mem::take(&mut scene.waiting) } else { vec![scene.waiting.remove(0)] };
            // Nothing but letting it pass: nothing to ask.
            if acts.iter().all(|a| a.passed) {
                scene.window_until = None;
                continue;
            }
            scene.asked = Some(id);
            scene.window_until = None;
            for act in &acts {
                out.push(Out::State { act: act.id, player: act.player.clone(), state: ActState::Asked });
            }
            // Those left behind know how far they have come.
            for (ahead, act) in scene.waiting.iter().enumerate() {
                out.push(Out::State { act: act.id, player: act.player.clone(), state: ActState::Queued { ahead } });
            }
            self.flights.insert(id, Flight { id, place: place.clone(), acts: acts.clone(), since: now });
            out.push(Out::Ask { id, place, acts });
        }
        out
    }

    /// The master answered. Gives the question back to be carried out — the caller says
    /// which of its acts happened with `done` or `void` — and lets the place be asked
    /// again. `None` for an answer nobody is waiting for: it is to be thrown away.
    pub fn answered(&mut self, id: RequestId, now: Millis) -> Option<Flight> {
        let flight = self.flights.remove(&id)?;
        if let Some(scene) = self.scenes.get_mut(&flight.place) {
            scene.asked = None;
            // What gathered meanwhile has waited already: it goes at once.
            if !scene.waiting.is_empty() {
                scene.window_until = Some(now);
            }
        }
        Some(flight)
    }

    /// The acts of an answered question that happened.
    pub fn done(&self, acts: &[Act]) -> Vec<Out> {
        acts.iter().map(|a| Out::State { act: a.id, player: a.player.clone(), state: ActState::Done }).collect()
    }

    /// The master could not answer: each of the question's players is told why. The other
    /// places are not touched.
    pub fn failed(&mut self, id: RequestId, why: &str, now: Millis) -> Vec<Out> {
        let Some(flight) = self.answered(id, now) else { return Vec::new() };
        flight.acts.iter().map(|a| Out::State { act: a.id, player: a.player.clone(), state: ActState::Refused(why.to_string()) }).collect()
    }

    /// An answer came to a place that had changed under it — a fight broke out — and was
    /// not carried out: its acts go back to the head of the queue, once. One sent back
    /// before is refused with the reason instead.
    pub fn give_back(&mut self, flight: Flight, why: &str, now: Millis) -> Vec<Out> {
        let mut out = Vec::new();
        let scene = self.scenes.entry(flight.place.clone()).or_default();
        let mut back = Vec::new();
        for mut act in flight.acts {
            if act.returned {
                out.push(Out::State { act: act.id, player: act.player.clone(), state: ActState::Refused(why.to_string()) });
            } else {
                act.returned = true;
                back.push(act);
            }
        }
        if !back.is_empty() {
            back.append(&mut scene.waiting);
            scene.waiting = back;
            scene.window_from = now;
            scene.window_until = Some(now);
            for (ahead, act) in scene.waiting.iter().enumerate() {
                out.push(Out::State { act: act.id, player: act.player.clone(), state: ActState::Queued { ahead } });
            }
        }
        out
    }

    /// A player is no longer in a place — walked on, left the table, their hero fell. What
    /// they had waiting there did not happen. What is with the master stays: the caller
    /// drops that player from the answer when it comes.
    pub fn gone_from(&mut self, place: &str, player: &str, why: &str) -> Vec<Out> {
        let Some(scene) = self.scenes.get_mut(place) else { return Vec::new() };
        let mut out = Vec::new();
        // One who left is not waited for: with nobody else to wait for, what waits goes.
        let was = scene.expecting.len();
        scene.expecting.retain(|p| p != player);
        if was > 0 && scene.expecting.is_empty() && !scene.waiting.is_empty() {
            scene.window_until = Some(0);
        }
        scene.waiting.retain(|a| {
            let theirs = a.player == player;
            if theirs {
                out.push(Out::State { act: a.id, player: a.player.clone(), state: ActState::Void(why.to_string()) });
            }
            !theirs
        });
        if scene.waiting.is_empty() {
            scene.window_until = None;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asks(out: &[Out]) -> Vec<(u64, &str, Vec<&str>)> {
        out.iter().filter_map(|o| match o { Out::Ask { id, place, acts } => Some((id.0, place.as_str(), acts.iter().map(|a| a.said.as_str()).collect())), _ => None }).collect()
    }

    fn states(out: &[Out], player: &str) -> Vec<ActState> {
        out.iter().filter_map(|o| match o { Out::State { player: p, state, .. } if p == player => Some(state.clone()), _ => None }).collect()
    }

    fn say(q: &mut Queues, place: &str, player: &str, said: &str, now: Millis) -> (ActId, Vec<Out>) {
        q.submit(place, player, said, None, false, false, now)
    }

    #[test]
    fn a_second_act_is_never_refused() {
        let mut q = Queues::default();
        let (_, first) = say(&mut q, "market", "Ann", "light the curtain", 0);
        assert!(asks(&first).is_empty(), "the window is open");
        let sent = q.tick(1_500);
        assert_eq!(asks(&sent).len(), 1);
        // The master is busy with the market, and Bob acts there: taken, and told he waits.
        let (_, second) = say(&mut q, "market", "Bob", "cut the purse", 2_000);
        assert_eq!(states(&second, "Bob"), [ActState::Queued { ahead: 0 }]);
        assert!(asks(&second).is_empty());
        assert!(!second.iter().any(|o| matches!(o, Out::State { state: ActState::Refused(_), .. })));
    }

    #[test]
    fn acts_within_the_window_go_as_one_request() {
        let mut q = Queues::default();
        say(&mut q, "market", "Ann", "light the curtain", 0);
        say(&mut q, "market", "Bob", "cut the purse", 900);
        assert!(asks(&q.tick(1_499)).is_empty());
        let sent = q.tick(1_500);
        assert_eq!(asks(&sent), [(3, "market", vec!["light the curtain", "cut the purse"])]);
        assert_eq!((states(&sent, "Ann"), states(&sent, "Bob")), (vec![ActState::Asked], vec![ActState::Asked]));
    }

    #[test]
    fn one_player_alone_waits_for_no_window() {
        let mut q = Queues::default();
        let (_, out) = q.submit("camp", "Ann", "look around", None, false, true, 10);
        assert_eq!(asks(&out).len(), 1);
    }

    #[test]
    fn typing_stretches_the_window_up_to_the_cap() {
        let mut q = Queues::default();
        say(&mut q, "market", "Ann", "wait for it", 0);
        for now in [1_000, 3_000, 5_000] {
            q.typing("market", now);
            assert!(asks(&q.tick(now + 500)).is_empty(), "held at {now}");
        }
        // Still typing, but the first act has waited as long as anything may.
        q.typing("market", 5_900);
        assert_eq!(asks(&q.tick(6_000)).len(), 1);
    }

    #[test]
    fn typing_that_stops_lets_the_window_close() {
        let mut q = Queues::default();
        say(&mut q, "market", "Ann", "go", 0);
        q.typing("market", 1_000);
        assert!(asks(&q.tick(3_999)).is_empty());
        assert_eq!(asks(&q.tick(4_000)).len(), 1);
    }

    #[test]
    fn acts_during_a_request_form_the_next_batch_and_go_at_once() {
        let mut q = Queues::default();
        say(&mut q, "market", "Ann", "one", 0);
        let first = asks(&q.tick(1_500))[0].0;
        say(&mut q, "market", "Bob", "two", 2_000);
        say(&mut q, "market", "Cid", "three", 9_000);
        assert!(asks(&q.tick(20_000)).is_empty(), "one question to a place at a time");
        let flight = q.answered(RequestId(first), 30_000).unwrap();
        assert_eq!(flight.acts.len(), 1);
        assert_eq!(asks(&q.tick(30_000))[0].2, ["two", "three"]);
    }

    #[test]
    fn two_places_are_asked_at_once() {
        let mut q = Queues::default();
        q.submit("market", "Ann", "a", None, false, true, 0);
        let (_, out) = q.submit("harbour", "Bob", "b", None, false, true, 5);
        assert_eq!(asks(&out).len(), 1);
        assert_eq!(q.flights().count(), 2);
    }

    #[test]
    fn the_parallel_limit_holds_and_the_oldest_goes_first() {
        let mut q = Queues::new(Rules { parallel: 2, ..Rules::default() });
        let (_, a) = q.submit("a", "Ann", "a", None, false, true, 0);
        q.submit("b", "Bob", "b", None, false, true, 1);
        let (_, c) = q.submit("c", "Cid", "c", None, false, true, 2);
        let (_, d) = q.submit("d", "Dee", "d", None, false, true, 3);
        assert!(asks(&c).is_empty() && asks(&d).is_empty());
        // One comes back: the place that has waited longest is next, not the last to ask.
        q.answered(RequestId(asks(&a)[0].0), 100);
        assert_eq!(asks(&q.tick(100))[0].1, "c");
        assert_eq!(q.flights().count(), 2);
    }

    #[test]
    fn an_act_is_withdrawn_before_it_is_asked_and_not_after() {
        let mut q = Queues::default();
        let (act, _) = say(&mut q, "market", "Ann", "hm", 0);
        assert!(q.withdraw(act, "Bob").is_empty(), "not Bob's to take back");
        assert_eq!(states(&q.withdraw(act, "Ann"), "Ann"), [ActState::Withdrawn]);
        assert!(asks(&q.tick(5_000)).is_empty() && q.idle());
        let (act, _) = say(&mut q, "market", "Ann", "again", 6_000);
        q.tick(7_500);
        assert!(q.withdraw(act, "Ann").is_empty(), "it is with the master");
    }

    #[test]
    fn words_added_join_what_the_player_has_waiting() {
        let mut q = Queues::default();
        let (first, _) = say(&mut q, "market", "Ann", "I light the curtain", 0);
        let (again, out) = say(&mut q, "market", "Ann", "and shout fire", 800);
        assert_eq!(first, again);
        assert_eq!(states(&out, "Ann"), [ActState::Queued { ahead: 0 }]);
        assert_eq!(asks(&q.tick(1_500))[0].2, ["I light the curtain and shout fire"]);
    }

    #[test]
    fn what_follows_a_choice_is_always_asked() {
        // The master is busy with the place; a choice is taken there. Its outcome is told
        // at once elsewhere; here, what follows it waits and is asked.
        let mut q = Queues::default();
        say(&mut q, "market", "Ann", "talk", 0);
        let first = asks(&q.tick(1_500))[0].0;
        let (_, out) = q.submit("market", "Bob", "Run for the gate", None, true, false, 2_000);
        assert_eq!(states(&out, "Bob"), [ActState::Queued { ahead: 0 }]);
        q.answered(RequestId(first), 5_000);
        let next = q.tick(5_000);
        assert_eq!(asks(&next)[0].2, ["Run for the gate"]);
        let Out::Ask { acts, .. } = next.iter().find(|o| matches!(o, Out::Ask { .. })).unwrap() else { unreachable!() };
        assert!(acts[0].follows);
    }

    #[test]
    fn a_failed_request_fails_only_its_own_acts() {
        let mut q = Queues::default();
        let (_, a) = q.submit("market", "Ann", "a", None, false, true, 0);
        q.submit("harbour", "Bob", "b", None, false, true, 0);
        let out = q.failed(RequestId(asks(&a)[0].0), "the quota is spent", 10);
        assert_eq!(states(&out, "Ann"), [ActState::Refused("the quota is spent".into())]);
        assert!(states(&out, "Bob").is_empty());
        assert_eq!(q.flights().count(), 1);
    }

    #[test]
    fn two_same_worded_acts_get_their_own_answers() {
        let mut q = Queues::default();
        let (_, a) = q.submit("market", "Ann", "look around", None, false, true, 0);
        let (_, b) = q.submit("harbour", "Bob", "look around", None, false, true, 0);
        let (a, b) = (RequestId(asks(&a)[0].0), RequestId(asks(&b)[0].0));
        assert_ne!(a, b);
        assert_eq!(q.answered(b, 5).unwrap().acts[0].player, "Bob");
        assert_eq!(q.answered(a, 6).unwrap().acts[0].player, "Ann");
        // An answer nobody waits for is known as such.
        assert!(q.answered(a, 7).is_none());
    }

    #[test]
    fn a_place_that_changed_sends_its_acts_back_once() {
        let mut q = Queues::default();
        let (_, a) = q.submit("market", "Ann", "pick the lock", None, false, true, 0);
        let flight = q.answered(RequestId(asks(&a)[0].0), 100).unwrap();
        let back = q.give_back(flight, "a fight broke out", 100);
        assert_eq!(states(&back, "Ann"), [ActState::Queued { ahead: 0 }]);
        let again = q.tick(100);
        let flight = q.answered(RequestId(asks(&again)[0].0), 200).unwrap();
        // The second time it is not sent round again: the player is told.
        let out = q.give_back(flight, "a fight broke out", 200);
        assert_eq!(states(&out, "Ann"), [ActState::Refused("a fight broke out".into())]);
        assert!(q.idle());
    }

    #[test]
    fn a_player_who_walks_on_leaves_nothing_waiting_behind() {
        let mut q = Queues::default();
        say(&mut q, "market", "Ann", "one", 0);
        say(&mut q, "market", "Bob", "two", 10);
        let out = q.gone_from("market", "Bob", "left the place");
        assert_eq!(states(&out, "Bob"), [ActState::Void("left the place".into())]);
        assert_eq!(asks(&q.tick(1_500))[0].2, ["one"]);
    }

    #[test]
    fn without_batching_each_act_is_its_own_question_in_turn() {
        let mut q = Queues::new(Rules { batch: false, ..Rules::default() });
        let (_, a) = say(&mut q, "market", "Ann", "one", 0);
        say(&mut q, "market", "Bob", "two", 0);
        assert_eq!(asks(&a)[0].2, ["one"]);
        q.answered(RequestId(asks(&a)[0].0), 50);
        assert_eq!(asks(&q.tick(50))[0].2, ["two"]);
    }

    #[test]
    fn what_a_player_has_pending_can_be_asked_of_the_queues() {
        let mut q = Queues::default();
        assert_eq!(q.pending_of("Ann"), None);
        let (act, _) = say(&mut q, "market", "Ann", "one", 0);
        assert_eq!(q.pending_of("Ann"), Some((act, ActState::Queued { ahead: 0 })));
        q.tick(1_500);
        assert_eq!(q.pending_of("Ann"), Some((act, ActState::Asked)));
        assert!(q.asking("market") && !q.asking("harbour"));
    }

    #[test]
    fn those_offered_something_are_waited_for_however_long_and_who_let_it_pass_is_told() {
        let mut q = Queues::default();
        q.expect("camp", &["Ann".to_string(), "Bob".to_string()]);
        // Ann chooses at once: nothing goes while Bob has not answered, not even a minute on.
        let (_, out) = say(&mut q, "camp", "Ann", "I light the torch", 0);
        assert!(asks(&out).is_empty());
        assert!(asks(&q.tick(60_000)).is_empty(), "no window closes on one who has not answered");
        assert_eq!(q.expecting("camp"), ["Bob"]);
        // Bob closes it: Ann's deed goes, and the master is told Bob let it pass.
        let out = q.pass("camp", "Bob", 61_000);
        let asked: Vec<_> = out.iter().filter_map(|o| match o { Out::Ask { acts, .. } => Some(acts.clone()), _ => None }).collect();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].iter().map(|a| (a.player.as_str(), a.passed)).collect::<Vec<_>>(), [("Ann", false), ("Bob", true)]);
    }

    #[test]
    fn when_everyone_lets_it_pass_nothing_is_asked_and_one_who_leaves_is_not_waited_for() {
        let mut q = Queues::default();
        q.expect("camp", &["Ann".to_string(), "Bob".to_string()]);
        assert!(asks(&q.pass("camp", "Ann", 0)).is_empty());
        assert!(asks(&q.pass("camp", "Bob", 1)).is_empty());
        assert!(q.idle(), "nothing to ask");
        // Ann acts; Bob, who was expected, walks off: Ann's deed goes.
        q.expect("camp", &["Ann".to_string(), "Bob".to_string()]);
        say(&mut q, "camp", "Ann", "I sit down", 10);
        q.gone_from("camp", "Bob", "left");
        assert_eq!(asks(&q.tick(20)).len(), 1);
    }
}
