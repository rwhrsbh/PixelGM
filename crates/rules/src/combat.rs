//! A fight, turn by turn, on the grid of a location. Everything is decided here from dice
//! and numbers; the interface plays back the events, and the game master is not asked.

use crate::character::{Ability, Character, DeathSaves, modifier};
use crate::data::{Monster, Spell, srd};
use crate::dice::{Dice, Edge, Rng, Roll};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub type Cell = (i32, i32);

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Party,
    Enemy,
}

/// One way of hurting (or healing): a weapon, a monster's attack, a spell.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Strike {
    pub name: String,
    /// Added to the d20 when the strike is rolled to hit.
    pub bonus: i32,
    pub damage: Vec<Dice>,
    /// Cells. Beyond `range` and up to `long_range` the attack has disadvantage.
    pub range: u32,
    pub long_range: u32,
    /// When set, targets roll a save instead of the attacker rolling to hit: ability,
    /// difficulty, and whether a success still takes half.
    pub save: Option<(Ability, i32, bool)>,
    /// Cells around the target cell that are caught as well; 0 for a single target.
    pub area: u32,
    /// The strike restores hit points instead of taking them.
    pub heals: bool,
    /// The strike cannot miss and cannot be resisted: no roll is made.
    pub sure: bool,
    /// The hero's feat this strike comes from, by its place in the hero's list.
    #[serde(default)]
    pub feat: Option<usize>,
    /// How many more times it can be used; `None` for no limit.
    #[serde(default)]
    pub uses_left: Option<u8>,
}

fn cells(feet: u32) -> u32 {
    (feet / 5).max(1)
}

impl Strike {
    pub fn weapon(character: &Character, index: &str) -> Self {
        let (name, range) = match srd().weapons.get(index) {
            Some(w) => (w.name.clone(), w.range),
            None => ("Unarmed strike".to_string(), (5, 5)),
        };
        Self { name, bonus: character.attack_bonus(index), damage: vec![character.damage(index)], range: cells(range.0), long_range: cells(range.1), ..Self::default() }
    }

    /// A feat that harms or heals, as a strike. `None` for feats that only change numbers.
    pub fn feat(character: &Character, index: usize) -> Option<Self> {
        let feat = character.feats.get(index)?;
        let dice = Dice::parse(&feat.dice, 0).filter(|d| d.average() > 0.0)?;
        let range = cells(feat.range.max(5));
        let modifier = character.feat_modifier();
        let base = Self { name: feat.name.clone(), damage: vec![dice], range, long_range: range, area: feat.area / 5, feat: Some(index), uses_left: feat.uses_left(), ..Self::default() };
        Some(match (feat.heals, feat.save) {
            (true, _) => Self { heals: true, ..base },
            (false, Some(save)) => Self { save: Some((save, 8 + character.proficiency() + modifier, true)), ..base },
            (false, None) => Self { bonus: character.proficiency() + modifier, ..base },
        })
    }

    /// A spell the rules can carry out, cast with a slot of `slot` (0 for a cantrip).
    /// `None` for spells that only the game master can adjudicate.
    pub fn spell(character: &Character, spell: &Spell, slot: u8) -> Option<Self> {
        let ability = character.casting_ability().map_or(0, |a| character.modifier(a));
        let range = cells(spell.range.max(5));
        let base = Self { name: spell.name.clone(), range, long_range: range, area: spell.area / 5, ..Self::default() };
        if !spell.heal.is_empty() {
            return Some(Self { damage: vec![Spell::dice_at(&spell.heal, slot, ability)?], heals: true, ..base });
        }
        let key = if spell.damage_by_character_level { character.level.min(20) as u8 } else { slot };
        let damage = vec![Spell::dice_at(&spell.damage, key, ability)?];
        match (spell.attack, spell.save) {
            (Some(_), _) => Some(Self { bonus: character.spell_attack(), damage, ..base }),
            (None, Some((save, half))) => Some(Self { save: Some((save, character.spell_dc(), half)), damage, ..base }),
            (None, None) => Some(Self { sure: true, damage, ..base }),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Combatant {
    /// The id of the creature in the location, or of the hero in the party.
    pub id: String,
    pub name: String,
    pub side: Side,
    pub x: i32,
    pub y: i32,
    pub hp: i32,
    pub max_hp: i32,
    pub ac: i32,
    /// Cells per turn.
    pub speed: u32,
    /// Bonus to each of the six saving throws.
    pub saves: [i32; 6],
    pub initiative_bonus: i32,
    pub strikes: Vec<Strike>,
    /// The strikes made with one Attack action, as indices into `strikes`.
    pub routine: Vec<usize>,
    pub xp: u32,
    pub initiative: i32,
    pub movement_left: u32,
    pub action_left: bool,
    pub reaction_left: bool,
    pub dodging: bool,
    pub disengaged: bool,
    pub death: DeathSaves,
    pub dead: bool,
    /// Dying heroes who rolled three successes: down, but no longer rolling.
    pub stable: bool,
}

impl Combatant {
    fn blank(id: &str, name: &str, side: Side, at: Cell) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            side,
            x: at.0,
            y: at.1,
            hp: 1,
            max_hp: 1,
            ac: 10,
            speed: 6,
            saves: [0; 6],
            initiative_bonus: 0,
            strikes: Vec::new(),
            routine: Vec::new(),
            xp: 0,
            initiative: 0,
            movement_left: 0,
            action_left: false,
            reaction_left: true,
            dodging: false,
            disengaged: false,
            death: DeathSaves::default(),
            dead: false,
            stable: false,
        }
    }

    pub fn hero(character: &Character, id: &str, at: Cell) -> Self {
        let mut strikes: Vec<Strike> = if character.weapons.is_empty() { vec![Strike::weapon(character, "")] } else { character.weapons.iter().map(|w| Strike::weapon(character, w)).collect() };
        strikes.extend((0..character.feats.len()).filter_map(|i| Strike::feat(character, i)));
        Self {
            hp: character.hp,
            max_hp: character.max_hp,
            ac: character.ac(),
            speed: cells(character.speed()),
            saves: Ability::ALL.map(|a| character.save_bonus(a)),
            initiative_bonus: character.modifier(Ability::Dex),
            routine: vec![0; character.attacks_per_action() as usize],
            strikes,
            death: character.death,
            dead: character.dead,
            ..Self::blank(id, &character.name, Side::Party, at)
        }
    }

    pub fn monster(monster: &Monster, id: &str, name: &str, at: Cell) -> Self {
        let strikes: Vec<Strike> = monster
            .attacks
            .iter()
            .map(|a| Strike { name: a.name.clone(), bonus: a.bonus, damage: a.damage.clone(), range: cells(a.range), long_range: cells(a.range), save: a.save, area: if a.save.is_some() { 2 } else { 0 }, heals: false, sure: false, feat: None, uses_left: None })
            .collect();
        let mut routine: Vec<usize> = monster.multiattack.iter().flat_map(|(wanted, count)| strikes.iter().position(|s| &s.name == wanted).into_iter().flat_map(move |i| std::iter::repeat_n(i, *count as usize))).collect();
        if routine.is_empty() && !strikes.is_empty() {
            routine.push(0);
        }
        Self {
            hp: monster.hp,
            max_hp: monster.hp,
            ac: monster.ac,
            speed: cells(monster.speed),
            saves: monster.abilities.map(modifier),
            initiative_bonus: modifier(monster.abilities[Ability::Dex as usize]),
            strikes,
            routine,
            xp: monster.xp,
            ..Self::blank(id, name, Side::Enemy, at)
        }
    }

    pub fn at(&self) -> Cell {
        (self.x, self.y)
    }

    /// On its feet and able to act.
    pub fn standing(&self) -> bool {
        !self.dead && self.hp > 0
    }

    fn hurt(&mut self, amount: i32) -> Option<Event> {
        let amount = amount.max(0);
        if self.dead || amount == 0 {
            return None;
        }
        if self.side == Side::Enemy {
            self.hp = (self.hp - amount).max(0);
            self.dead = self.hp == 0;
            return self.dead.then(|| Event::Died { who: self.id.clone() });
        }
        // Heroes fall, and die only by failed saves or massive damage.
        if self.hp <= 0 {
            self.death.failures += 1;
            self.stable = false;
            self.dead = self.death.failures >= 3;
            return self.dead.then(|| Event::Died { who: self.id.clone() });
        }
        let left_over = amount - self.hp;
        self.hp = (self.hp - amount).max(0);
        if left_over >= self.max_hp {
            self.dead = true;
            return Some(Event::Died { who: self.id.clone() });
        }
        (self.hp == 0).then(|| Event::Fell { who: self.id.clone() })
    }

    fn mend(&mut self, amount: i32) {
        if !self.dead && amount > 0 {
            self.hp = (self.hp.max(0) + amount).min(self.max_hp);
            self.death = DeathSaves::default();
            self.stable = false;
        }
    }
}

/// What happened, in order, for the interface to play back and the log to record.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum Event {
    TurnBegan { who: String, round: u32 },
    Moved { who: String, path: Vec<Cell> },
    Attacked { who: String, target: String, strike: String, roll: Roll, hit: bool, critical: bool, damage: i32 },
    /// A strike that needs no roll landed.
    Struck { who: String, target: String, strike: String, damage: i32 },
    Saved { who: String, strike: String, roll: Roll, difficulty: i32, success: bool, damage: i32 },
    Healed { who: String, target: String, amount: i32 },
    Fell { who: String },
    Died { who: String },
    DeathSave { who: String, die: u32, successes: u8, failures: u8 },
    Revived { who: String },
    Dodged { who: String },
    Dashed { who: String },
    Disengaged { who: String },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Ongoing,
    Victory,
    Defeat,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Encounter {
    pub combatants: Vec<Combatant>,
    /// Indices into `combatants`, in initiative order.
    pub order: Vec<usize>,
    /// Position in `order` of whoever's turn it is.
    pub turn: usize,
    pub round: u32,
    pub rng: Rng,
}

/// Distance in cells, a diagonal step counting as one.
pub fn distance(a: Cell, b: Cell) -> u32 {
    (a.0 - b.0).unsigned_abs().max((a.1 - b.1).unsigned_abs())
}

const STEPS: [Cell; 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

impl Encounter {
    /// Rolls initiative and starts the first turn.
    pub fn start(mut combatants: Vec<Combatant>, mut rng: Rng) -> (Self, Vec<Event>) {
        for c in &mut combatants {
            c.initiative = rng.die(20) as i32 + c.initiative_bonus;
        }
        let mut order: Vec<usize> = (0..combatants.len()).collect();
        // Higher goes first; on a tie the party does, then the quicker.
        order.sort_by_key(|i| {
            let c = &combatants[*i];
            (-c.initiative, c.side != Side::Party, -c.initiative_bonus)
        });
        let mut encounter = Self { combatants, order, turn: 0, round: 1, rng };
        let events = encounter.begin_turn();
        (encounter, events)
    }

    pub fn actor_index(&self) -> usize {
        self.order[self.turn]
    }

    pub fn actor(&self) -> &Combatant {
        &self.combatants[self.actor_index()]
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.combatants.iter().position(|c| c.id == id)
    }

    fn begin_turn(&mut self) -> Vec<Event> {
        let round = self.round;
        let i = self.actor_index();
        let mut events = Vec::new();
        let c = &mut self.combatants[i];
        c.movement_left = c.speed;
        c.action_left = true;
        c.reaction_left = true;
        c.dodging = false;
        c.disengaged = false;
        events.push(Event::TurnBegan { who: c.id.clone(), round });
        if c.side == Side::Party && c.hp <= 0 && !c.dead && !c.stable {
            // A dying hero's whole turn is the roll.
            let die = self.rng.die(20);
            let c = &mut self.combatants[i];
            match die {
                20 => {
                    c.mend(1);
                    events.push(Event::DeathSave { who: c.id.clone(), die, successes: 0, failures: 0 });
                    events.push(Event::Revived { who: c.id.clone() });
                    return events;
                }
                1 => c.death.failures += 2,
                10.. => c.death.successes += 1,
                _ => c.death.failures += 1,
            }
            events.push(Event::DeathSave { who: c.id.clone(), die, successes: c.death.successes, failures: c.death.failures });
            if c.death.failures >= 3 {
                c.dead = true;
                events.push(Event::Died { who: c.id.clone() });
            } else if c.death.successes >= 3 {
                c.stable = true;
            }
        }
        events
    }

    /// Passes the turn on, skipping the dead and those who are down and stable.
    pub fn end_turn(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        for _ in 0..self.order.len() * 2 {
            self.turn += 1;
            if self.turn >= self.order.len() {
                self.turn = 0;
                self.round += 1;
            }
            let c = self.actor();
            let dying = c.side == Side::Party && c.hp <= 0 && !c.dead && !c.stable;
            if !c.standing() && !dying {
                continue;
            }
            events.extend(self.begin_turn());
            // A hero who is still down after the roll has no turn to take.
            if self.actor().standing() || self.outcome() != Outcome::Ongoing {
                break;
            }
        }
        events
    }

    pub fn outcome(&self) -> Outcome {
        let alive = |side: Side| self.combatants.iter().any(|c| c.side == side && c.standing());
        match (alive(Side::Party), alive(Side::Enemy)) {
            (_, false) => Outcome::Victory,
            (false, true) => Outcome::Defeat,
            _ => Outcome::Ongoing,
        }
    }

    /// Experience for every enemy killed so far.
    pub fn xp_won(&self) -> u32 {
        self.combatants.iter().filter(|c| c.side == Side::Enemy && c.dead).map(|c| c.xp).sum()
    }

    fn occupied(&self, cell: Cell, except: usize) -> Option<usize> {
        self.combatants.iter().enumerate().find(|(i, c)| *i != except && !c.dead && c.at() == cell).map(|(i, _)| i)
    }

    /// Every cell the actor can walk to this turn, with the path that gets there. Allies can
    /// be walked past but not stopped on; enemies and terrain block.
    pub fn reach(&self, blocked: &dyn Fn(Cell) -> bool) -> Vec<Vec<Cell>> {
        let me = self.actor_index();
        let actor = &self.combatants[me];
        let mut paths: Vec<Vec<Cell>> = vec![vec![actor.at()]];
        let mut queue = VecDeque::from([0usize]);
        while let Some(at) = queue.pop_front() {
            if paths[at].len() as u32 > actor.movement_left {
                continue;
            }
            let here = *paths[at].last().unwrap();
            for step in STEPS {
                let next = (here.0 + step.0, here.1 + step.1);
                let enemy_there = self.occupied(next, me).is_some_and(|i| self.combatants[i].side != actor.side && self.combatants[i].standing());
                if blocked(next) || enemy_there || paths.iter().any(|p| p.last() == Some(&next)) {
                    continue;
                }
                let mut path = paths[at].clone();
                path.push(next);
                paths.push(path);
                queue.push_back(paths.len() - 1);
            }
        }
        // Somewhere to stop: not the start, and not a cell someone is standing in.
        paths.into_iter().skip(1).filter(|p| self.occupied(*p.last().unwrap(), me).is_none()).collect()
    }

    /// Walks the actor to `to` if it can get there this turn. Leaving an enemy's reach
    /// without disengaging lets that enemy strike once.
    pub fn move_to(&mut self, to: Cell, blocked: &dyn Fn(Cell) -> bool) -> Vec<Event> {
        let Some(path) = self.reach(blocked).into_iter().find(|p| p.last() == Some(&to)) else { return Vec::new() };
        let me = self.actor_index();
        let mut events = Vec::new();
        let mut walked = vec![self.combatants[me].at()];
        for step in path.into_iter().skip(1) {
            let from = self.combatants[me].at();
            if !self.combatants[me].disengaged {
                let watchers: Vec<usize> = (0..self.combatants.len())
                    .filter(|i| {
                        let w = &self.combatants[*i];
                        w.side != self.combatants[me].side && w.standing() && w.reaction_left && distance(w.at(), from) <= 1 && distance(w.at(), step) > 1
                    })
                    .collect();
                for watcher in watchers {
                    let Some(strike) = self.combatants[watcher].strikes.iter().position(|s| s.range <= 1 && s.save.is_none() && !s.heals) else { continue };
                    self.combatants[watcher].reaction_left = false;
                    events.extend(self.resolve(watcher, strike, me));
                    if !self.combatants[me].standing() {
                        events.insert(0, Event::Moved { who: self.combatants[me].id.clone(), path: walked });
                        return events;
                    }
                }
            }
            (self.combatants[me].x, self.combatants[me].y) = step;
            self.combatants[me].movement_left -= 1;
            walked.push(step);
        }
        events.insert(0, Event::Moved { who: self.combatants[me].id.clone(), path: walked });
        events
    }

    /// One strike by `attacker` against `target`, or against everyone in its area.
    fn resolve(&mut self, attacker: usize, strike: usize, target: usize) -> Vec<Event> {
        let strike = self.combatants[attacker].strikes[strike].clone();
        self.resolve_strike(attacker, &strike, target)
    }

    fn resolve_strike(&mut self, attacker: usize, strike: &Strike, target: usize) -> Vec<Event> {
        let mut events = Vec::new();
        let who = self.combatants[attacker].id.clone();
        let centre = self.combatants[target].at();
        if strike.heals {
            let amount = strike.damage.iter().map(|d| d.roll(&mut self.rng)).sum::<i32>().max(1);
            let was_down = self.combatants[target].hp <= 0;
            self.combatants[target].mend(amount);
            events.push(Event::Healed { who, target: self.combatants[target].id.clone(), amount });
            if was_down && self.combatants[target].standing() {
                events.push(Event::Revived { who: self.combatants[target].id.clone() });
            }
            return events;
        }
        if let Some((ability, difficulty, half)) = strike.save {
            // Everyone in the area rolls, friend and foe alike; the caster is spared.
            let caught: Vec<usize> = (0..self.combatants.len()).filter(|i| *i != attacker && !self.combatants[*i].dead && distance(self.combatants[*i].at(), centre) <= strike.area).collect();
            let rolled: i32 = strike.damage.iter().map(|d| d.roll(&mut self.rng)).sum();
            for i in caught {
                let roll = Roll::d20(&mut self.rng, self.combatants[i].saves[ability as usize], Edge::Plain);
                let success = roll.total() >= difficulty;
                let damage = match (success, half) {
                    (false, _) => rolled,
                    (true, true) => rolled / 2,
                    (true, false) => 0,
                };
                events.push(Event::Saved { who: self.combatants[i].id.clone(), strike: strike.name.clone(), roll, difficulty, success, damage });
                events.extend(self.combatants[i].hurt(damage));
            }
            return events;
        }
        if strike.sure {
            let damage = strike.damage.iter().map(|d| d.roll(&mut self.rng)).sum::<i32>().max(1);
            events.push(Event::Struck { who, target: self.combatants[target].id.clone(), strike: strike.name.clone(), damage });
            events.extend(self.combatants[target].hurt(damage));
            return events;
        }
        let (from, to) = (self.combatants[attacker].at(), self.combatants[target].at());
        let far = distance(from, to);
        let ranged = strike.range > 1;
        let crowded = ranged && self.combatants.iter().any(|c| c.side != self.combatants[attacker].side && c.standing() && distance(c.at(), from) <= 1);
        let helpless = self.combatants[target].hp <= 0;
        let disadvantage = far > strike.range || crowded || self.combatants[target].dodging || (helpless && far > 1);
        let advantage = helpless && far <= 1;
        let roll = Roll::d20(&mut self.rng, strike.bonus, Edge::of(advantage, disadvantage));
        let critical = roll.natural_20() || (helpless && far <= 1 && !roll.natural_1() && roll.total() >= self.combatants[target].ac);
        let hit = !roll.natural_1() && (roll.natural_20() || roll.total() >= self.combatants[target].ac);
        let damage = if hit { strike.damage.iter().map(|d| if critical { d.roll_critical(&mut self.rng) } else { d.roll(&mut self.rng) }).sum::<i32>().max(1) } else { 0 };
        events.push(Event::Attacked { who, target: self.combatants[target].id.clone(), strike: strike.name.clone(), roll, hit, critical, damage });
        events.extend(self.combatants[target].hurt(damage));
        events
    }

    /// Whether the actor could use `strike` on `target` from where it stands.
    pub fn in_range(&self, strike: &Strike, target: usize) -> bool {
        distance(self.actor().at(), self.combatants[target].at()) <= strike.long_range.max(strike.range)
    }

    /// The Attack action: every strike of the actor's routine against `target`, stopping
    /// when it falls. `first` picks which weapon the routine uses.
    pub fn attack(&mut self, target: usize, first: usize) -> Vec<Event> {
        let me = self.actor_index();
        let actor = &self.combatants[me];
        if !actor.action_left || !actor.standing() || first >= actor.strikes.len() || actor.strikes[first].uses_left == Some(0) {
            return Vec::new();
        }
        // A hero swings the chosen weapon as many times as it has attacks, and uses a feat
        // once; a monster's routine names its own strikes.
        let routine: Vec<usize> = match actor.side {
            Side::Party if actor.strikes[first].feat.is_some() => vec![first],
            Side::Party => vec![first; actor.routine.len().max(1)],
            Side::Enemy => actor.routine.clone(),
        };
        let mut events = Vec::new();
        let mut used = false;
        for strike in routine {
            if !self.combatants[target].standing() && self.combatants[target].side == Side::Enemy {
                break;
            }
            if !self.in_range(&self.combatants[me].strikes[strike].clone(), target) {
                continue;
            }
            used = true;
            events.extend(self.resolve(me, strike, target));
        }
        if used {
            self.combatants[me].action_left = false;
            if let Some(left) = &mut self.combatants[me].strikes[first].uses_left {
                *left = left.saturating_sub(1);
            }
        }
        events
    }

    /// Something not in the rules that the game master has judged: the actor spends its
    /// action, and what the master allowed happens without a further roll.
    pub fn improvise(&mut self, name: &str, target: Option<usize>, damage: i32, heal: i32) -> Vec<Event> {
        let me = self.actor_index();
        if !self.combatants[me].action_left || !self.combatants[me].standing() {
            return Vec::new();
        }
        self.combatants[me].action_left = false;
        let who = self.combatants[me].id.clone();
        let mut events = Vec::new();
        if let Some(target) = target.filter(|t| *t < self.combatants.len() && damage > 0) {
            events.push(Event::Struck { who: who.clone(), target: self.combatants[target].id.clone(), strike: name.to_string(), damage });
            events.extend(self.combatants[target].hurt(damage));
        }
        if heal > 0 {
            // Healing goes to the one named when it is a friend, otherwise to the actor.
            let side = self.combatants[me].side;
            let mended = target.filter(|t| *t < self.combatants.len() && self.combatants[*t].side == side && damage <= 0).unwrap_or(me);
            let was_down = self.combatants[mended].hp <= 0;
            self.combatants[mended].mend(heal);
            events.push(Event::Healed { who, target: self.combatants[mended].id.clone(), amount: heal });
            if was_down && self.combatants[mended].standing() {
                events.push(Event::Revived { who: self.combatants[mended].id.clone() });
            }
        }
        events
    }

    /// Uses an action on a strike that is not among the actor's own: a spell.
    pub fn cast(&mut self, strike: &Strike, target: usize) -> Vec<Event> {
        let me = self.actor_index();
        if !self.combatants[me].action_left || !self.combatants[me].standing() || !self.in_range(strike, target) {
            return Vec::new();
        }
        self.combatants[me].action_left = false;
        self.resolve_strike(me, strike, target)
    }

    pub fn dodge(&mut self) -> Vec<Event> {
        self.spend_action(|c| c.dodging = true).then(|| Event::Dodged { who: self.actor().id.clone() }).into_iter().collect()
    }

    /// The actor spends the action on something that mends it: a potion drunk, food eaten.
    pub fn mend_self(&mut self, amount: i32) -> Vec<Event> {
        let who = self.actor().id.clone();
        self.spend_action(|c| c.mend(amount)).then(|| Event::Healed { who: who.clone(), target: who, amount }).into_iter().collect()
    }

    pub fn dash(&mut self) -> Vec<Event> {
        self.spend_action(|c| c.movement_left += c.speed).then(|| Event::Dashed { who: self.actor().id.clone() }).into_iter().collect()
    }

    pub fn disengage(&mut self) -> Vec<Event> {
        self.spend_action(|c| c.disengaged = true).then(|| Event::Disengaged { who: self.actor().id.clone() }).into_iter().collect()
    }

    fn spend_action(&mut self, effect: impl FnOnce(&mut Combatant)) -> bool {
        let me = self.actor_index();
        let c = &mut self.combatants[me];
        if !c.action_left || !c.standing() {
            return false;
        }
        c.action_left = false;
        effect(c);
        true
    }

    /// A monster's turn: close with the nearest hero still standing and attack. A monster
    /// whose best strike is ranged stays where it is when the target is already in range.
    pub fn enemy_turn(&mut self, blocked: &dyn Fn(Cell) -> bool) -> Vec<Event> {
        let me = self.actor_index();
        let mut events = Vec::new();
        let from = self.combatants[me].at();
        let target = (0..self.combatants.len()).filter(|i| self.combatants[*i].side != self.combatants[me].side && self.combatants[*i].standing()).min_by_key(|i| distance(self.combatants[*i].at(), from));
        let Some(target) = target else { return events };
        let Some(&first) = self.combatants[me].routine.first() else { return events };
        let range = self.combatants[me].strikes[first].range;
        if distance(from, self.combatants[target].at()) > range {
            let goal = self.combatants[target].at();
            // The reachable cell closest to the target; among equals, the shortest walk.
            let best = self.reach(blocked).into_iter().min_by_key(|p| (distance(*p.last().unwrap(), goal).max(range), p.len()));
            if let Some(path) = best
                && distance(*path.last().unwrap(), goal) < distance(from, goal)
            {
                events.extend(self.move_to(*path.last().unwrap(), blocked));
            }
        }
        if self.combatants[me].standing() {
            events.extend(self.attack(target, first));
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(_: Cell) -> bool {
        false
    }

    fn fighter() -> Character {
        let skills = vec!["acrobatics".to_string(), "athletics".to_string()];
        Character::create("Brak", "", "human", "fighter", [15, 12, 14, 8, 10, 13], &skills, &[]).unwrap()
    }

    fn goblin(id: &str, at: Cell) -> Combatant {
        Combatant::monster(&srd().monsters["goblin"], id, "Goblin", at)
    }

    /// A fight with the hero placed first in the order, whatever the dice said.
    fn fight(hero_at: Cell, enemies: Vec<Combatant>, seed: u64) -> Encounter {
        let mut all = vec![Combatant::hero(&fighter(), "hero", hero_at)];
        all.extend(enemies);
        let (mut e, _) = Encounter::start(all, Rng::seeded(seed));
        e.order.sort();
        e.turn = 0;
        e.begin_turn();
        e
    }

    #[test]
    fn initiative_orders_everyone_highest_first() {
        let all = vec![Combatant::hero(&fighter(), "hero", (0, 0)), goblin("g1", (5, 0)), goblin("g2", (6, 0))];
        let (e, events) = Encounter::start(all, Rng::seeded(4));
        assert_eq!(e.order.len(), 3);
        assert!(e.order.windows(2).all(|w| e.combatants[w[0]].initiative >= e.combatants[w[1]].initiative));
        assert_eq!(events[0], Event::TurnBegan { who: e.actor().id.clone(), round: 1 });
        assert!(e.actor().action_left && e.actor().movement_left == e.actor().speed);
    }

    #[test]
    fn a_hero_is_built_from_the_sheet() {
        let hero = Combatant::hero(&fighter(), "hero", (2, 3));
        assert_eq!((hero.ac, hero.hp, hero.speed, hero.at()), (18, 12, 6, (2, 3)));
        assert_eq!(hero.strikes[0], Strike { name: "Longsword".into(), bonus: 5, damage: vec![Dice::new(1, 8, 3)], range: 1, long_range: 1, ..Strike::default() });
        assert_eq!((hero.strikes[1].range, hero.strikes[1].long_range), (16, 64));
    }

    #[test]
    fn movement_is_limited_by_speed_walls_and_enemies() {
        let mut e = fight((5, 5), vec![goblin("g", (7, 5))], 1);
        let wall = |c: Cell| c.1 == 4;
        let reach = e.reach(&wall);
        let ends: Vec<Cell> = reach.iter().map(|p| *p.last().unwrap()).collect();
        assert!(ends.contains(&(5, 11)) && !ends.contains(&(5, 12)), "six cells and no further");
        assert!(!ends.iter().any(|c| c.1 <= 4), "not through the wall");
        assert!(!ends.contains(&(7, 5)), "not onto an enemy");
        assert!(ends.contains(&(8, 5)), "but around it");
        assert!(reach.iter().all(|p| p.len() - 1 <= 6));

        assert_eq!(e.move_to((5, 12), &wall), []);
        let events = e.move_to((5, 8), &wall);
        assert_eq!(events, [Event::Moved { who: "hero".into(), path: vec![(5, 5), (5, 6), (5, 7), (5, 8)] }]);
        assert_eq!((e.actor().at(), e.actor().movement_left), ((5, 8), 3));
    }

    #[test]
    fn walking_out_of_reach_draws_a_strike_unless_disengaged() {
        let mut provoked = 0;
        for seed in 0..40 {
            let mut e = fight((5, 5), vec![goblin("g", (6, 5))], seed);
            let events = e.move_to((2, 5), &open);
            if events.iter().any(|ev| matches!(ev, Event::Attacked { who, .. } if who == "g")) {
                provoked += 1;
            }
            assert!(!e.combatants[1].reaction_left);

            let mut careful = fight((5, 5), vec![goblin("g", (6, 5))], seed);
            careful.disengage();
            let events = careful.move_to((2, 5), &open);
            assert_eq!(events.len(), 1, "only the move");
        }
        assert_eq!(provoked, 40);
    }

    #[test]
    fn attacks_hit_about_as_often_as_the_numbers_say_and_only_once_per_action() {
        // +5 against armour class 15: hits on an 10 or better, 55% of the time.
        let mut hits = 0;
        for seed in 0..400 {
            let mut e = fight((5, 5), vec![goblin("g", (6, 5))], seed);
            let events = e.attack(1, 0);
            let Event::Attacked { hit, damage, roll, .. } = &events[0] else { panic!("expected an attack") };
            assert_eq!(*hit, roll.die != 1 && (roll.die == 20 || roll.total() >= 15));
            assert_eq!(*hit, *damage > 0);
            hits += *hit as u32;
            assert!(!e.actor().action_left);
            assert_eq!(e.attack(1, 0), [], "the action is spent");
        }
        assert!((180..260).contains(&hits), "{hits} hits of 400");
    }

    #[test]
    fn an_attack_out_of_range_does_nothing_and_keeps_the_action() {
        let mut e = fight((5, 5), vec![goblin("g", (9, 5))], 2);
        assert_eq!(e.attack(1, 0), []);
        assert!(e.actor().action_left);
        // The crossbow reaches.
        assert_eq!(e.attack(1, 1).len() >= 1, true);
    }

    #[test]
    fn a_critical_hit_rolls_the_dice_twice() {
        let mut biggest = 0;
        for seed in 0..600 {
            let mut e = fight((5, 5), vec![goblin("g", (6, 5))], seed);
            e.combatants[1].hp = 100;
            e.combatants[1].max_hp = 100;
            if let Some(Event::Attacked { critical: true, damage, roll, .. }) = e.attack(1, 0).first() {
                assert_eq!(roll.die, 20);
                // 2d8 + 3.
                assert!((5..=19).contains(damage));
                biggest = biggest.max(*damage);
            }
        }
        assert!(biggest > 11, "a critical can exceed the most a normal hit does");
    }

    #[test]
    fn killing_the_last_enemy_wins_and_pays_its_experience() {
        let mut e = fight((5, 5), vec![goblin("g", (6, 5))], 3);
        e.combatants[1].hp = 1;
        let mut seed = 0;
        while e.outcome() == Outcome::Ongoing {
            e.combatants[0].action_left = true;
            e.rng = Rng::seeded(seed);
            e.attack(1, 0);
            seed += 1;
        }
        assert_eq!(e.outcome(), Outcome::Victory);
        assert!(e.combatants[1].dead);
        assert_eq!(e.xp_won(), 50);
    }

    #[test]
    fn a_monster_closes_in_and_attacks_and_a_bowman_stays_back() {
        let mut e = fight((2, 5), vec![goblin("g", (9, 5))], 5);
        e.end_turn();
        assert_eq!(e.actor().id, "g");
        let events = e.enemy_turn(&open);
        assert!(matches!(&events[0], Event::Moved { who, .. } if who == "g"));
        assert_eq!(distance(e.combatants[1].at(), (2, 5)), 1);
        assert!(events.iter().any(|ev| matches!(ev, Event::Attacked { who, target, .. } if who == "g" && target == "hero")));

        // A goblin whose routine is its bow shoots from where it stands.
        let mut archer = goblin("a", (9, 5));
        archer.routine = vec![1];
        let mut e = fight((2, 5), vec![archer], 5);
        e.end_turn();
        let events = e.enemy_turn(&open);
        assert_eq!(e.combatants[1].at(), (9, 5));
        assert!(matches!(&events[0], Event::Attacked { strike, .. } if strike == "Shortbow"));
    }

    #[test]
    fn a_hero_at_zero_falls_rolls_death_saves_and_can_be_healed() {
        let mut e = fight((5, 5), vec![goblin("g", (6, 5))], 6);
        assert_eq!(e.combatants[0].hurt(13), Some(Event::Fell { who: "hero".into() }));
        assert!(!e.combatants[0].standing() && !e.combatants[0].dead);
        assert_eq!(e.outcome(), Outcome::Defeat, "the only hero is down");

        let heal = Strike { name: "Cure Wounds".into(), damage: vec![Dice::new(1, 8, 2)], range: 1, long_range: 1, heals: true, ..Strike::default() };
        let events = e.resolve_strike(1, &heal, 0);
        assert!(matches!(events[0], Event::Healed { amount, .. } if amount >= 3));
        assert_eq!(events[1], Event::Revived { who: "hero".into() });
        assert!(e.combatants[0].standing());

        // Down again: the goblin acts, then the hero's turn is a death save and nothing else.
        e.combatants[0].hurt(12);
        e.end_turn();
        let events = e.end_turn();
        assert!(events.iter().any(|ev| matches!(ev, Event::DeathSave { who, .. } if who == "hero")));
        let death = e.combatants[0].death;
        assert!(death.successes + death.failures >= 1 || e.combatants[0].standing());
    }

    #[test]
    fn massive_damage_kills_a_hero_outright_and_monsters_die_at_zero() {
        let mut hero = Combatant::hero(&fighter(), "hero", (0, 0));
        assert_eq!(hero.hurt(26), Some(Event::Died { who: "hero".into() }));
        let mut g = goblin("g", (0, 0));
        assert_eq!(g.hurt(7), Some(Event::Died { who: "g".into() }));
        assert_eq!(g.hurt(7), None, "the dead are not hurt again");
    }

    #[test]
    fn an_area_spell_makes_everyone_in_it_save_and_a_success_halves() {
        let mut wizard = Character::create("Zed", "", "human", "wizard", [8, 14, 13, 15, 12, 10], &["arcana".to_string(), "history".to_string()], &[]).unwrap();
        wizard.gain_xp(6_500);
        let fireball = Strike::spell(&wizard, &srd().spells["fireball"], 3).unwrap();
        assert_eq!((fireball.area, fireball.range), (4, 30));
        // Proficiency 3 + Int 16 (+3) + 8.
        assert_eq!(fireball.save, Some((Ability::Dex, 14, true)));
        assert_eq!(fireball.damage, [Dice::new(8, 6, 0)]);

        let all = vec![Combatant::hero(&wizard, "hero", (0, 0)), goblin("near", (10, 5)), goblin("edge", (14, 5)), goblin("far", (15, 5))];
        let (mut e, _) = Encounter::start(all, Rng::seeded(9));
        e.turn = e.order.iter().position(|i| *i == 0).unwrap();
        e.begin_turn();
        let events = e.cast(&fireball, 1);
        let saved: Vec<&str> = events.iter().filter_map(|ev| if let Event::Saved { who, .. } = ev { Some(who.as_str()) } else { None }).collect();
        assert_eq!(saved, ["near", "edge"], "four cells around the target, no more");
        for ev in &events {
            if let Event::Saved { success, damage, roll, difficulty, .. } = ev {
                assert_eq!(*success, roll.total() >= *difficulty);
                assert!(*damage >= 4, "even a successful save takes half of at least eight");
            }
        }
        assert!(!e.actor().action_left);
    }

    #[test]
    fn spells_the_rules_cannot_carry_out_make_no_strike() {
        let wizard = Character::create("Zed", "", "human", "wizard", [8, 14, 13, 15, 12, 10], &["arcana".to_string(), "history".to_string()], &[]).unwrap();
        assert_eq!(Strike::spell(&wizard, &srd().spells["charm-person"], 1), None);
        let missile = Strike::spell(&wizard, &srd().spells["magic-missile"], 1).unwrap();
        assert!(missile.sure && missile.save.is_none());
        let bolt = Strike::spell(&wizard, &srd().spells["fire-bolt"], 0).unwrap();
        assert_eq!((bolt.bonus, bolt.range, bolt.save), (5, 24, None));
        assert_eq!(bolt.damage, [Dice::new(1, 10, 0)]);
    }

    #[test]
    fn dodging_dashing_and_disengaging_each_cost_the_action() {
        let mut e = fight((5, 5), vec![goblin("g", (9, 5))], 2);
        assert_eq!(e.dash(), [Event::Dashed { who: "hero".into() }]);
        assert_eq!(e.actor().movement_left, 12);
        assert_eq!(e.dodge(), [], "one action a turn");
        let mut e = fight((5, 5), vec![goblin("g", (9, 5))], 2);
        assert_eq!(e.dodge(), [Event::Dodged { who: "hero".into() }]);
        assert!(e.actor().dodging);
        e.end_turn();
        e.end_turn();
        assert!(!e.combatants[0].dodging, "dodging lasts until the dodger's next turn");
    }

    #[test]
    fn the_dead_are_skipped_and_rounds_count_up() {
        let mut e = fight((5, 5), vec![goblin("g1", (9, 5)), goblin("g2", (9, 6))], 2);
        e.combatants[1].dead = true;
        e.combatants[1].hp = 0;
        let events = e.end_turn();
        assert_eq!(events, [Event::TurnBegan { who: "g2".into(), round: 1 }]);
        let events = e.end_turn();
        assert_eq!(events, [Event::TurnBegan { who: "hero".into(), round: 2 }]);
    }

    #[test]
    fn a_fight_can_be_saved_in_the_middle() {
        let e = fight((5, 5), vec![goblin("g", (6, 5))], 3);
        let back: Encounter = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn a_feat_is_a_strike_used_once_per_action_and_only_as_often_as_it_has_uses() {
        use crate::character::Feat;
        let skills = vec!["athletics".to_string(), "perception".to_string()];
        let mut hero = Character::create("Brak", "", "human", "fighter", [15, 12, 14, 8, 10, 13], &skills, &[]).unwrap();
        hero.grant(Feat { name: "Thunder shout".into(), dice: "2d6".into(), range: 30, save: Some(Ability::Con), uses: 1, ..Feat::default() }, true);
        hero.grant(Feat { name: "Tough".into(), hp: 2, ..Feat::default() }, true);
        let brak = Combatant::hero(&hero, "hero_0", (2, 2));
        let shout = brak.strikes.iter().position(|s| s.feat == Some(0)).expect("the shout is a strike");
        assert_eq!(brak.strikes.iter().filter(|s| s.feat.is_some()).count(), 1, "a feat of plain numbers is no strike");
        assert_eq!(brak.strikes[shout].uses_left, Some(1));
        let goblin = Combatant::monster(&srd().monsters["goblin"], "goblin", "Goblin", (4, 2));
        let mut fight = Encounter { combatants: vec![brak, goblin], order: vec![0, 1], turn: 0, round: 1, rng: Rng::seeded(4) };
        fight.combatants[0].action_left = true;
        let events = fight.attack(1, shout);
        assert_eq!(events.iter().filter(|e| matches!(e, Event::Saved { .. })).count(), 1, "{events:?}");
        assert_eq!(fight.combatants[0].strikes[shout].uses_left, Some(0));
        fight.combatants[0].action_left = true;
        assert!(fight.attack(1, shout).is_empty(), "no uses left");
    }

    #[test]
    fn an_improvised_action_spends_the_action_and_does_what_the_master_allowed() {
        let skills = vec!["athletics".to_string(), "perception".to_string()];
        let hero = Character::create("Brak", "", "human", "fighter", [15, 12, 14, 8, 10, 13], &skills, &[]).unwrap();
        let brak = Combatant::hero(&hero, "hero_0", (2, 2));
        let goblin = Combatant::monster(&srd().monsters["goblin"], "goblin", "Goblin", (9, 2));
        let mut fight = Encounter { combatants: vec![brak, goblin], order: vec![0, 1], turn: 0, round: 1, rng: Rng::seeded(4) };
        fight.combatants[0].action_left = true;
        fight.combatants[0].hp = 3;
        let events = fight.improvise("Chandelier", Some(1), 4, 2);
        assert!(matches!(&events[0], Event::Struck { target, damage: 4, .. } if target == "goblin"), "{events:?}");
        assert!(events.iter().any(|e| matches!(e, Event::Healed { target, amount: 2, .. } if target == "hero_0")));
        assert_eq!(fight.combatants[0].hp, 5);
        assert!(!fight.combatants[0].action_left);
        assert!(fight.improvise("Again", Some(1), 4, 0).is_empty(), "the action is spent");
    }

}
