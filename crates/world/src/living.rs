//! A world that answers back: factions that remember, deeds that return, threats that ripen
//! while the party is busy, and what wanders into a place while the party stands in it.

use crate::model::{Attitude, Creature, World};
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
    /// A pack came in; the id of the one that stands for it.
    Ambush(String),
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
        if !place.finds.is_empty() && (place.wanderers.is_empty() || rng.die(3) == 1) {
            let place = self.locations.get_mut(&here)?;
            return Some(Wandered::Find(place.finds.remove(0)));
        }
        let index = place.wanderers.get((rng.die(place.wanderers.len().max(1) as u32) - 1) as usize)?.clone();
        let monster = srd().monsters.get(&index)?;
        // It comes from as far off as the place allows: the corner furthest from the party.
        let corners = [(1usize, 1usize), (crate::MAP_W - 2, 1), (1, crate::MAP_H - 2), (crate::MAP_W - 2, crate::MAP_H - 2)];
        let far = corners.into_iter().max_by_key(|(x, y)| (*x as i32 - px).abs() + (*y as i32 - py).abs())?;
        let free = |(x, y): (usize, usize)| !place.blocked(x as i32, y as i32, &self.tiles) && place.exit_at(x, y).is_none();
        let (x, y) = nearest(far, free)?;
        let mut id = format!("wandering_{index}").replace('-', "_");
        let mut n = 2;
        while place.creatures.iter().any(|c| c.id == id) || place.props.iter().any(|p| p.id == id) {
            id = format!("wandering_{}_{n}", index.replace('-', "_"));
            n += 1;
        }
        // Drawn as something of its kind already seen here, or as the marker.
        let sprite = self.sprites.contains_key(&index).then(|| index.clone()).unwrap_or_else(|| crate::base::MARKER_SPRITE.to_string());
        let comer = Creature {
            id: id.clone(),
            name: rules::tr(self.language.to_lowercase().starts_with("ru") || self.language.contains("усск"), &monster.name).to_string(),
            description: String::new(),
            x,
            y,
            sprite,
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
        };
        self.locations.get_mut(&here)?.creatures.push(comer);
        Some(Wandered::Ambush(id))
    }
}
