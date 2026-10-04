//! A world that answers back: factions that remember, deeds that return, threats that ripen
//! while the party is busy, and what wanders into a place while the party stands in it.

use crate::model::{Attitude, Creature, World};
#[allow(unused_imports)]
use crate::model::Wanderer;
use crate::path::nearest;
use rules::{Rng, srd};
use serde::{Deserialize, Serialize};

/// A people, an order, a gang: someone whose regard for the party is kept.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct Faction {
    pub id: String,
    pub name: String,
    pub description: String,
    /// From -3, sworn enemies, to 3, allies.
    pub standing: i8,
    /// The party has had to do with them: only then is their regard shown.
    pub met: bool,
}

/// A deed that will come back: the master's note to itself.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct Thread {
    pub text: String,
    /// The turn it was noted on.
    pub since: u32,
}

/// Something that ripens whether the party looks or not.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct Threat {
    pub name: String,
    /// What happens when the time is up: the master's own.
    pub what: String,
    /// What the players hear of it, with no hour named.
    pub rumour: String,
    /// The minute of the world's clock it comes due.
    pub due: u32,
    /// It has come due and the master has been told to tell it.
    pub told: bool,
}

/// What came of an hour in a dangerous place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wandered {
    /// Something that is not a fight: words the master wrote with the place.
    Find(String),
    /// Something hostile the master drew with the place has come: it stands there now. The
    /// words are what to tell of it.
    Came(String),
    /// Something hostile is coming that the master never drew: what the place said may come,
    /// and the cell it comes to. The master is asked now to draw it, set it down and tell of
    /// it; nothing appears before.
    Ambush(crate::model::Wanderer, (usize, usize)),
}

/// How often a thread is brought back: every so many turns.
const THREAD_EVERY: u32 = 6;

impl World {
    pub fn faction_mut(&mut self, id: &str) -> Option<&mut Faction> {
        let wanted = id.trim().to_lowercase().replace([' ', '-'], "_");
        self.factions.iter_mut().find(|f| f.id == wanted || f.name.to_lowercase() == id.trim().to_lowercase())
    }

    /// The standing of the faction a creature belongs to, if any.
    pub fn standing_of(&self, creature: &Creature) -> i8 {
        self.factions.iter().find(|f| !creature.faction.is_empty() && f.id == creature.faction).map_or(0, |f| f.standing)
    }

    /// Adds or changes a faction as the master says.
    pub fn set_faction(&mut self, id: &str, name: &str, description: &str, standing: Option<i64>) {
        let id = id.trim().to_lowercase().replace([' ', '-'], "_");
        if id.is_empty() {
            return;
        }
        let room = self.factions.len() < 8;
        match self.factions.iter_mut().find(|f| f.id == id) {
            Some(faction) => {
                if !name.trim().is_empty() {
                    faction.name = name.trim().to_string();
                }
                if !description.trim().is_empty() {
                    faction.description = description.trim().to_string();
                }
                if let Some(standing) = standing {
                    faction.standing = standing.clamp(-3, 3) as i8;
                }
            }
            None if room => self.factions.push(Faction { name: if name.trim().is_empty() { id.replace('_', " ") } else { name.trim().to_string() }, id, description: description.trim().to_string(), standing: standing.unwrap_or(0).clamp(-3, 3) as i8, met: false }),
            None => {}
        }
    }

    /// The party's deeds move a faction's regard; it has met them now.
    pub fn change_standing(&mut self, id: &str, change: i64) -> bool {
        match self.faction_mut(id) {
            Some(faction) => {
                faction.standing = (faction.standing as i64 + change).clamp(-3, 3) as i8;
                faction.met = true;
                true
            }
            None => false,
        }
    }

    /// The deed the master must bring back in this telling, when it is time: every few
    /// turns, the oldest one first.
    pub fn thread_due(&self) -> Option<&Thread> {
        let oldest = self.threads.first()?;
        (self.turns > 0 && self.turns % THREAD_EVERY == 0 && self.turns > oldest.since + 2).then_some(oldest)
    }

    /// Threats whose time is up and which have not been told yet.
    pub fn threats_due(&self) -> Vec<&Threat> {
        self.threats.iter().filter(|t| !t.told && self.clock >= t.due).collect()
    }

    /// What wanders into the place the party stands in, rolled once for an hour gone by.
    /// Nothing where the place is safe, where the master already set something hostile, or
    /// where it wrote nothing that could come.
    pub fn wander(&mut self, rng: &mut Rng) -> Option<Wandered> {
        if self.fight.is_some() || self.party.is_empty() {
            return None;
        }
        let night = self.is_night();
        let here = self.current.clone();
        let (px, py) = self.player.cell();
        let place = self.locations.get(&here)?;
        if place.danger == 0 || place.creatures.iter().any(|c| c.attitude == Attitude::Hostile) {
            return None;
        }
        let chance = place.danger.min(5) as u32 * if night { 10 } else { 5 };
        if rng.die(100) > chance {
            return None;
        }
        // One in three is not a fight, while the master's lines last.
        let comers: Vec<crate::model::Wanderer> = place.wanderers.clone();
        if !place.finds.is_empty() && (comers.is_empty() || rng.die(3) == 1) {
            let place = self.locations.get_mut(&here)?;
            return Some(Wandered::Find(place.finds.remove(0)));
        }
        let comer = comers.get((rng.die(comers.len().max(1) as u32) - 1) as usize)?.clone();
        let index = comer.monster.clone();
        let monster = srd().monsters.get(&index)?;
        // It has crept up on the party: a few cells off, on the side with the most room, so
        // that the fight is on at once. Hostile things do not walk of themselves.
        let (w, h) = place.size();
        let _board = crate::path::board((w, h));
        let reach = 3i32;
        let sides = [(reach, 0), (-reach, 0), (0, reach), (0, -reach)];
        let inside = |(dx, dy): &(i32, i32)| (1..w as i32 - 1).contains(&(px + dx)) && (1..h as i32 - 1).contains(&(py + dy));
        let (dx, dy) = sides.into_iter().filter(inside).max_by_key(|(dx, dy)| ((px + dx * 3).clamp(0, w as i32) - px).abs() + ((py + dy * 3).clamp(0, h as i32) - py).abs()).unwrap_or((reach, 0));
        let from = ((px + dx).clamp(1, w as i32 - 2) as usize, (py + dy).clamp(1, h as i32 - 2) as usize);
        let free = |(x, y): (usize, usize)| !place.blocked(x as i32, y as i32, &self.tiles) && place.exit_at(x, y).is_none() && (x as i32, y as i32) != (px, py);
        let (x, y) = nearest(from, free)?;
        // Never drawn: the master is asked for it now.
        if comer.sprite.is_empty() || !self.sprites.contains_key(&comer.sprite) {
            return Some(Wandered::Ambush(comer, (x, y)));
        }
        let mut id = format!("wandering_{index}").replace('-', "_");
        let mut n = 2;
        while place.creatures.iter().any(|c| c.id == id) || place.props.iter().any(|p| p.id == id) {
            id = format!("wandering_{}_{n}", index.replace('-', "_"));
            n += 1;
        }
        let russian = self.language.to_lowercase().starts_with("ru") || self.language.contains("усск");
        let name = if comer.name.is_empty() { rules::tr(russian, &monster.name).to_string() } else { comer.name.clone() };
        let told = match (comer.description.is_empty(), russian) {
            (false, _) => format!("{name}. {}", comer.description),
            (true, true) => format!("{name} — совсем рядом."),
            (true, false) => format!("{name} is upon you."),
        };
        let creature = Creature {
            id,
            name,
            description: comer.description.clone(),
            x,
            y,
            sprite: comer.sprite.clone(),
            attitude: Attitude::Hostile,
            actions: Vec::new(),
            monster: Some(index),
            stats: None,
            count: 0,
            mustered: false,
            stock: Vec::new(),
            wares: Vec::new(),
            shop_day: 0,
            haggle: 0,
            haggled: false,
            faction: String::new(),
            hurt: 0,
        };
        self.locations.get_mut(&here)?.creatures.push(creature);
        self.story.say(crate::story::Speaker::Master, &told);
        Some(Wandered::Came(told))
    }
}
