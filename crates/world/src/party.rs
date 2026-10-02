//! The party and what the rules do to it: checks whose two outcomes the game master wrote
//! in advance, damage and healing outside a fight, experience, and fights themselves.

use crate::model::{Attitude, Creature, MAP_H, MAP_W, Player, World};
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

/// Story experience the master may hand out in one go, per character level: enough to
/// matter, not enough to skip levels.
const STORY_XP_PER_LEVEL: u32 = 100;

fn hero_id(index: usize) -> String {
    format!("hero_{index}")
}

impl World {
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
        let stats = creature.stats.as_ref().filter(|s| s.hp > 0).unwrap_or(&fallback);
        let monster = rules::data::Monster {
            index: creature.id.clone(),
            name: creature.name.clone(),
            ac: stats.ac.clamp(5, 30),
            hp: stats.hp.clamp(1, 999),
            speed: stats.speed.clamp(5, 120),
            abilities: [10; 6],
            attacks: stats
                .attacks
                .iter()
                .filter_map(|a| Some(rules::data::MonsterAttack { name: a.name.clone(), bonus: a.bonus.clamp(-5, 20), damage: vec![Dice::parse(&a.damage, 0)?], range: a.range.max(5), save: None }))
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
        let best = self.party.iter().filter(|h| !h.is_down() && !h.dead).max_by_key(|h| h.check_bonus(check.ability, skill));
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
        let amount = Dice::parse(dice, 0).map_or(0, |d| d.roll(rng)).clamp(0, 500);
        if let Some(hero) = self.party.iter_mut().find(|h| !h.is_down() && !h.dead) {
            hero.take_damage(amount);
        }
        amount
    }

    pub(crate) fn heal_party(&mut self, dice: &str, rng: &mut Rng) -> i32 {
        let amount = Dice::parse(dice, 0).map_or(0, |d| d.roll(rng)).clamp(0, 500);
        // The one who needs it most: the lowest share of hit points left.
        if let Some(hero) = self.party.iter_mut().filter(|h| !h.dead).min_by_key(|h| h.hp * 1000 / h.max_hp.max(1)) {
            hero.heal(amount);
        }
        amount
    }

    /// Experience from the story, capped so a generous master cannot skip levels.
    pub(crate) fn award_story_xp(&mut self, amount: i64) -> Vec<(String, LevelUp)> {
        let capped = amount.clamp(0, (STORY_XP_PER_LEVEL * self.party_level()) as i64) as u32;
        self.award_xp(capped)
    }

    fn award_xp(&mut self, each: u32) -> Vec<(String, LevelUp)> {
        let mut ups = Vec::new();
        for hero in self.party.iter_mut().filter(|h| !h.dead) {
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
        if x < 0 || y < 0 || x >= MAP_W as i32 || y >= MAP_H as i32 {
            return true;
        }
        let here = self.here();
        let (ux, uy) = (x as usize, y as usize);
        let solid = here.tile_id(ux, uy).and_then(|id| self.tiles.get(id)).is_none_or(|t| !t.walkable);
        let fighting = |id: &str| self.fight.as_ref().is_some_and(|f| f.index_of(id).is_some());
        solid || here.props.iter().any(|p| p.blocking && (p.x, p.y) == (ux, uy)) || here.creatures.iter().any(|c| (c.x, c.y) == (ux, uy) && !fighting(&c.id))
    }

    /// Starts a fight with every hostile creature here. `false` when there is nobody to
    /// fight or nobody to do the fighting.
    pub fn start_fight(&mut self, rng: Rng) -> bool {
        if self.fight.is_some() || self.party.iter().all(|h| h.dead) || self.hostiles().next().is_none() {
            return false;
        }
        let enemies: Vec<Combatant> = self.hostiles().map(|c| self.combatant_for(c)).collect();
        let inside = |(x, y): (i32, i32)| (x.clamp(0, MAP_W as i32 - 1) as usize, y.clamp(0, MAP_H as i32 - 1) as usize);
        let leader = inside(self.player.cell());
        let mut taken: Vec<(usize, usize)> = Vec::new();
        let mut combatants = Vec::new();
        let here = self.here();
        for (i, hero) in self.party.iter().enumerate().filter(|(_, h)| !h.dead) {
            // A hero whose player walks a token of their own starts where that token stands;
            // the rest stand with the leader. Each takes the nearest free cell.
            let start = self.owners.get(i).and_then(|owner| self.others.iter().find(|t| t.name == *owner)).map_or(leader, |t| inside(Player { x: t.x, y: t.y }.cell()));
            let free = |c: (usize, usize)| !here.blocked(c.0 as i32, c.1 as i32, &self.tiles) && !taken.contains(&c);
            let cell = nearest(start, free).unwrap_or(start);
            taken.push(cell);
            combatants.push(Combatant::hero(hero, &hero_id(i), (cell.0 as i32, cell.1 as i32)));
        }
        combatants.extend(enemies);
        self.fight = Some(Encounter::start(combatants, rng).0);
        true
    }

    /// Ends the fight: heroes keep the wounds they took, the dead leave the map, the
    /// survivors share the experience.
    pub fn end_fight(&mut self) -> Option<FightSummary> {
        let fight = self.fight.take()?;
        let won = fight.outcome() == Outcome::Victory;
        for (i, hero) in self.party.iter_mut().enumerate() {
            if let Some(c) = fight.combatants.iter().find(|c| c.id == hero_id(i)) {
                hero.hp = c.hp;
                hero.death = c.death;
                hero.dead = c.dead;
            }
        }
        let here = self.current.clone();
        if let Some(location) = self.locations.get_mut(&here) {
            for c in fight.combatants.iter().filter(|c| c.side == Side::Enemy) {
                if c.dead {
                    location.creatures.retain(|creature| creature.id != c.id);
                } else if let Some(creature) = location.creatures.iter_mut().find(|creature| creature.id == c.id) {
                    (creature.x, creature.y) = (c.x.max(0) as usize, c.y.max(0) as usize);
                }
            }
        }
        // The host's own token ends where its hero stood: the first hero whose player has
        // no token of their own, or failing that anyone still alive.
        let walks_alone = |c: &&Combatant| {
            let owner = c.id.strip_prefix("hero_").and_then(|i| i.parse::<usize>().ok()).and_then(|i| self.owners.get(i));
            owner.is_some_and(|o| self.others.iter().any(|t| t.name == *o))
        };
        let alive = || fight.combatants.iter().filter(|c| c.side == Side::Party && !c.dead);
        if let Some(leader) = alive().find(|c| !walks_alone(c)).or_else(|| alive().next()) {
            self.player = Player::in_cell(leader.x.max(0) as usize, leader.y.max(0) as usize);
        }
        let survivors = self.party.iter().filter(|h| !h.dead).count().max(1) as u32;
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
        let index: usize = fight.actor().id.strip_prefix("hero_")?.parse().ok()?;
        self.owners.get(index).map(String::as_str).filter(|o| !o.is_empty())
    }

    /// Carries out what the acting hero's player asked for. Nothing happens, and no events
    /// come back, when it is not a hero's turn or the rules do not allow it.
    pub fn fight_act(&mut self, intent: &FightIntent) -> Vec<Event> {
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
        assert!(w.story.inventory.is_empty());
    }

    #[test]
    fn the_roll_decides_the_branch_and_then_the_rest_follows() {
        // Strength 16 (+3) with athletics (+2): an easy check always succeeds.
        let mut w = world();
        apply(&mut w, check(1));
        let done = w.roll_check(&mut Rng::seeded(1)).unwrap();
        assert!(done.success && done.narration == "The door gives." && done.hero == "Brak");
        assert_eq!(done.roll.bonus, 5);
        assert_eq!(w.story.inventory.len(), 1);
        assert_eq!(w.story.facts, ["The door was tried."]);
        assert!(w.pending.is_none());

        // An impossible one always fails, and the failure's damage lands.
        let mut w = world();
        apply(&mut w, check(40));
        let done = w.roll_check(&mut Rng::seeded(1)).unwrap();
        assert!(!done.success);
        assert_eq!(w.party[0].hp, w.party[0].max_hp - 2);
        assert!(w.story.inventory.is_empty());
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
}
