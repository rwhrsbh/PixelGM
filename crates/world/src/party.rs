//! The party and what the rules do to it: checks whose two outcomes the game master wrote
//! in advance, damage and healing outside a fight, experience, and fights themselves.

use crate::model::{Attitude, Creature, Player, World};
use crate::path::nearest;
use crate::story::{Effect, Speaker};
use rules::character::LevelUp;
use rules::combat::{Cell, Strike};
use rules::dice::Edge;
use rules::{Ability, Combatant, Dice, Encounter, Event, Outcome, Rng, Roll, Side, srd};
use serde::{Deserialize, Serialize};

/// What happens on one side of a check.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct Branch {
    pub narration: String,
    pub effects: Vec<Effect>,
}

/// A check the party has yet to roll. Everything about it was decided when the master
/// wrote it; only the die is missing.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct PendingCheck {
    pub ability: Ability,
    pub skill: Option<String>,
    pub difficulty: i32,
    pub success: Branch,
    pub failure: Branch,
    /// The thing whose action led here, for the id `self` in the branches.
    pub me: Option<String>,
    /// Effects that were listed after the check and wait for it.
    pub rest: Vec<Effect>,
}

/// A check after the die was rolled.
#[derive(Clone, PartialEq, Debug)]
pub struct Checked {
    /// Who rolled: the member of the party best at it.
    pub hero: String,
    pub roll: Roll,
    pub difficulty: i32,
    pub success: bool,
    pub narration: String,
}

/// A creature's fighting numbers, written by the game master for creatures that are not
/// in the SRD.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct Stats {
    pub ac: i32,
    pub hp: i32,
    /// Feet per turn.
    pub speed: u32,
    pub attacks: Vec<StatAttack>,
    pub xp: u32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct StatAttack {
    pub name: String,
    pub bonus: i32,
    /// Dice, as in `1d6+2`.
    pub damage: String,
    /// Feet.
    pub range: u32,
}

/// How a fight ended, for the interface to tell.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct FightSummary {
    pub won: bool,
    /// Experience each surviving hero received.
    pub xp_each: u32,
    pub level_ups: Vec<(String, LevelUp)>,
}

/// What the master made of something a hero tried in a fight that the rules do not cover:
/// a check, and what a success is worth.
#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct StuntDraft {
    /// What the attempt looks like, told whichever way it goes.
    pub narration: String,
    pub ability: String,
    pub skill: Option<String>,
    #[serde(deserialize_with = "crate::story::lenient_int")]
    pub dc: i64,
    /// The id of the creature it is aimed at, or the name of a hero; empty for nobody.
    pub target: String,
    /// Dice of harm on a success; empty for none.
    #[serde(deserialize_with = "crate::story::lenient_text")]
    pub damage: String,
    /// Dice of healing on a success; empty for none.
    #[serde(deserialize_with = "crate::story::lenient_text")]
    pub heal: String,
    pub success: String,
    pub failure: String,
}

/// An improvised action after the die was rolled.
#[derive(Clone, PartialEq, Debug)]
pub struct Stunt {
    pub hero: String,
    pub roll: Roll,
    pub difficulty: i32,
    pub success: bool,
    /// What the players read: the attempt and how it went.
    pub narration: String,
    pub events: Vec<Event>,
}

/// One unusual deed in this many earns a secret skill.
const DEEDS_PER_SECRET: u32 = 5;

/// Story experience the master may hand out in one go, per character level: enough to
/// matter, not enough to skip levels.
const STORY_XP_PER_LEVEL: u32 = 100;

fn hero_id(index: usize) -> String {
    format!("hero_{index}")
}

impl World {
    /// Whether the hero's player is not at the table. Heroes nobody owns are never away.
    pub fn is_away(&self, hero: usize) -> bool {
        !self.present.is_empty() && self.owners.get(hero).is_some_and(|o| !o.is_empty() && !self.present.contains(o))
    }

    /// Whether the hero takes part in what happens here: alive, its player at the table,
    /// and in the place being looked at.
    pub fn takes_part(&self, hero: usize) -> bool {
        self.party.get(hero).is_some_and(|h| !h.dead) && !self.is_away(hero) && self.hero_place(hero) == self.current
    }

    /// Runs something in the place the fight is in, wherever the host happens to be.
    fn in_fight_place<R>(&mut self, act: impl FnOnce(&mut Self) -> R) -> R {
        if self.fight_place.is_empty() || self.fight_place == self.current || !self.locations.contains_key(&self.fight_place) {
            return act(self);
        }
        // Already looking from elsewhere (a guest's action): the host's place is known.
        let nested = self.host_place.is_some();
        let home = std::mem::replace(&mut self.current, self.fight_place.clone());
        if !nested {
            self.host_place = Some(home.clone());
        }
        let result = act(self);
        self.current = home;
        if !nested {
            self.host_place = None;
        }
        result
    }

    /// Gives an away hero to a player who has none. `false` when the hero is somebody's who
    /// is here, or the player already has one.
    pub fn claim(&mut self, hero: usize, player: &str) -> bool {
        if hero >= self.party.len() || player.is_empty() || self.owners.iter().any(|o| o == player) {
            return false;
        }
        self.owners.resize(self.party.len(), String::new());
        if self.owners[hero].is_empty() || !self.is_away(hero) {
            return false;
        }
        self.owners[hero] = player.to_string();
        true
    }

    /// Whether the caps that keep a generous master in check apply at this table.
    pub fn limited(&self) -> bool {
        !self.table.no_limits
    }

    /// The most harm or healing one effect outside a fight may do.
    fn story_cap(&self) -> i32 {
        if self.limited() { 10 + 10 * self.party_level() as i32 } else { 500 }
    }

    /// A hero by name, or the first who takes part when the name fits nobody.
    fn hero_named(&self, name: &str) -> Option<usize> {
        let wanted = name.trim().to_lowercase();
        self.party.iter().position(|h| !wanted.is_empty() && h.name.to_lowercase() == wanted).or_else(|| (0..self.party.len()).find(|i| self.takes_part(*i)))
    }

    /// Counts an unusual deed. Now and then one earns a secret skill, which the master is
    /// then asked to write; a hero holds only so many of them for its level.
    pub(crate) fn deed(&mut self, hero: &str, rng: &mut Rng) {
        if self.table.book_only {
            return;
        }
        let Some(index) = self.hero_named(hero) else { return };
        let held = self.party[index].feats.iter().filter(|f| f.secret).count() as u32;
        let room = held < 1 + self.party[index].level / 4;
        if room && !self.secrets_due.contains(&index) && rng.die(DEEDS_PER_SECRET) == 1 {
            self.secrets_due.push(index);
        }
    }

    /// The story brings a dead hero back, on one hit point.
    pub(crate) fn revive(&mut self, hero: &str) -> bool {
        let wanted = hero.trim().to_lowercase();
        // A creature's body, by its own id or the body's: it gets up where it lay.
        let here = self.current.clone();
        if let Some(place) = self.locations.get_mut(&here) {
            let id = crate::build::slug(&wanted);
            if let Some(at) = place.props.iter().position(|p| p.body.is_some() && (p.id == id || p.id == format!("{id}_body"))) {
                let prop = place.props.remove(at);
                let mut creature = *prop.body.expect("found by its body");
                (creature.x, creature.y) = (prop.x, prop.y);
                place.creatures.push(creature);
                return true;
            }
        }
        let found = self.party.iter_mut().find(|h| h.dead && (wanted.is_empty() || h.name.to_lowercase() == wanted));
        match found {
            Some(hero) => {
                hero.dead = false;
                hero.death = Default::default();
                hero.hp = 1;
                true
            }
            None => false,
        }
    }

    /// The hero the master should be asked about next: one owed a secret skill, or one at
    /// a level that calls for abilities to choose from. `true` marks the secret.
    pub fn feat_wanted(&self) -> Option<(usize, bool)> {
        if self.table.book_only {
            return None;
        }
        if let Some(hero) = self.secrets_due.first() {
            return Some((*hero, true));
        }
        (0..self.party.len()).find(|i| self.party[*i].feat_due && !self.party[*i].dead && !self.is_away(*i) && !self.offers.iter().any(|o| o.hero == *i)).map(|i| (i, false))
    }

    /// Takes what the master wrote for a hero. A secret skill is the hero's at once, and
    /// its name comes back to be told; abilities for a level wait for the player's choice.
    pub fn take_feats(&mut self, hero: usize, secret: bool, mut feats: Vec<rules::Feat>) -> Option<String> {
        let limits = self.limited();
        let level = self.party.get(hero)?.level;
        for feat in &mut feats {
            feat.fit(level, limits);
            feat.secret = secret;
        }
        feats.retain(|f| !f.name.is_empty());
        if secret {
            self.secrets_due.retain(|h| *h != hero);
            let feat = feats.into_iter().next()?;
            let name = feat.name.clone();
            self.party[hero].grant(feat, limits);
            return Some(name);
        }
        feats.truncate(3);
        if feats.is_empty() {
            // Nothing usable came: the level passes without one rather than asking forever.
            self.party[hero].feat_due = false;
        } else {
            self.offers.retain(|o| o.hero != hero);
            self.offers.push(crate::model::FeatOffer { hero, feats });
        }
        None
    }

    /// The player picked one of the abilities offered to their hero.
    pub fn choose_feat(&mut self, hero: usize, choice: usize) -> bool {
        let Some(at) = self.offers.iter().position(|o| o.hero == hero) else { return false };
        if choice >= self.offers[at].feats.len() || hero >= self.party.len() {
            return false;
        }
        let offer = self.offers.remove(at);
        let limits = self.limited();
        self.party[hero].grant(offer.feats[choice].clone(), limits);
        self.party[hero].feat_due = false;
        true
    }

    /// A table that plays by the book has no abilities to offer: the levels pass.
    pub fn settle_feats(&mut self) {
        if self.table.book_only {
            for hero in &mut self.party {
                hero.feat_due = false;
            }
            self.secrets_due.clear();
            self.offers.clear();
        }
    }

    /// An hour's rest for everyone who takes part. Gives the hit points regained in all.
    pub fn short_rest(&mut self, rng: &mut Rng) -> i32 {
        let resting: Vec<bool> = (0..self.party.len()).map(|i| self.takes_part(i)).collect();
        self.party.iter_mut().zip(resting).filter(|(_, rests)| *rests).map(|(hero, _)| hero.short_rest(rng)).sum()
    }

    /// Something the acting hero tried that the rules do not cover, as the master judged
    /// it: the hero rolls the check, and a success does what the master allowed. Either
    /// way the action is spent. `None` when it is not a hero's turn to act.
    pub fn fight_stunt(&mut self, draft: &StuntDraft, name: &str, rng: &mut Rng) -> Option<Stunt> {
        self.in_fight_place(|world| world.fight_stunt_here(draft, name, rng))
    }

    fn fight_stunt_here(&mut self, draft: &StuntDraft, name: &str, rng: &mut Rng) -> Option<Stunt> {
        let mut fight = self.fight.clone()?;
        let actor = fight.actor();
        if actor.side != Side::Party || !actor.action_left || !actor.standing() {
            return None;
        }
        let hero = actor.id.strip_prefix("hero_").and_then(|n| n.parse::<usize>().ok()).and_then(|i| self.party.get(i))?;
        let ability = Ability::from_index(draft.ability.trim().to_lowercase().as_str()).unwrap_or(Ability::Dex);
        let skill = draft.skill.as_ref().map(|s| s.trim().to_lowercase().replace([' ', '_'], "-")).filter(|s| !s.is_empty());
        let difficulty = draft.dc.clamp(5, 30) as i32;
        let roll = hero.check(rng, ability, skill.as_deref(), Edge::Plain);
        let success = roll.total() >= difficulty;
        let cap = if self.limited() { 6 + 6 * hero.level as i32 } else { 500 };
        let rolled = |dice: &str, rng: &mut Rng| Dice::parse(dice, 0).map_or(0, |d| d.roll(rng)).clamp(0, cap);
        let wanted = draft.target.trim().to_lowercase();
        let target = fight.combatants.iter().position(|c| !wanted.is_empty() && (c.id == crate::build::slug(&wanted) || c.name.to_lowercase() == wanted));
        let (damage, heal) = if success { (rolled(&draft.damage, rng), rolled(&draft.heal, rng)) } else { (0, 0) };
        let hero_name = hero.name.clone();
        let events = fight.improvise(name, target, damage, heal);
        self.fight = Some(fight);
        let outcome = if success { draft.success.trim() } else { draft.failure.trim() };
        let narration = [draft.narration.trim(), outcome].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" ");
        self.story.say(Speaker::Master, &narration);
        Some(Stunt { hero: hero_name, roll, difficulty, success, narration, events })
    }

    /// The average level of the party; 1 when there is no party yet.
    pub fn party_level(&self) -> u32 {
        match self.party.len() as u32 {
            0 => 1,
            n => (self.party.iter().map(|h| h.level).sum::<u32>() / n).max(1),
        }
    }

    /// Fighting numbers for a creature: its SRD monster, the numbers the master gave it, or
    /// a plain opponent sized to the party when it has neither.
    pub fn combatant_for(&self, creature: &Creature) -> Combatant {
        let at = (creature.x as i32, creature.y as i32);
        if let Some(monster) = creature.monster.as_deref().and_then(|m| srd().monsters.get(m)) {
            return Combatant::monster(monster, &creature.id, &creature.name, at);
        }
        let level = self.party_level() as i32;
        let fallback = Stats { ac: 12, hp: 6 + 5 * level, speed: 30, attacks: vec![StatAttack { name: "Attack".into(), bonus: 3 + level / 2, damage: format!("1d6+{}", 1 + level / 2), range: 5 }], xp: 50 * level as u32 };
        // A table that plays by the book takes no numbers the master made up.
        let stats = creature.stats.as_ref().filter(|s| s.hp > 0 && !self.table.book_only).unwrap_or(&fallback);
        let (most_hp, most_bonus, most_damage) = if self.limited() { (30 + 40 * level, 4 + level, 6.0 + 6.0 * level as f32) } else { (999, 20, 500.0) };
        let within = |text: &str| {
            let mut dice = Dice::parse(text, 0)?;
            let each = (dice.sides as f32 + 1.0) / 2.0;
            if dice.sides > 0 && dice.average() > most_damage {
                dice.bonus = dice.bonus.min((most_damage / 3.0) as i32);
                dice.count = (((most_damage - dice.bonus as f32) / each).floor() as u32).max(1);
            }
            Some(dice)
        };
        let monster = rules::data::Monster {
            index: creature.id.clone(),
            name: creature.name.clone(),
            ac: stats.ac.clamp(5, 30),
            hp: stats.hp.clamp(1, most_hp),
            speed: stats.speed.clamp(5, 120),
            abilities: [10; 6],
            attacks: stats
                .attacks
                .iter()
                .filter_map(|a| Some(rules::data::MonsterAttack { name: a.name.clone(), bonus: a.bonus.clamp(-5, most_bonus), damage: vec![within(&a.damage)?], range: a.range.max(5), save: None }))
                .collect(),
            multiattack: Vec::new(),
            challenge: 0.0,
            xp: stats.xp.min(50_000),
            size: 1,
        };
        Combatant::monster(&monster, &creature.id, &creature.name, at)
    }

    /// Rolls the check that is waiting. The party member with the best bonus makes the
    /// attempt; the branch that fits the result is told and takes effect.
    pub fn roll_check(&mut self, rng: &mut Rng) -> Option<Checked> {
        let check = self.pending.take()?;
        let skill = check.skill.as_deref();
        let best = self.party.iter().enumerate().filter(|(i, h)| !h.is_down() && self.takes_part(*i)).map(|(_, h)| h).max_by_key(|h| h.check_bonus(check.ability, skill));
        let (hero, roll) = match best {
            Some(hero) => (hero.name.clone(), hero.check(rng, check.ability, skill, Edge::Plain)),
            // No party yet: a plain d20.
            None => (String::new(), Roll::d20(rng, 0, Edge::Plain)),
        };
        let success = roll.total() >= check.difficulty;
        let branch = if success { check.success } else { check.failure };
        self.story.say(Speaker::Master, &branch.narration);
        let mut effects = branch.effects;
        effects.extend(check.rest);
        self.apply(&effects, check.me.as_deref());
        Some(Checked { hero, roll, difficulty: check.difficulty, success, narration: branch.narration })
    }

    /// Damage outside a fight falls on the first hero still standing.
    pub(crate) fn hurt_party(&mut self, dice: &str, rng: &mut Rng) -> i32 {
        let amount = Dice::parse(dice, 0).map_or(0, |d| d.roll(rng)).clamp(0, self.story_cap());
        let target = (0..self.party.len()).find(|i| self.takes_part(*i) && !self.party[*i].is_down());
        if let Some(hero) = target.and_then(|i| self.party.get_mut(i)) {
            hero.take_damage(amount);
        }
        amount
    }

    pub(crate) fn heal_party(&mut self, dice: &str, rng: &mut Rng) -> i32 {
        let amount = Dice::parse(dice, 0).map_or(0, |d| d.roll(rng)).clamp(0, self.story_cap());
        // The one who needs it most: the lowest share of hit points left.
        let target = (0..self.party.len()).filter(|i| self.takes_part(*i)).min_by_key(|i| self.party[*i].hp * 1000 / self.party[*i].max_hp.max(1));
        if let Some(hero) = target.and_then(|i| self.party.get_mut(i)) {
            hero.heal(amount);
        }
        amount
    }

    /// Experience from the story, capped so a generous master cannot skip levels.
    pub(crate) fn award_story_xp(&mut self, amount: i64) -> Vec<(String, LevelUp)> {
        let most = if self.limited() { (STORY_XP_PER_LEVEL * self.party_level()) as i64 } else { 100_000 };
        let capped = amount.clamp(0, most) as u32;
        self.award_xp(capped)
    }

    fn award_xp(&mut self, each: u32) -> Vec<(String, LevelUp)> {
        let mut ups = Vec::new();
        let taking_part: Vec<bool> = (0..self.party.len()).map(|i| self.takes_part(i)).collect();
        for hero in self.party.iter_mut().zip(taking_part).filter(|(_, part)| *part).map(|(h, _)| h) {
            for up in hero.gain_xp(each) {
                ups.push((hero.name.clone(), up));
            }
        }
        ups
    }

    /// The hostile creatures of the place the party is in.
    pub fn hostiles(&self) -> impl Iterator<Item = &Creature> {
        self.here().creatures.iter().filter(|c| c.attitude == Attitude::Hostile)
    }

    /// A hostile creature that has noticed the party: one within `cells` of it.
    pub fn hostile_near(&self, cells: u32) -> bool {
        let (px, py) = self.player.cell();
        !self.party.is_empty() && self.hostiles().any(|c| rules::combat::distance((c.x as i32, c.y as i32), (px, py)) <= cells)
    }

    /// Whether a cell cannot be entered in a fight because of the ground or a thing on it.
    /// Creatures taking part are the encounter's business, not the map's.
    pub fn fight_blocked(&self, (x, y): Cell) -> bool {
        let (w, h) = self.here().size();
        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
            return true;
        }
        let here = self.here();
        let (ux, uy) = (x as usize, y as usize);
        let solid = here.tile_id(ux, uy).and_then(|id| self.tiles.get(id)).is_none_or(|t| !t.walkable);
        let fighting = |id: &str| self.fight.as_ref().is_some_and(|f| f.index_of(id).is_some());
        solid || here.props.iter().any(|p| p.blocking && (p.x, p.y) == (ux, uy)) || here.creatures.iter().any(|c| (c.x, c.y) == (ux, uy) && !fighting(&c.id))
    }

    /// The place a companion is in: where the hero of its player is.
    pub fn companion_place(&self, companion: &crate::model::Companion) -> &str {
        if companion.owner.is_empty() { self.host_place() } else { self.place_of(&companion.owner) }
    }

    /// The companions in the place being looked at, by their number.
    pub fn companions_here(&self) -> Vec<usize> {
        (0..self.companions.len()).filter(|n| self.companion_place(&self.companions[*n]) == self.current).collect()
    }

    /// How many may still join: two beside a lone hero, five fighters in all at a shared table.
    pub fn room_for_companions(&self) -> usize {
        let heroes = self.party.iter().filter(|h| !h.dead).count();
        let most = if heroes <= 1 { 2 } else { 5usize.saturating_sub(heroes) };
        most.saturating_sub(self.companions.len())
    }

    /// Someone here joins the party, under the hand of `owner`.
    pub fn join(&mut self, id: &str, owner: &str) -> Result<(), String> {
        if self.room_for_companions() == 0 {
            return Err("the party has no room for another companion".to_string());
        }
        let here = self.current.clone();
        let at = self.here().creatures.iter().position(|c| c.id == id).ok_or(format!("nobody called '{id}' is here"))?;
        let mut creature = self.locations.get_mut(&here).expect("the party is somewhere").creatures.remove(at);
        creature.attitude = Attitude::Friendly;
        creature.actions.clear();
        creature.stock.clear();
        creature.wares.clear();
        let max_hp = self.combatant_for(&creature).max_hp.max(1);
        // The id must stay its own among the companions.
        if self.companions.iter().any(|c| c.creature.id == creature.id) {
            creature.id = format!("{}_{}", creature.id, self.companions.len() + 1);
        }
        self.companions.push(crate::model::Companion { creature, hp: max_hp, max_hp, owner: owner.to_string() });
        Ok(())
    }

    /// A companion leaves the party and stays in the place it is in.
    pub fn dismiss(&mut self, id: &str) -> bool {
        let wanted = id.to_lowercase();
        let Some(n) = self.companions.iter().position(|c| c.creature.id == id || c.creature.name.to_lowercase() == wanted) else { return false };
        let place = self.companion_place(&self.companions[n]).to_string();
        let mut gone = self.companions.remove(n).creature;
        let (px, py) = self.player.cell();
        if let Some(location) = self.locations.get(&place) {
            let free = |(x, y): (usize, usize)| !location.blocked(x as i32, y as i32, &self.tiles) && location.exit_at(x, y).is_none() && (x as i32, y as i32) != (px, py);
            let (w, h) = location.size();
            let near = (px.clamp(0, w as i32 - 1) as usize, py.clamp(0, h as i32 - 1) as usize);
            (gone.x, gone.y) = nearest(near, free).unwrap_or(near);
        }
        gone.mustered = true;
        if let Some(location) = self.locations.get_mut(&place) {
            location.creatures.push(gone);
        }
        true
    }

    /// What a fight here may cost: the experience the book calls a hard encounter for the
    /// heroes taking part, more or less by the difficulty of the world.
    pub fn encounter_budget(&self) -> u32 {
        const HARD: [u32; 20] = [75, 150, 225, 375, 750, 900, 1100, 1400, 1600, 1900, 2400, 3000, 3400, 3800, 4300, 4800, 5900, 6300, 7300, 8500];
        let sum: u32 = self.party.iter().enumerate().filter(|(i, h)| self.takes_part(*i) && !h.is_down()).map(|(_, h)| HARD[(h.level.clamp(1, 20) - 1) as usize]).sum();
        let factor = match self.table.difficulty {
            crate::model::Difficulty::Casual => 3,
            crate::model::Difficulty::Honest => 4,
            crate::model::Difficulty::Hardcore => 5,
        };
        sum * factor / 4
    }

    /// How many of a monster worth `xp` each make a fair pack for this party: the book
    /// counts two as half as much again and three or more as double.
    pub fn pack_size(&self, xp: u32) -> u8 {
        let budget = self.encounter_budget();
        let cost = |n: u32| xp.max(1) * n * if n >= 3 { 4 } else if n == 2 { 3 } else { 2 } / 2;
        (1..=6u32).rev().find(|n| cost(*n) <= budget).unwrap_or(1) as u8
    }

    /// Counts out the packs of hostile creatures here: each one the master set down stands
    /// for as many as the master said, or as many as make a fair fight. The others take the
    /// nearest free cells. `true` when anyone was added.
    pub fn muster(&mut self) -> bool {
        if !(0..self.party.len()).any(|i| self.takes_part(i)) {
            return false;
        }
        // A hostile one stands for its pack; anyone the master counted out comes in that number.
        let waiting: Vec<usize> = self.here().creatures.iter().enumerate().filter(|(_, c)| (c.attitude == Attitude::Hostile || c.count > 1) && !c.mustered).map(|(n, _)| n).collect();
        if waiting.is_empty() {
            return false;
        }
        let mut added = false;
        for n in waiting {
            let leader = self.here().creatures[n].clone();
            let wanted = match (leader.count, leader.monster.as_deref().and_then(|m| srd().monsters.get(m))) {
                (count @ 1.., _) => count,
                (0, Some(monster)) if leader.attitude == Attitude::Hostile => self.pack_size(monster.xp),
                (0, Some(_)) => 1,
                // Numbers of its own, and no count: the master meant one.
                (0, None) => 1,
            };
            let here = self.current.clone();
            let (px, py) = self.player.cell();
            for k in 2..=wanted {
                let location = &self.locations[&here];
                let free = |(x, y): (usize, usize)| !location.blocked(x as i32, y as i32, &self.tiles) && location.exit_at(x, y).is_none() && (x as i32, y as i32) != (px, py);
                let Some((x, y)) = nearest((leader.x, leader.y), free) else { break };
                let fellow = Creature { id: format!("{}_{k}", leader.id), x, y, actions: Vec::new(), count: 1, mustered: true, ..leader.clone() };
                self.locations.get_mut(&here).expect("the party is somewhere").creatures.push(fellow);
                added = true;
            }
            if let Some(c) = self.locations.get_mut(&here).and_then(|l| l.creatures.get_mut(n)) {
                c.mustered = true;
            }
        }
        added
    }

    /// Starts a fight with every hostile creature here. `false` when there is nobody to
    /// fight or nobody to do the fighting.
    pub fn start_fight(&mut self, rng: Rng) -> bool {
        if self.fight.is_some() || !(0..self.party.len()).any(|i| self.takes_part(i)) || self.hostiles().next().is_none() {
            return false;
        }
        // Whoever turned on the party a moment ago is joined by the rest of its pack.
        self.muster();
        let enemies: Vec<Combatant> = self.hostiles().map(|c| self.combatant_for(c)).collect();
        let (w, h) = self.here().size();
        let inside = |(x, y): (i32, i32)| (x.clamp(0, w as i32 - 1) as usize, y.clamp(0, h as i32 - 1) as usize);
        let leader = inside(self.player.cell());
        let mut taken: Vec<(usize, usize)> = Vec::new();
        let mut combatants = Vec::new();
        let here = self.here();
        for (i, hero) in self.party.iter().enumerate().filter(|(i, _)| self.takes_part(*i)) {
            // A hero whose player walks a token of their own starts where that token stands;
            // the rest stand with the leader. Each takes the nearest free cell.
            let start = self.owners.get(i).and_then(|owner| self.tokens_here().find(|t| t.name == *owner)).map_or(leader, |t| inside(Player { x: t.x, y: t.y }.cell()));
            let free = |c: (usize, usize)| !here.blocked(c.0 as i32, c.1 as i32, &self.tiles) && !taken.contains(&c);
            let cell = nearest(start, free).unwrap_or(start);
            taken.push(cell);
            combatants.push(Combatant::hero(hero, &hero_id(i), (cell.0 as i32, cell.1 as i32)));
        }
        // Those who travel with the heroes here fight beside them.
        for n in self.companions_here() {
            let companion = &self.companions[n];
            let free = |c: (usize, usize)| !here.blocked(c.0 as i32, c.1 as i32, &self.tiles) && !taken.contains(&c);
            let cell = nearest(leader, free).unwrap_or(leader);
            taken.push(cell);
            let mut ally = self.combatant_for(&companion.creature);
            ally.id = format!("ally_{n}");
            ally.side = Side::Party;
            ally.hp = companion.hp.min(ally.max_hp);
            (ally.x, ally.y) = (cell.0 as i32, cell.1 as i32);
            combatants.push(ally);
        }
        combatants.extend(enemies);
        self.fight = Some(Encounter::start(combatants, rng).0);
        self.fight_place = self.current.clone();
        true
    }

    /// Ends the fight: heroes keep the wounds they took, the dead leave the map, the
    /// survivors share the experience.
    pub fn end_fight(&mut self) -> Option<FightSummary> {
        // The host's token moves only when the host was in the fight's place.
        let host_there = self.fight_place.is_empty() || self.fight_place == self.host_place();
        let summary = self.in_fight_place(|world| world.end_fight_here(host_there));
        self.fight_place.clear();
        summary
    }

    fn end_fight_here(&mut self, host_there: bool) -> Option<FightSummary> {
        let fight = self.fight.take()?;
        let won = fight.outcome() == Outcome::Victory;
        // The master was not there for the dice: what happened goes into the story and the
        // facts, so the next telling knows who is dead.
        let place = self.locations.get(&self.current).map_or(self.current.clone(), |l| l.name.clone());
        let fallen: Vec<&str> = fight.combatants.iter().filter(|c| c.side == Side::Enemy && c.dead).map(|c| c.name.as_str()).collect();
        let standing: Vec<&str> = fight.combatants.iter().filter(|c| c.side == Side::Enemy && !c.dead).map(|c| c.name.as_str()).collect();
        let heroes: Vec<String> = fight.combatants.iter().filter(|c| c.side == Side::Party).map(|c| format!("{} {}{}", c.name, c.hp.max(0), if c.dead { " (dead)" } else if c.hp <= 0 { " (down)" } else { "" })).collect();
        let mut told = format!("[Fight in {place}: {}.", if won { "the party won" } else { "the party was beaten" });
        if !fallen.is_empty() {
            told.push_str(&format!(" Killed: {}.", fallen.join(", ")));
        }
        if !standing.is_empty() {
            told.push_str(&format!(" Still standing: {}.", standing.join(", ")));
        }
        told.push_str(&format!(" Heroes' hit points after: {}.]", heroes.join(", ")));
        self.story.say(crate::story::Speaker::Master, &told);
        for name in &fallen {
            self.story.remember(&format!("{name} was killed by the party in {place}."));
        }
        for (i, hero) in self.party.iter_mut().enumerate() {
            if let Some(c) = fight.combatants.iter().find(|c| c.id == hero_id(i)) {
                hero.hp = c.hp;
                hero.death = c.death;
                hero.dead = c.dead;
                for strike in &c.strikes {
                    if let (Some(feat), Some(left)) = (strike.feat.and_then(|f| hero.feats.get_mut(f)), strike.uses_left) {
                        feat.used = feat.uses.saturating_sub(left);
                    }
                }
            }
        }
        // Companions keep their wounds; the dead are gone for good.
        for n in (0..self.companions.len()).rev() {
            if let Some(c) = fight.combatants.iter().find(|c| c.id == format!("ally_{n}")) {
                if c.dead {
                    let name = self.companions.remove(n).creature.name;
                    self.story.remember(&format!("{name}, who travelled with the party, died in {place}."));
                } else {
                    self.companions[n].hp = c.hp.max(1);
                }
            }
        }
        let here = self.current.clone();
        if let Some(location) = self.locations.get_mut(&here) {
            for c in fight.combatants.iter().filter(|c| c.side == Side::Enemy) {
                if c.dead {
                    // The dead stay where they fell, as a body that can be searched — or raised.
                    if let Some(at) = location.creatures.iter().position(|creature| creature.id == c.id) {
                        let mut dead = location.creatures.remove(at);
                        (dead.x, dead.y) = (c.x.max(0) as usize, c.y.max(0) as usize);
                        let russian = self.language.to_lowercase().starts_with("ru") || self.language.contains("усск");
                        let name = if russian { format!("Тело: {}", dead.name) } else { format!("Body of {}", dead.name) };
                        location.props.push(crate::model::Prop {
                            id: format!("{}_body", dead.id),
                            name,
                            description: dead.description.clone(),
                            x: dead.x,
                            y: dead.y,
                            blocking: false,
                            sprite: dead.sprite.clone(),
                            actions: Vec::new(),
                            body: Some(Box::new(dead)),
                            chest: None,
                        });
                    }
                } else if let Some(creature) = location.creatures.iter_mut().find(|creature| creature.id == c.id) {
                    (creature.x, creature.y) = (c.x.max(0) as usize, c.y.max(0) as usize);
                }
            }
        }
        // The host's own token ends where its hero stood: the first hero whose player has
        // no token of their own, or failing that anyone still alive.
        let walks_alone = |c: &&Combatant| {
            let owner = c.id.strip_prefix("hero_").and_then(|i| i.parse::<usize>().ok()).and_then(|i| self.owners.get(i));
            owner.is_some_and(|o| self.tokens_here().any(|t| t.name == *o))
        };
        let alive = || fight.combatants.iter().filter(|c| c.side == Side::Party && !c.dead);
        if let Some(leader) = alive().find(|c| !walks_alone(c)).or_else(|| alive().next()).filter(|_| host_there) {
            self.player = Player::in_cell(leader.x.max(0) as usize, leader.y.max(0) as usize);
        }
        let survivors = (0..self.party.len()).filter(|i| self.takes_part(*i)).count().max(1) as u32;
        let xp_each = if won { fight.xp_won() / survivors } else { 0 };
        let level_ups = self.award_xp(xp_each);
        Some(FightSummary { won, xp_each, level_ups })
    }

    /// After a lost fight the game master decides what became of the party; whatever that
    /// is, the heroes who are not dead are back on their feet with one hit point.
    pub fn revive_fallen(&mut self) {
        for hero in self.party.iter_mut().filter(|h| !h.dead && h.hp <= 0) {
            hero.heal(1);
        }
    }

    /// A night's rest for everyone.
    pub fn long_rest(&mut self) {
        for hero in &mut self.party {
            hero.long_rest();
        }
        for companion in &mut self.companions {
            companion.hp = companion.max_hp;
        }
    }
}

/// Something a hero does on its turn in a fight. Kept as data so that a player on another
/// machine can ask the host to do it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum FightIntent {
    Move(Cell),
    Attack { target: usize, weapon: usize },
    Cast { spell: String, slot: u8, target: usize },
    Dodge,
    Dash,
    Disengage,
    EndTurn,
    /// Drink or eat the thing at this place in the acting hero's bag.
    UseItem(usize),
}

impl World {
    /// The name of whoever plays the hero acting in the fight; `None` when a monster is
    /// acting, there is no fight, or the hero has no owner (a solo game).
    /// Where a player's hero stands in the fight going on, if there is one and it lives.
    pub fn fight_cell_of(&self, owner: &str) -> Option<(usize, usize)> {
        let index = self.owners.iter().position(|o| o == owner)?;
        let fighter = self.fight.as_ref()?.combatants.iter().find(|c| c.id == hero_id(index) && !c.dead)?;
        Some((fighter.x.max(0) as usize, fighter.y.max(0) as usize))
    }

    pub fn acting_owner(&self) -> Option<&str> {
        let fight = self.fight.as_ref()?;
        let id = fight.actor().id.as_str();
        // A companion is played by whoever took it on.
        if let Some(n) = id.strip_prefix("ally_").and_then(|n| n.parse::<usize>().ok()) {
            return self.companions.get(n).map(|c| c.owner.as_str()).filter(|o| !o.is_empty());
        }
        let index: usize = id.strip_prefix("hero_")?.parse().ok()?;
        self.owners.get(index).map(String::as_str).filter(|o| !o.is_empty())
    }

    /// Carries out what the acting hero's player asked for. Nothing happens, and no events
    /// come back, when it is not a hero's turn or the rules do not allow it.
    pub fn fight_act(&mut self, intent: &FightIntent) -> Vec<Event> {
        self.in_fight_place(|world| world.fight_act_here(intent))
    }

    fn fight_act_here(&mut self, intent: &FightIntent) -> Vec<Event> {
        let Some(mut fight) = self.fight.clone() else { return Vec::new() };
        if fight.actor().side != Side::Party {
            return Vec::new();
        }
        let events = match intent {
            FightIntent::Move(cell) => {
                let blocked = |c: Cell| self.fight_blocked(c);
                fight.move_to(*cell, &blocked)
            }
            FightIntent::Attack { target, weapon } if *target < fight.combatants.len() => fight.attack(*target, *weapon),
            FightIntent::Cast { spell, slot, target } if *target < fight.combatants.len() => {
                let hero = fight.actor().id.strip_prefix("hero_").and_then(|n| n.parse::<usize>().ok());
                let strike = hero.and_then(|h| spell_strike(self, h, spell, *slot));
                let can_pay = hero.and_then(|h| self.party.get(h)).is_some_and(|h| *slot == 0 || h.slots_left(*slot) > 0);
                match strike {
                    Some(strike) if can_pay => {
                        let events = fight.cast(&strike, *target);
                        // The slot is spent only when the spell was actually cast.
                        if !events.is_empty()
                            && let Some(h) = hero.and_then(|h| self.party.get_mut(h))
                        {
                            h.spend_slot(*slot);
                        }
                        events
                    }
                    _ => Vec::new(),
                }
            }
            FightIntent::UseItem(i) => {
                let hero = fight.actor().id.strip_prefix("hero_").and_then(|n| n.parse::<usize>().ok());
                let dice = hero
                    .and_then(|h| self.story.bags.get(h))
                    .and_then(|b| b.inventory.get(*i))
                    .filter(|item| !item.heal.trim().is_empty())
                    .and_then(|item| Dice::parse(&item.heal, 0));
                match (hero, dice) {
                    (Some(hero), Some(dice)) => {
                        let mut rng = Rng::from_clock();
                        let events = fight.mend_self(dice.roll(&mut rng).max(1));
                        // The thing is spent only when it was actually used.
                        if !events.is_empty() {
                            let bag = &mut self.story.bags[hero].inventory;
                            if bag[*i].count > 1 {
                                bag[*i].count -= 1;
                            } else {
                                bag.remove(*i);
                            }
                        }
                        events
                    }
                    _ => Vec::new(),
                }
            }
            FightIntent::Dodge => fight.dodge(),
            FightIntent::Dash => fight.dash(),
            FightIntent::Disengage => fight.disengage(),
            FightIntent::EndTurn => fight.end_turn(),
            _ => Vec::new(),
        };
        self.fight = Some(fight);
        events
    }

    /// A monster's whole turn, when it is one's turn.
    pub fn fight_monster_turn(&mut self) -> Vec<Event> {
        self.in_fight_place(|world| world.fight_monster_turn_here())
    }

    fn fight_monster_turn_here(&mut self) -> Vec<Event> {
        let Some(mut fight) = self.fight.clone() else { return Vec::new() };
        if fight.actor().side != Side::Enemy {
            return Vec::new();
        }
        let blocked = |cell: Cell| self.fight_blocked(cell);
        let mut events = fight.enemy_turn(&blocked);
        events.extend(fight.end_turn());
        self.fight = Some(fight);
        events
    }
}

/// A spell as something to do in a fight: the strike when the rules can carry it out.
pub fn spell_strike(world: &World, hero: usize, spell: &str, slot: u8) -> Option<Strike> {
    Strike::spell(world.party.get(hero)?, srd().spells.get(spell)?, slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{WorldDraft, build_world};
    use crate::fake;
    use crate::story::read_effects;
    use rules::Character;
    use serde_json::json;

    fn world() -> World {
        let draft: WorldDraft = serde_json::from_str(&fake::world("party")).unwrap();
        let mut w = build_world(&draft, "party", "English").unwrap().0;
        let skills = vec!["acrobatics".to_string(), "athletics".to_string()];
        w.party.push(Character::create("Brak", "", "human", "fighter", [15, 12, 14, 8, 10, 13], &skills, &[]).unwrap());
        w
    }

    fn apply(w: &mut World, value: serde_json::Value) -> Vec<String> {
        let mut notes = Vec::new();
        let effects = read_effects(value.as_array().unwrap(), &mut notes);
        assert!(notes.is_empty(), "{notes:?}");
        w.apply(&effects, None)
    }

    fn check(dc: i32) -> serde_json::Value {
        json!([
            { "op": "check", "ability": "str", "skill": "athletics", "dc": dc,
              "success": { "narration": "The door gives.", "effects": [{ "op": "give", "name": "Splinter" }] },
              "failure": { "narration": "Your shoulder aches.", "effects": [{ "op": "damage", "amount": "2" }] } },
            { "op": "fact", "text": "The door was tried." }
        ])
    }

    #[test]
    fn a_check_waits_for_the_die_and_holds_back_what_follows_it() {
        let mut w = world();
        apply(&mut w, check(10));
        assert!(w.pending.is_some());
        assert!(w.story.facts.is_empty(), "the fact comes after the check");
        assert!(w.story.bags[0].inventory.is_empty());
    }

    #[test]
    fn the_roll_decides_the_branch_and_then_the_rest_follows() {
        // Strength 16 (+3) with athletics (+2): an easy check always succeeds.
        let mut w = world();
        apply(&mut w, check(1));
        let done = w.roll_check(&mut Rng::seeded(1)).unwrap();
        assert!(done.success && done.narration == "The door gives." && done.hero == "Brak");
        assert_eq!(done.roll.bonus, 5);
        assert_eq!(w.story.bags[0].inventory.len(), 1);
        assert_eq!(w.story.facts, ["The door was tried."]);
        assert!(w.pending.is_none());

        // An impossible one always fails, and the failure's damage lands.
        let mut w = world();
        apply(&mut w, check(40));
        let done = w.roll_check(&mut Rng::seeded(1)).unwrap();
        assert!(!done.success);
        assert_eq!(w.party[0].hp, w.party[0].max_hp - 2);
        assert!(w.story.bags[0].inventory.is_empty());
        assert_eq!(w.roll_check(&mut Rng::seeded(1)), None, "nothing left to roll");
    }

    #[test]
    fn a_check_succeeds_exactly_when_the_total_meets_the_difficulty() {
        let mut wins = 0;
        for seed in 0..300 {
            let mut w = world();
            apply(&mut w, check(15));
            let done = w.roll_check(&mut Rng::seeded(seed)).unwrap();
            assert_eq!(done.success, done.roll.total() >= 15);
            wins += done.success as u32;
        }
        // d20 + 5 against 15: eleven faces of twenty.
        assert!((130..200).contains(&wins), "{wins}");
    }

    #[test]
    fn healing_goes_to_the_most_wounded_and_story_experience_is_capped() {
        let mut w = world();
        w.party[0].take_damage(8);
        apply(&mut w, json!([{ "op": "heal", "amount": "5" }, { "op": "xp", "amount": 99999 }]));
        assert_eq!(w.party[0].hp, w.party[0].max_hp - 3);
        assert_eq!(w.party[0].xp, 100, "one level's worth at most");
    }

    #[test]
    fn a_creature_fights_with_its_srd_numbers_its_own_or_a_fallback() {
        let mut w = world();
        let mut cat = w.here().creatures[0].clone();
        cat.monster = Some("goblin".into());
        assert_eq!((w.combatant_for(&cat).ac, w.combatant_for(&cat).hp), (15, 7));
        cat.monster = Some("no-such-monster".into());
        cat.stats = Some(Stats { ac: 14, hp: 20, speed: 40, attacks: vec![StatAttack { name: "Claws".into(), bonus: 5, damage: "2d4+3".into(), range: 5 }], xp: 100 });
        let own = w.combatant_for(&cat);
        assert_eq!((own.ac, own.hp, own.speed, own.xp), (14, 20, 8, 100));
        assert_eq!(own.strikes[0].damage, [Dice::new(2, 4, 3)]);
        cat.stats = None;
        let plain = w.combatant_for(&cat);
        assert_eq!((plain.ac, plain.hp), (12, 11));
        assert_eq!(plain.strikes.len(), 1);
        w.party[0].gain_xp(6_500);
        assert_eq!(w.combatant_for(&cat).hp, 31, "the fallback grows with the party");
    }

    #[test]
    fn a_fight_starts_only_when_something_hostile_is_here() {
        let mut w = world();
        assert!(!w.start_fight(Rng::seeded(1)));
        assert!(!w.hostile_near(99));
        apply(&mut w, json!([{ "op": "attitude", "id": "cat", "attitude": "hostile" }]));
        assert!(w.hostile_near(99) && !w.hostile_near(0));
        assert!(w.start_fight(Rng::seeded(1)));
        let fight = w.fight.as_ref().unwrap();
        assert_eq!(fight.combatants.len(), 2);
        assert_eq!(fight.combatants[0].id, "hero_0");
        assert!(!w.start_fight(Rng::seeded(1)), "one fight at a time");
    }

    #[test]
    fn the_world_keeps_standings_threads_threats_and_what_wanders_in() {
        let mut w = world();
        apply(&mut w, json!([{ "op": "faction", "id": "Bog Tribe", "name": "The Bog Tribe", "description": "Beetle folk.", "standing": 0 }]));
        apply(&mut w, json!([{ "op": "standing", "id": "bog_tribe", "change": -2 }, { "op": "thread", "text": "The shaman was robbed; his tribe hunts the thief." }, { "op": "threat", "name": "Ritual", "what": "The cult wakes the thing below.", "rumour": "Chanting under the hill.", "hours": 2 }]));
        assert_eq!((w.factions[0].standing, w.factions[0].met), (-2, true));
        assert_eq!(w.threads.len(), 1);
        assert!(w.threats_due().is_empty());
        w.pass(121);
        assert_eq!(w.threats_due().len(), 1);
        let text = crate::prompt::turn(&w, "x", None);
        assert!(text.contains("A THREAT HAS COME DUE") && text.contains("bog_tribe") && text.contains("THREADS"), "{text}");
        apply(&mut w, json!([{ "op": "threat_end", "name": "ritual" }, { "op": "thread_done", "text": "shaman" }]));
        assert!(w.threats.is_empty() && w.threads.is_empty());
        // A dangerous place with something to wander in: sooner or later it comes.
        w.here_mut().danger = 5;
        w.here_mut().wanderers = vec!["wolf".into()];
        w.here_mut().finds = vec!["A peddler passes.".into()];
        let mut rng = Rng::seeded(7);
        let mut seen = Vec::new();
        for _ in 0..200 {
            if let Some(what) = w.wander(&mut rng) {
                seen.push(what);
                // Once something hostile stands here, nothing more is rolled.
                if w.hostiles().next().is_some() {
                    break;
                }
            }
        }
        assert!(seen.iter().any(|s| matches!(s, crate::living::Wandered::Ambush(_))), "{seen:?}");
        assert_eq!(w.wander(&mut rng), None);
        w.here_mut().danger = 0;
    }

    #[test]
    fn someone_joins_fights_beside_the_party_and_can_be_sent_away() {
        let mut w = world();
        apply(&mut w, json!([{ "op": "spawn", "kind": "creature", "id": "guard", "name": "Old guard", "x": 6, "y": 6, "sprite": "cat", "attitude": "friendly", "monster": "guard" }]));
        let notes = apply(&mut w, json!([{ "op": "join", "id": "guard" }]));
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(w.companions.len(), 1);
        assert!(w.here().creatures.iter().all(|c| c.id != "guard"));
        assert_eq!((w.room_for_companions(), w.companions_here()), (1, vec![0]));
        // In a fight the guard is on the party's side and is the lone player's to move.
        apply(&mut w, json!([{ "op": "attitude", "id": "cat", "attitude": "hostile" }]));
        assert!(w.start_fight(Rng::seeded(4)));
        let fight = w.fight.as_ref().unwrap();
        let ally = fight.combatants.iter().find(|c| c.id == "ally_0").unwrap();
        assert_eq!((ally.side, ally.name.as_str()), (Side::Party, "Old guard"));
        // A wound taken is kept; then the guard is sent away and stands in the place again.
        w.fight.as_mut().unwrap().combatants.iter_mut().find(|c| c.id == "ally_0").unwrap().hp = 3;
        for c in w.fight.as_mut().unwrap().combatants.iter_mut().filter(|c| c.side == Side::Enemy) {
            c.hp = 0;
            c.dead = true;
        }
        w.end_fight();
        assert_eq!(w.companions[0].hp, 3);
        apply(&mut w, json!([{ "op": "dismiss", "id": "guard" }]));
        assert!(w.companions.is_empty() && w.here().creatures.iter().any(|c| c.id == "guard"));
    }

    #[test]
    fn a_hostile_creature_stands_for_a_pack_matched_to_the_party() {
        let mut w = world();
        apply(&mut w, json!([{ "op": "attitude", "id": "cat", "attitude": "hostile" }]));
        // One first-level hero: a rat each is worth little, so several come; a goblin comes alone.
        w.here_mut().creatures[0].monster = Some("giant-rat".into());
        assert_eq!(w.encounter_budget(), 75);
        assert_eq!((w.pack_size(25), w.pack_size(50), w.pack_size(10)), (2, 1, 3));
        assert!(w.muster());
        assert_eq!(w.here().creatures.len(), 2);
        assert_eq!(w.here().creatures[1].id, "cat_2");
        assert_ne!((w.here().creatures[1].x, w.here().creatures[1].y), (w.here().creatures[0].x, w.here().creatures[0].y));
        assert!(!w.muster(), "counted out once");
        // The master's own number is kept, whatever the budget.
        apply(&mut w, json!([{ "op": "spawn", "kind": "creature", "id": "wolf", "name": "Wolf", "x": 5, "y": 5, "sprite": "cat", "attitude": "hostile", "monster": "wolf", "count": 3 }]));
        w.muster();
        assert_eq!(w.here().creatures.iter().filter(|c| c.id.starts_with("wolf")).count(), 3);
        assert!(w.start_fight(Rng::seeded(3)));
        assert_eq!(w.fight.as_ref().unwrap().combatants.len(), 6);
    }

    #[test]
    fn a_potion_drunk_in_a_fight_heals_costs_the_action_and_is_spent() {
        let mut w = world();
        apply(&mut w, json!([{ "op": "attitude", "id": "cat", "attitude": "hostile" }, { "op": "give", "name": "Potion", "kind": "potion", "heal": "2d4+2" }]));
        w.start_fight(Rng::seeded(1));
        let fight = w.fight.as_mut().unwrap();
        // The hero's turn, wounded.
        fight.turn = fight.order.iter().position(|i| *i == 0).unwrap();
        fight.combatants[0].hp = 3;
        fight.combatants[0].action_left = true;
        let events = w.fight_act(&FightIntent::UseItem(0));
        assert!(matches!(events.as_slice(), [Event::Healed { amount, .. }] if *amount >= 4), "{events:?}");
        let fight = w.fight.as_ref().unwrap();
        assert!(fight.combatants[0].hp >= 7 && !fight.combatants[0].action_left);
        assert!(w.story.bags[0].inventory.is_empty());
        assert!(w.fight_act(&FightIntent::UseItem(0)).is_empty(), "nothing left to drink, no action left");
    }

    #[test]
    fn winning_removes_the_dead_pays_experience_and_keeps_the_wounds() {
        let mut w = world();
        apply(&mut w, json!([{ "op": "attitude", "id": "cat", "attitude": "hostile" }]));
        w.here_mut().creatures[0].monster = Some("goblin".into());
        w.start_fight(Rng::seeded(2));
        let fight = w.fight.as_mut().unwrap();
        fight.combatants[1].hp = 0;
        fight.combatants[1].dead = true;
        fight.combatants[0].hp = 5;
        fight.combatants[0].x = 3;
        fight.combatants[0].y = 3;
        let summary = w.end_fight().unwrap();
        assert_eq!(summary, FightSummary { won: true, xp_each: 50, level_ups: vec![] });
        assert!(w.here().creatures.is_empty());
        assert_eq!((w.party[0].hp, w.party[0].xp), (5, 50));
        assert_eq!(w.player.cell(), (3, 3));
        assert!(w.fight.is_none());
        assert_eq!(w.end_fight(), None);
        // The body lies where the creature fell, and the story can raise it.
        let body = w.here().props.iter().find(|p| p.body.is_some()).expect("a body is left");
        assert_eq!(body.id, "cat_body");
        assert!(w.story.facts.iter().any(|f| f.contains("killed")), "the master is told");
        apply(&mut w, json!([{ "op": "revive", "hero": "cat" }]));
        assert_eq!(w.here().creatures.len(), 1);
        assert!(w.here().props.iter().all(|p| p.body.is_none()));
    }

    #[test]
    fn losing_pays_nothing_and_the_fallen_can_be_put_back_on_their_feet() {
        let mut w = world();
        apply(&mut w, json!([{ "op": "attitude", "id": "cat", "attitude": "hostile" }]));
        w.start_fight(Rng::seeded(2));
        w.fight.as_mut().unwrap().combatants[0].hp = 0;
        let summary = w.end_fight().unwrap();
        assert!(!summary.won && summary.xp_each == 0);
        assert_eq!(w.here().creatures.len(), 1, "the victor is still there");
        assert!(w.party[0].is_down());
        w.revive_fallen();
        assert_eq!(w.party[0].hp, 1);
    }

    #[test]
    fn a_player_acts_through_intents_and_a_monster_takes_its_own_turn() {
        let mut w = world();
        w.owners = vec!["Ann".into()];
        apply(&mut w, json!([{ "op": "attitude", "id": "cat", "attitude": "hostile" }]));
        w.start_fight(Rng::seeded(2));
        let fight = w.fight.as_mut().unwrap();
        fight.turn = fight.order.iter().position(|i| *i == 0).unwrap();
        fight.combatants[0].movement_left = 6;
        fight.combatants[0].action_left = true;
        assert_eq!(w.acting_owner(), Some("Ann"));
        assert_eq!(w.fight_monster_turn(), [], "it is the hero turn");

        let at = w.fight.as_ref().unwrap().combatants[0].at();
        let blocked = |c: Cell| w.fight_blocked(c);
        let goal = *w.fight.as_ref().unwrap().reach(&blocked).first().unwrap().last().unwrap();
        let events = w.fight_act(&FightIntent::Move(goal));
        assert!(matches!(&events[0], Event::Moved { who, .. } if who == "hero_0"));
        assert_ne!(w.fight.as_ref().unwrap().combatants[0].at(), at);
        assert_eq!(w.fight_act(&FightIntent::Attack { target: 99, weapon: 0 }), [], "no such target");
        assert_eq!(w.fight_act(&FightIntent::Cast { spell: "fireball".into(), slot: 3, target: 1 }), [], "a fighter has no spells");

        let events = w.fight_act(&FightIntent::EndTurn);
        assert!(matches!(&events[0], Event::TurnBegan { who, .. } if who == "cat"));
        assert_eq!(w.acting_owner(), None);
        assert_eq!(w.fight_act(&FightIntent::Dodge), [], "not a hero turn");
        let events = w.fight_monster_turn();
        assert!(matches!(events.last(), Some(Event::TurnBegan { who, .. }) if who == "hero_0"));
    }

    #[test]
    fn in_a_fight_the_ground_blocks_but_the_fighters_do_not() {
        let mut w = world();
        apply(&mut w, json!([{ "op": "attitude", "id": "cat", "attitude": "hostile" }]));
        let cat = w.here().creatures[0].clone();
        assert!(w.fight_blocked((cat.x as i32, cat.y as i32)), "before the fight the cat is in the way");
        w.start_fight(Rng::seeded(2));
        assert!(!w.fight_blocked((cat.x as i32, cat.y as i32)));
        assert!(w.fight_blocked((0, 0)) && w.fight_blocked((-1, 5)), "trees and the edge of the map");
        let fire = w.here().props[0].clone();
        assert!(w.fight_blocked((fire.x as i32, fire.y as i32)));
    }

    #[test]
    fn a_hero_whose_player_is_away_takes_no_part_and_can_be_claimed() {
        let mut w = world();
        w.party.clear();
        let skills = vec!["acrobatics".to_string(), "athletics".to_string()];
        w.party.push(Character::create("Ann's", "", "human", "fighter", [15, 12, 14, 8, 10, 13], &skills, &[]).unwrap());
        w.party.push(Character::create("Bob's", "", "human", "fighter", [15, 12, 14, 8, 10, 13], &skills, &[]).unwrap());
        w.owners = vec!["Ann".into(), "Bob".into()];
        assert!(!w.is_away(1), "with nobody listed as present nobody is away");
        w.present = vec!["Ann".into()];
        assert!(w.is_away(1) && !w.is_away(0));
        w.here_mut().creatures[0].attitude = Attitude::Hostile;
        assert!(w.start_fight(Rng::seeded(3)));
        let fight = w.fight.as_ref().unwrap();
        assert!(fight.index_of("hero_0").is_some() && fight.index_of("hero_1").is_none());
        w.fight = None;
        assert!(!w.claim(0, "Carl"), "Ann is here");
        assert!(!w.claim(1, "Ann"), "Ann has a hero already");
        assert!(w.claim(1, "Carl"));
        assert_eq!(w.owners, ["Ann", "Carl"]);
    }


    #[test]
    fn abilities_for_a_level_are_offered_cut_to_size_and_one_is_chosen() {
        let mut w = world();
        w.party[0].gain_xp(300);
        assert_eq!(w.feat_wanted(), Some((0, false)));
        let feats = vec![
            rules::Feat { name: "Storm".into(), dice: "12d12".into(), uses: 1, ..rules::Feat::default() },
            rules::Feat { name: "Thick skin".into(), ac: 9, ..rules::Feat::default() },
        ];
        assert_eq!(w.take_feats(0, false, feats), None);
        assert_eq!(w.feat_wanted(), None, "an offer is out; the master is not asked again");
        let offered = &w.offers[0].feats;
        assert!(rules::Dice::parse(&offered[0].dice, 0).unwrap().average() <= 18.0, "{}", offered[0].dice);
        assert_eq!(offered[1].ac, 1);
        assert!(!w.choose_feat(0, 5) && !w.choose_feat(3, 0));
        let ac = w.party[0].ac();
        assert!(w.choose_feat(0, 1));
        assert_eq!((w.party[0].ac(), w.party[0].feat_due, w.offers.len()), (ac + 1, false, 0));
    }

    #[test]
    fn a_secret_skill_is_granted_at_once_and_a_book_table_has_none() {
        let mut w = world();
        w.secrets_due.push(0);
        assert_eq!(w.feat_wanted(), Some((0, true)));
        let name = w.take_feats(0, true, vec![rules::Feat { name: "Wolf's nose".into(), dice: "1d4".into(), ..rules::Feat::default() }]);
        assert_eq!(name.as_deref(), Some("Wolf's nose"));
        assert!(w.party[0].feats[0].secret && w.secrets_due.is_empty());
        let mut book = world();
        book.table.book_only = true;
        book.party[0].gain_xp(300);
        book.secrets_due.push(0);
        assert_eq!(book.feat_wanted(), None);
        book.settle_feats();
        assert!(!book.party[0].feat_due && book.secrets_due.is_empty());
        for _ in 0..200 {
            book.deed("Brak", &mut Rng::seeded(1));
        }
        assert!(book.secrets_due.is_empty());
    }

    #[test]
    fn deeds_earn_a_secret_now_and_then_and_only_so_many() {
        let mut w = world();
        let mut rng = Rng::seeded(11);
        for _ in 0..100 {
            w.deed("Brak", &mut rng);
        }
        assert_eq!(w.secrets_due, [0], "earned once, not a hundred times");
        w.take_feats(0, true, vec![rules::Feat { name: "One".into(), ..rules::Feat::default() }]);
        for _ in 0..100 {
            w.deed("", &mut rng);
        }
        assert!(w.secrets_due.is_empty(), "a first-level hero holds one secret");
    }

    #[test]
    fn the_dead_come_back_only_when_the_story_says_so() {
        let mut w = world();
        assert!(!w.revive("Brak"), "Brak is alive");
        w.party[0].dead = true;
        w.party[0].hp = 0;
        let notes = w.apply(&read_effects(&[json!({ "op": "revive", "hero": "Brak" })], &mut Vec::new()), None);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!((w.party[0].dead, w.party[0].hp), (false, 1));
    }

    #[test]
    fn story_harm_is_capped_by_level_unless_the_table_plays_without_limits() {
        let mut w = world();
        w.party[0].max_hp = 400;
        w.party[0].hp = 400;
        assert_eq!(w.hurt_party("300", &mut Rng::seeded(1)), 20);
        w.table.no_limits = true;
        assert_eq!(w.hurt_party("300", &mut Rng::seeded(1)), 300);
    }

    #[test]
    fn a_stunt_rolls_the_check_and_a_success_does_what_the_master_allowed() {
        let mut w = world();
        w.here_mut().creatures[0].attitude = Attitude::Hostile;
        assert!(w.start_fight(Rng::seeded(2)));
        let fight = w.fight.as_mut().unwrap();
        fight.turn = fight.order.iter().position(|i| *i == 0).unwrap();
        fight.combatants[0].action_left = true;
        let draft: StuntDraft = serde_json::from_value(json!({ "narration": "You kick the pot over.", "ability": "str", "skill": "athletics", "dc": "1", "target": "cat", "damage": "2d4", "success": "It scalds the cat.", "failure": "It rolls away." })).unwrap();
        let stunt = w.fight_stunt(&draft, "Pot", &mut Rng::seeded(3)).expect("it is Brak's turn");
        assert!(stunt.success && stunt.narration.ends_with("It scalds the cat."));
        assert!(matches!(&stunt.events[0], Event::Struck { target, .. } if target == "cat"), "{:?}", stunt.events);
        assert!(!w.fight.as_ref().unwrap().combatants[0].action_left);
        assert!(w.fight_stunt(&draft, "Pot", &mut Rng::seeded(3)).is_none(), "the action is spent");
    }

    #[test]
    fn a_book_table_ignores_numbers_the_master_made_up() {
        let mut w = world();
        let mut creature = w.here().creatures[0].clone();
        creature.stats = Some(Stats { ac: 19, hp: 77, speed: 30, attacks: Vec::new(), xp: 10 });
        assert_eq!(w.combatant_for(&creature).max_hp, 70, "capped for a first-level party");
        w.table.no_limits = true;
        assert_eq!(w.combatant_for(&creature).max_hp, 77);
        w.table.book_only = true;
        assert_eq!(w.combatant_for(&creature).max_hp, 11, "the plain opponent");
    }


    #[test]
    fn a_party_that_split_up_fights_and_rolls_where_each_hero_is() {
        let mut w = world();
        w.party.clear();
        let skills = vec!["acrobatics".to_string(), "athletics".to_string()];
        for name in ["Ann's", "Bob's"] {
            w.party.push(Character::create(name, "", "human", "fighter", [15, 12, 14, 8, 10, 13], &skills, &[]).unwrap());
        }
        w.owners = vec!["Ann".into(), "Bob".into()];
        w.present = vec!["Ann".into(), "Bob".into()];
        // Bob walked on to the east road; Ann, the host, stayed in the camp.
        let draft: crate::LocationDraft = serde_json::from_str(&fake::location("east_road", "The East Road", "camp")).unwrap();
        w.add_location(&draft, "east_road", Some("camp")).unwrap();
        w.places.insert("Bob".into(), "east_road".into());
        assert_eq!((w.hero_place(0), w.hero_place(1)), ("camp", "east_road"));
        assert!(w.takes_part(0) && !w.takes_part(1), "seen from the camp only Ann's hero is here");

        // The wolf of the east road notices Bob: the fight is there, and only Bob's hero is in it.
        let home = w.enter("east_road", Player::in_cell(19, 8));
        assert!(w.takes_part(1) && !w.takes_part(0));
        assert!(w.start_fight(Rng::seeded(5)));
        w.leave(home);
        assert_eq!((w.current.as_str(), w.fight_place.as_str()), ("camp", "east_road"));
        assert!(!w.fight_here(), "the host is not in the fight");
        let fight = w.fight.as_ref().unwrap();
        assert!(fight.index_of("hero_1").is_some() && fight.index_of("hero_0").is_none());

        // The fight goes on from the host's machine without the host being there.
        let before = w.player;
        let fight = w.fight.as_mut().unwrap();
        for c in fight.combatants.iter_mut().filter(|c| c.side == Side::Enemy) {
            c.hp = 0;
            c.dead = true;
        }
        let summary = w.end_fight().expect("the fight ends");
        assert!(summary.won);
        assert_eq!((w.player, w.current.as_str(), w.fight_place.as_str()), (before, "camp", ""));
        assert_eq!((w.party[0].xp, w.party[1].xp > 0), (0, true), "only the one who fought learns from it");
        assert!(w.locations["east_road"].creatures.iter().all(|c| c.id != "wolf"), "the dead wolf is gone from its own place");
    }

    #[test]
    fn a_place_that_does_not_exist_is_not_entered() {
        let mut w = world();
        let home = w.enter("nowhere", Player::in_cell(1, 1));
        assert_eq!(w.current, home.0);
        w.places.insert("Bob".into(), "nowhere".into());
        assert_eq!(w.place_of("Bob"), w.current, "a guest whose place is gone is with the host");
    }

}
