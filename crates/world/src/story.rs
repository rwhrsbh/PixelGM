//! What happens in a world after it is drawn: actions with outcomes the game master wrote
//! ahead of time, the effects those outcomes have, and the record of what has been said.

use crate::base::MARKER_SPRITE;
use crate::build::slug;
use crate::model::{Attitude, Creature, Exit, Location, MAP_H, MAP_W, Player, Prop, Stub, World};
use crate::party::{Branch, PendingCheck, Stats};
use rules::{Ability, Rng};
use crate::path::nearest;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Dice may be written as "2d6" or as a bare number.
pub(crate) fn lenient_text<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => s,
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    })
}

fn nowhere() -> i64 {
    -1
}

fn secret_dc() -> i64 {
    13
}

pub(crate) fn lenient_int<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Number(n) => n.as_f64().map_or(0, |f| f.round() as i64),
        Value::String(s) => s.trim().parse::<f64>().map_or(0, |f| f.round() as i64),
        _ => 0,
    })
}

/// One change to the world. The game master writes these; the game checks and applies them.
/// In an action that belongs to a prop or creature, the id `self` means that thing.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Effect {
    /// A prop or creature leaves the scene.
    Remove { id: String },
    /// The party gains an item.
    Give {
        name: String,
        #[serde(default)]
        description: String,
        /// The hero who gets it; the one who acted when empty.
        #[serde(default)]
        to: String,
        #[serde(flatten)]
        traits: ItemTraits,
    },
    /// The party loses an item: from the hero named, or whoever has it.
    Take {
        name: String,
        #[serde(default)]
        from: String,
    },
    /// A thing looks different from now on.
    Describe { id: String, description: String },
    Attitude { id: String, attitude: Attitude },
    Move {
        id: String,
        #[serde(deserialize_with = "lenient_int")]
        x: i64,
        #[serde(deserialize_with = "lenient_int")]
        y: i64,
    },
    /// One cell of ground changes.
    Tile {
        #[serde(deserialize_with = "lenient_int")]
        x: i64,
        #[serde(deserialize_with = "lenient_int")]
        y: i64,
        tile: String,
        /// The far corner, when a whole line or block of cells changes: [x, y].
        #[serde(default)]
        to: Vec<i64>,
    },
    /// The main quest moves: a new goal for the players, the next act, or its end.
    Quest {
        #[serde(default)]
        goal: String,
        #[serde(default)]
        advance: bool,
        /// "won" or "lost", when it is over.
        #[serde(default)]
        done: String,
    },
    /// A new main quest, when there is none or the last is over.
    QuestNew {
        #[serde(flatten)]
        quest: crate::build::QuestDraft,
    },
    /// Something asked of the party on the side: new, moved on, or done.
    Task {
        name: String,
        #[serde(default)]
        step: String,
        #[serde(default)]
        done: bool,
    },
    /// The ground or the whole scene shakes, for so many seconds.
    Shake {
        #[serde(default)]
        seconds: Value,
    },
    /// Something new appears. `kind` is `prop` or `creature`.
    Spawn {
        kind: String,
        id: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        description: String,
        #[serde(deserialize_with = "lenient_int")]
        x: i64,
        #[serde(deserialize_with = "lenient_int")]
        y: i64,
        #[serde(default)]
        sprite: String,
        #[serde(default)]
        blocking: bool,
        #[serde(default)]
        attitude: Attitude,
        /// For a creature: the SRD monster it fights as, or numbers of its own.
        #[serde(default)]
        monster: Option<String>,
        #[serde(default)]
        stats: Option<Stats>,
        /// For a creature: how many of them appear; 0 leaves it to the game.
        #[serde(default, deserialize_with = "lenient_int")]
        count: i64,
        /// For a prop that holds things: what is in it, how hard its lock is, its key.
        #[serde(default)]
        contents: Vec<ThingDraft>,
        #[serde(default, deserialize_with = "lenient_int")]
        lock: i64,
        #[serde(default)]
        key: String,
    },
    /// Something hidden here: found by a sharp eye nearby, or a trap found underfoot.
    Secret {
        #[serde(deserialize_with = "lenient_int")]
        x: i64,
        #[serde(deserialize_with = "lenient_int")]
        y: i64,
        #[serde(default = "secret_dc", deserialize_with = "lenient_int")]
        dc: i64,
        #[serde(default)]
        kind: String,
        #[serde(default)]
        note: String,
        #[serde(default)]
        effects: Vec<Value>,
    },
    /// A way out opens.
    Exit {
        #[serde(deserialize_with = "lenient_int")]
        x: i64,
        #[serde(deserialize_with = "lenient_int")]
        y: i64,
        to: String,
        #[serde(default)]
        label: String,
    },
    /// Something worth remembering for the rest of the campaign.
    Fact { text: String },
    /// The hour changes here: day, dusk, night or dark.
    Light { light: String },
    /// The sky changes here: clear, sun, rain, storm, snow or fog.
    Weather { weather: String },
    /// The music changes here, to a mood.
    Music { mood: String },
    /// Something is left on the ground of a cell: blood, a stain, a scorch, a puddle, a heap.
    /// With no cell it is where the party stands; with an id, under that thing.
    Mark {
        #[serde(default)]
        id: String,
        #[serde(default = "nowhere", deserialize_with = "lenient_int")]
        x: i64,
        #[serde(default = "nowhere", deserialize_with = "lenient_int")]
        y: i64,
        #[serde(default)]
        kind: String,
        #[serde(default)]
        color: String,
    },
    /// The party goes to another place; a new one is drawn when reached.
    Travel {
        to: String,
        #[serde(default)]
        name: String,
        /// The heroes who go, by name; everyone here when empty.
        #[serde(default)]
        who: Vec<String>,
        /// What a new place looks like, for the drawing of it.
        #[serde(default)]
        description: String,
    },
    /// What was left on a cell is taken up or washed away; with no cell, where the party stands.
    Clean {
        #[serde(default = "nowhere", deserialize_with = "lenient_int")]
        x: i64,
        #[serde(default = "nowhere", deserialize_with = "lenient_int")]
        y: i64,
    },
    /// Someone moves their body: jumps, crouches, strikes, falls, dances. Shown to everyone.
    Animate {
        #[serde(default)]
        id: String,
        motion: String,
    },
    /// The outcome is uncertain: the party rolls, and one of two prepared branches follows.
    Check {
        ability: String,
        #[serde(default)]
        skill: Option<String>,
        #[serde(deserialize_with = "lenient_int")]
        dc: i64,
        #[serde(default)]
        success: Branch,
        #[serde(default)]
        failure: Branch,
    },
    /// The party is hurt outside a fight. `amount` is dice or a number.
    Damage {
        #[serde(deserialize_with = "lenient_text")]
        amount: String,
    },
    Heal {
        #[serde(deserialize_with = "lenient_text")]
        amount: String,
    },
    /// Experience for something achieved in the story.
    Xp {
        #[serde(deserialize_with = "lenient_int")]
        amount: i64,
    },
    /// The hostile creatures here attack.
    Fight,
    /// A hero did something out of the ordinary. The game keeps count and decides whether
    /// a secret skill comes of it.
    Deed {
        #[serde(default)]
        hero: String,
    },
    /// The story brings a dead hero back.
    Revive {
        #[serde(default)]
        hero: String,
    },
}

/// Something the players can do with a thing without asking the game master: the outcome
/// was written when the thing was.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Action {
    pub label: String,
    pub outcome: String,
    pub effects: Vec<Effect>,
    /// The thing the party must stand beside to do it, if any.
    #[serde(default)]
    pub near: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct ActionDraft {
    pub label: String,
    pub outcome: String,
    pub effects: Vec<Value>,
    pub near: String,
}

const MAX_ACTIONS: usize = 5;
const MAX_LABEL: usize = 140;

/// Reads effects one by one, so a single malformed effect does not cost the rest.
pub fn read_effects(raw: &[Value], notes: &mut Vec<String>) -> Vec<Effect> {
    raw.iter()
        .filter_map(|value| match serde_json::from_value::<Effect>(value.clone()) {
            Ok(effect) => Some(effect),
            Err(e) => {
                notes.push(format!("an effect could not be read and was dropped: {e}"));
                None
            }
        })
        .collect()
}

pub fn read_actions(drafts: &[ActionDraft], notes: &mut Vec<String>) -> Vec<Action> {
    drafts
        .iter()
        .filter(|d| !d.label.trim().is_empty() && !d.outcome.trim().is_empty())
        .take(MAX_ACTIONS)
        .map(|d| Action {
            label: d.label.trim().chars().take(MAX_LABEL).collect(),
            outcome: d.outcome.trim().to_string(),
            effects: read_effects(&d.effects, notes),
            near: Some(slug(&d.near)).filter(|id| !id.is_empty()),
        })
        .collect()
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
pub struct Item {
    pub name: String,
    pub description: String,
    pub count: u32,
    /// The weapon, armour or shield of the rules this is, so that a hero can put it on.
    #[serde(default)]
    pub gear: Option<String>,
    /// The cell of the bag its top left corner lies in; none when the bag had no room.
    #[serde(default)]
    pub at: Option<(u8, u8)>,
    /// What sort of thing it is: weapon, armor, shield, light, potion, food, throwable, key,
    /// tool, junk, quest… Empty when the name says enough.
    #[serde(default)]
    pub kind: String,
    /// Cells across and down, when the master gave them.
    #[serde(default)]
    pub size_cells: Option<(u8, u8)>,
    /// The master's own picture of it, rows of palette indices.
    #[serde(default)]
    pub rows: Vec<String>,
    /// Dice of healing when used up, dice of harm when thrown, cells of light when lit.
    #[serde(default)]
    pub heal: String,
    #[serde(default)]
    pub damage: String,
    #[serde(default)]
    pub light: u8,
}

/// One hero's bag: the grid of things, and the light in the off hand.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct Bag {
    pub inventory: Vec<Item>,
    pub held: Option<Item>,
    /// Things of the master's own that the hero wears or wields now: taken off, each comes
    /// back into the bag as itself, with its name and picture.
    pub worn_as: Vec<Item>,
    /// The names of the things kept at hand, a key each; empty for a free slot.
    pub quick: Vec<String>,
}

/// What the master may say about a thing it gives, beyond its name.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct ItemTraits {
    pub kind: String,
    /// [cells across, cells down], or "1x3".
    pub size: Value,
    pub rows: Vec<String>,
    pub heal: String,
    pub damage: String,
    pub light: Value,
    /// The weapon or armour of the rules this works as, for a thing to wield or wear.
    pub gear: String,
}

impl ItemTraits {
    fn size(&self) -> Option<(u8, u8)> {
        let pair = match &self.size {
            Value::Array(v) if v.len() >= 2 => Some((v[0].as_f64()?, v[1].as_f64()?)),
            Value::String(s) => {
                let (a, b) = s.split_once(['x', 'х', '*', ' '])?;
                Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
            }
            _ => None,
        }?;
        Some((pair.0.round().clamp(1.0, 4.0) as u8, pair.1.round().clamp(1.0, 4.0) as u8))
    }

    fn light(&self) -> u8 {
        match &self.light {
            Value::Number(n) => n.as_f64().unwrap_or(0.0).round().clamp(0.0, 8.0) as u8,
            Value::Bool(true) => 4,
            Value::String(s) => s.trim().parse::<f64>().map_or(0, |n| n.round().clamp(0.0, 8.0) as u8),
            _ => 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Speaker {
    Master,
    Player,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Line {
    pub speaker: Speaker,
    pub text: String,
}

/// Everything the game master needs to remember, apart from the map: what the party carries,
/// what was said lately, and what was said long ago in shorter words.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Story {
    /// What the party carried before each hero had a bag of their own; moved into the first
    /// hero's bag when a save from then is opened.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub inventory: Vec<Item>,
    /// Each hero's bag, by the hero's place in the party.
    #[serde(default)]
    pub bags: Vec<Bag>,

    /// The story since the last chapter was written, word for word.
    pub log: Vec<Line>,
    /// Older story, one detailed summary per chapter.
    pub chapters: Vec<String>,
    /// Facts that must never be forgotten; only ever added to.
    pub facts: Vec<String>,
}

const MAX_FACTS: usize = 300;
/// Lines of the newest story kept word for word when a chapter is written, so the master
/// does not lose the thread of the scene in progress.
pub const KEPT_LINES: usize = 6;

impl Story {
    pub fn say(&mut self, speaker: Speaker, text: &str) {
        let text = text.trim();
        if !text.is_empty() {
            self.log.push(Line { speaker, text: text.to_string() });
        }
    }

    /// A rough count of the tokens the word-for-word story takes in a request. Counting
    /// exactly would cost a request; three characters to a token is close for mixed text and
    /// errs on the side of compressing early.
    pub fn log_tokens(&self) -> usize {
        self.log.iter().map(|l| l.text.chars().count() + 8).sum::<usize>() / 3
    }

    /// Whether the story has grown enough that the older part should be written up.
    pub fn needs_chapter(&self, budget_tokens: usize) -> bool {
        self.log.len() > KEPT_LINES && self.log_tokens() * 5 >= budget_tokens * 4
    }

    /// Replaces all but the newest lines with a chapter the master wrote about them.
    pub fn close_chapter(&mut self, chapter: &str, facts: &[String]) {
        if !chapter.trim().is_empty() {
            self.chapters.push(chapter.trim().to_string());
            let keep_from = self.log.len().saturating_sub(KEPT_LINES);
            self.log.drain(..keep_from);
        }
        for fact in facts {
            self.remember(fact);
        }
    }

    pub fn remember(&mut self, fact: &str) {
        let fact = fact.trim();
        if !fact.is_empty() && self.facts.len() < MAX_FACTS && !self.facts.iter().any(|f| f.eq_ignore_ascii_case(fact)) {
            self.facts.push(fact.to_string());
        }
    }

}

/// A thing as the master wrote it: a name, a description and what else it said.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct ThingDraft {
    pub name: String,
    pub description: String,
    #[serde(flatten)]
    pub traits: ItemTraits,
}

impl ThingDraft {
    pub fn item(&self) -> Option<Item> {
        let name = self.name.trim();
        if name.is_empty() {
            return None;
        }
        let traits = &self.traits;
        Some(Item {
            name: name.to_string(),
            description: self.description.trim().to_string(),
            count: 1,
            gear: crate::bag::gear_like(name, &traits.kind, &traits.gear),
            at: None,
            kind: traits.kind.trim().to_lowercase(),
            size_cells: traits.size(),
            rows: traits.rows.clone(),
            heal: traits.heal.trim().to_string(),
            damage: traits.damage.trim().to_string(),
            light: traits.light(),
        })
    }
}

impl World {
    /// The chance, in hundredths, that the best of the party passes the check a choice
    /// begins with; nothing for a choice that needs no die.
    pub fn chance(&self, action: &Action) -> Option<u8> {
        let (ability, skill, dc) = action.effects.iter().find_map(|e| match e {
            Effect::Check { ability, skill, dc, .. } => Some((Ability::from_index(ability.trim())?, skill.as_ref().map(|s| s.trim().to_lowercase().replace([' ', '_'], "-")).filter(|s| !s.is_empty()), (*dc).clamp(1, 40) as i32)),
            _ => None,
        })?;
        let bonus = self.party.iter().enumerate().filter(|(i, h)| !h.is_down() && self.takes_part(*i)).map(|(_, h)| h.check_bonus(ability, skill.as_deref())).max().unwrap_or(0);
        // A d20 plus the bonus must reach the difficulty.
        Some((((21 - (dc - bonus)).clamp(0, 20)) * 5) as u8)
    }
}

impl Bag {
    pub fn give_with(&mut self, name: &str, description: &str, traits: &ItemTraits) {
        let draft = ThingDraft { name: name.to_string(), description: description.to_string(), traits: traits.clone() };
        if let Some(item) = draft.item() {
            self.receive(item);
        }
    }

    /// A thing into the bag: onto a heap of the same name, or a place of its own. Gear lies
    /// apart, one piece to a place.
    pub fn receive(&mut self, item: Item) {
        match self.inventory.iter_mut().find(|i| item.gear.is_none() && i.gear.is_none() && i.name.to_lowercase() == item.name.to_lowercase()) {
            Some(heap) => heap.count += item.count.max(1),
            None => {
                self.inventory.push(Item { at: None, count: item.count.max(1), ..item });
                self.pack();
            }
        }
    }

    pub fn take(&mut self, name: &str) -> bool {
        let wanted = name.trim().to_lowercase();
        let Some(at) = self.inventory.iter().position(|i| i.name.to_lowercase() == wanted) else { return false };
        if self.inventory[at].count > 1 {
            self.inventory[at].count -= 1;
        } else {
            self.inventory.remove(at);
        }
        true
    }
}

fn clamp_cell(x: i64, y: i64) -> (usize, usize) {
    (x.clamp(0, MAP_W as i64 - 1) as usize, y.clamp(0, MAP_H as i64 - 1) as usize)
}

/// Characters a location may use for tiles added after it was drawn.
const SPARE_SYMBOLS: &str = "!$^()[]{}<>?/|`'\"-_";

impl Location {
    /// Whether the party has seen a cell of this place.
    pub fn was_seen(&self, x: usize, y: usize) -> bool {
        self.seen.get(y).is_some_and(|row| row & (1 << x) != 0)
    }

    /// Marks the cells within `reach` cells of a point as seen. True when any was new.
    pub fn reveal(&mut self, cx: f32, cy: f32, reach: f32) -> bool {
        self.seen.resize(MAP_H, 0);
        let mut new = false;
        for y in 0..MAP_H {
            for x in 0..MAP_W {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                if dx * dx + dy * dy <= reach * reach && self.seen[y] & (1 << x) == 0 {
                    self.seen[y] |= 1 << x;
                    new = true;
                }
            }
        }
        new
    }

    fn has_thing(&self, id: &str) -> bool {
        self.props.iter().any(|p| p.id == id) || self.creatures.iter().any(|c| c.id == id)
    }

    /// Changes the tile of one cell, giving the tile a map symbol if it has none yet.
    fn set_tile(&mut self, x: usize, y: usize, tile: &str) -> bool {
        let symbol = match self.legend.iter().find(|(_, id)| id.as_str() == tile) {
            Some((symbol, _)) => *symbol,
            None => {
                let Some(free) = SPARE_SYMBOLS.chars().find(|c| !self.legend.contains_key(c)) else { return false };
                self.legend.insert(free, tile.to_string());
                free
            }
        };
        let mut row: Vec<char> = self.map[y].chars().collect();
        row[x] = symbol;
        self.map[y] = row.into_iter().collect();
        true
    }
}

impl World {
    /// Applies the master's effects to the place the party is in. `me` is the thing whose
    /// action is being carried out, for the id `self`. What could not be done is noted and
    /// skipped: an effect naming something that is not there must not stop the rest.
    pub fn apply(&mut self, effects: &[Effect], me: Option<&str>) -> Vec<String> {
        let mut notes = Vec::new();
        let here = self.current.clone();
        let resolve = |id: &str| if id == "self" { me.unwrap_or("self").to_string() } else { slug(id) };
        self.ensure_bags();
        // Whose bag a thing goes into: the hero named, or the one who acted.
        let names: Vec<String> = self.party.iter().map(|h| slug(&h.name)).collect();
        let acting = self.acting.filter(|a| *a < self.party.len()).unwrap_or(0);
        let bag_of = |to: &str| names.iter().position(|n| !to.trim().is_empty() && *n == slug(to)).unwrap_or(acting);
        let mut moved: Vec<(usize, (usize, usize))> = Vec::new();
        let mut travel: Option<(String, String, Vec<usize>)> = None;
        let mut travel_look = String::new();
        // The heroes in this scene: those the word "party" means here.
        let present: Vec<usize> = (0..self.party.len()).filter(|i| !self.party[*i].dead && self.takes_part(*i) && self.hero_place(*i) == here).collect();
        let present = if present.is_empty() { (0..self.party.len().min(1)).collect() } else { present };
        let heroes_named = |id: &str| -> Vec<usize> {
            match id {
                "party" | "hero" | "heroes" | "player" | "players" | "everyone" | "all" => present.clone(),
                _ => names.iter().position(|n| *n == id).into_iter().collect(),
            }
        };
        // Where the party stands, for what is left on the ground with no cell given.
        let party_cell = {
            let (x, y) = self.player.cell();
            clamp_cell(x as i64, y as i64)
        };
        for (n, effect) in effects.iter().enumerate() {
            // Effects on the party itself come first: they need the whole world, not just
            // the place.
            match effect {
                Effect::Check { ability, skill, dc, success, failure } => {
                    let Some(ability) = Ability::from_index(ability.trim()) else {
                        notes.push(format!("check: there is no ability '{ability}'"));
                        continue;
                    };
                    let skill = skill.as_ref().map(|s| s.trim().to_lowercase().replace([' ', '_'], "-")).filter(|s| !s.is_empty());
                    // Everything after a check waits for the die.
                    self.pending_place = self.current.clone();
                    self.pending = Some(PendingCheck { ability, skill, difficulty: (*dc).clamp(1, 40) as i32, success: success.clone(), failure: failure.clone(), me: me.map(str::to_string), rest: effects[n + 1..].to_vec() });
                    break;
                }
                Effect::Damage { amount } => {
                    self.hurt_party(amount, &mut Rng::from_clock());
                    continue;
                }
                Effect::Heal { amount } => {
                    self.heal_party(amount, &mut Rng::from_clock());
                    continue;
                }
                Effect::Xp { amount } => {
                    let ups = self.award_story_xp(*amount);
                    self.level_ups.extend(ups);
                    continue;
                }
                Effect::Fight => {
                    if !self.start_fight(Rng::from_clock()) {
                        notes.push("fight: nothing hostile is here".to_string());
                    }
                    continue;
                }
                Effect::Deed { hero } => {
                    self.deed(hero, &mut Rng::from_clock());
                    continue;
                }
                Effect::Revive { hero } => {
                    if !self.revive(hero) {
                        notes.push(format!("revive: nobody dead is called '{hero}'"));
                    }
                    continue;
                }
                _ => {}
            }
            let tiles = &self.tiles;
            let Some(location) = self.locations.get_mut(&here) else { break };
            match effect {
                Effect::Remove { id } => {
                    let id = resolve(id);
                    if !location.has_thing(&id) {
                        notes.push(format!("remove: nothing called '{id}' is here"));
                    }
                    location.props.retain(|p| p.id != id);
                    location.creatures.retain(|c| c.id != id);
                }
                Effect::Give { name, description, to, traits } => {
                    let b = bag_of(to);
                    if let Some(bag) = self.story.bags.get_mut(b) {
                        bag.give_with(name, description, traits);
                    }
                }
                Effect::Take { name, from } => {
                    // The named hero's bag first, then the one who acted, then anyone's.
                    let first = bag_of(from);
                    let order: Vec<usize> = std::iter::once(first).chain(0..self.story.bags.len()).collect();
                    if !order.into_iter().any(|b| self.story.bags.get_mut(b).is_some_and(|bag| bag.take(name))) {
                        notes.push(format!("take: the party has no '{name}'"));
                    }
                }
                Effect::Describe { id, description } => {
                    let id = resolve(id);
                    if let Some(p) = location.props.iter_mut().find(|p| p.id == id) {
                        p.description = description.trim().to_string();
                    } else if let Some(c) = location.creatures.iter_mut().find(|c| c.id == id) {
                        c.description = description.trim().to_string();
                    } else {
                        notes.push(format!("describe: nothing called '{id}' is here"));
                    }
                }
                Effect::Attitude { id, attitude } => {
                    let id = resolve(id);
                    match location.creatures.iter_mut().find(|c| c.id == id) {
                        Some(c) => c.attitude = *attitude,
                        None => notes.push(format!("attitude: no creature called '{id}' is here")),
                    }
                }
                Effect::Move { id, x, y } => {
                    let id = resolve(id);
                    let wanted = clamp_cell(*x, *y);
                    if let Some(p) = location.props.iter_mut().find(|p| p.id == id) {
                        (p.x, p.y) = wanted;
                    } else if location.creatures.iter().any(|c| c.id == id) {
                        // The creature's own cell does not count as taken while it moves.
                        let mut without = location.clone();
                        without.creatures.retain(|c| c.id != id);
                        let free = |(cx, cy): (usize, usize)| !without.blocked(cx as i32, cy as i32, tiles) && without.exit_at(cx, cy).is_none();
                        if let (Some(cell), Some(c)) = (nearest(wanted, free), location.creatures.iter_mut().find(|c| c.id == id)) {
                            (c.x, c.y) = cell;
                        }
                    } else if !heroes_named(&id).is_empty() {
                        let heroes = heroes_named(&id);
                        // Onto a way out, those heroes take it.
                        if let Some(exit) = location.exit_at(wanted.0, wanted.1) {
                            travel = Some((exit.to.clone(), String::new(), heroes.clone()));
                        }
                        // They are carried, pushed or walk where the story says.
                        let free = |(cx, cy): (usize, usize)| !location.blocked(cx as i32, cy as i32, tiles) && location.exit_at(cx, cy).is_none();
                        if let Some(cell) = nearest(wanted, free) {
                            moved.extend(heroes.into_iter().map(|h| (h, cell)));
                        }
                    } else {
                        notes.push(format!("move: nothing called '{id}' is here"));
                    }
                }
                Effect::Tile { x, y, tile, to } => {
                    let tile = slug(tile);
                    let (x, y) = clamp_cell(*x, *y);
                    let (x2, y2) = match to.as_slice() {
                        [x2, y2, ..] => clamp_cell(*x2, *y2),
                        _ => (x, y),
                    };
                    if !tiles.contains_key(&tile) {
                        notes.push(format!("tile: there is no tile '{tile}'"));
                    } else {
                        // Every cell from one corner to the other: a bridge, a wall, a flood.
                        'cells: for cy in y.min(y2)..=y.max(y2) {
                            for cx in x.min(x2)..=x.max(x2) {
                                if !location.set_tile(cx, cy, &tile) {
                                    notes.push("tile: the place cannot hold another kind of tile".to_string());
                                    break 'cells;
                                }
                            }
                        }
                    }
                }
                Effect::Secret { x, y, dc, kind, note, effects } => {
                    let (sx, sy) = clamp_cell(*x, *y);
                    let mut read_notes = Vec::new();
                    let effects = read_effects(effects, &mut read_notes);
                    notes.extend(read_notes);
                    location.secrets.push(crate::model::Secret { x: sx, y: sy, dc: (*dc).clamp(5, 30) as i32, kind: kind.trim().to_lowercase(), note: note.trim().to_string(), effects, found: false });
                }
                Effect::Spawn { kind, id, name, description, x, y, sprite, blocking, attitude, monster, stats, count, contents, lock, key } => {
                    let mut id = slug(id);
                    if id.is_empty() {
                        id = "thing".to_string();
                    }
                    let base = id.clone();
                    let mut n = 2;
                    while location.has_thing(&id) {
                        id = format!("{base}_{n}");
                        n += 1;
                    }
                    let mut sprite = slug(sprite);
                    if !self.sprites.contains_key(&sprite) {
                        notes.push(format!("spawn: sprite '{sprite}' was not drawn"));
                        sprite = MARKER_SPRITE.to_string();
                    }
                    let wanted = clamp_cell(*x, *y);
                    let name = if name.trim().is_empty() { id.clone() } else { name.trim().to_string() };
                    if kind.trim().eq_ignore_ascii_case("creature") {
                        let free = |(cx, cy): (usize, usize)| !location.blocked(cx as i32, cy as i32, tiles) && location.exit_at(cx, cy).is_none();
                        if let Some((cx, cy)) = nearest(wanted, free) {
                            location.creatures.push(Creature { id, name, description: description.trim().to_string(), x: cx, y: cy, sprite, attitude: *attitude, actions: Vec::new(), monster: monster.as_deref().map(|m| m.trim().to_lowercase().replace([' ', '_'], "-")).filter(|m| !m.is_empty()), stats: stats.clone(), count: (*count).clamp(0, 8) as u8, mustered: false });
                        }
                    } else {
                        let chest = (!contents.is_empty() || *lock > 0 || !key.trim().is_empty()).then(|| crate::model::Chest { items: contents.iter().filter_map(ThingDraft::item).collect(), lock: (*lock).clamp(0, 30) as i32, key: key.trim().to_string(), open: false });
                        location.props.push(Prop { id, name, description: description.trim().to_string(), x: wanted.0, y: wanted.1, blocking: *blocking, sprite, actions: Vec::new(), body: None, chest });
                    }
                }
                Effect::Exit { x, y, to, label } => {
                    let to = slug(to);
                    let (x, y) = clamp_cell(*x, *y);
                    if to.is_empty() || to == here {
                        notes.push("exit: it leads nowhere".to_string());
                    } else if location.exit_at(x, y).is_some() {
                        notes.push(format!("exit: cell ({x}, {y}) already has an exit"));
                    } else {
                        location.exits.push(Exit { x, y, to: to.clone(), label: label.trim().to_string() });
                        if !self.locations.contains_key(&to) {
                            let name = if label.trim().is_empty() { to.clone() } else { label.trim().to_string() };
                            self.stubs.entry(to).or_insert(Stub { name, description: String::new() });
                        }
                    }
                }
                Effect::Fact { text } => self.story.remember(text),
                Effect::Light { light } => location.light = light.trim().to_lowercase(),
                Effect::Clean { x, y } => {
                    let cell = if *x >= 0 && *y >= 0 { clamp_cell(*x, *y) } else { party_cell };
                    // Beside the party counts too: what one picks up is rarely exactly underfoot.
                    let near = |m: &crate::model::Mark| m.x.abs_diff(cell.0) <= 1 && m.y.abs_diff(cell.1) <= 1;
                    if !location.marks.iter().any(near) {
                        notes.push(format!("clean: nothing lies at {cell:?}"));
                    }
                    let first = location.marks.iter().position(|m| (m.x, m.y) == cell).or_else(|| location.marks.iter().position(near));
                    if let Some(at) = first {
                        location.marks.remove(at);
                    }
                }
                Effect::Travel { to, name, who, description } => {
                    travel_look = description.trim().to_string();
                    let to = slug(to);
                    if !to.is_empty() && to != here {
                        let heroes: Vec<usize> = if who.is_empty() { present.clone() } else { who.iter().flat_map(|w| heroes_named(&slug(w))).collect() };
                        travel = Some((to, name.trim().to_string(), heroes));
                    }
                }
                Effect::Mark { id, x, y, kind, color } => {
                    let id = resolve(id);
                    let under = location.props.iter().find(|p| p.id == id).map(|p| (p.x, p.y)).or_else(|| location.creatures.iter().find(|c| c.id == id).map(|c| (c.x, c.y)));
                    let (cx, cy) = match under {
                        Some(cell) => cell,
                        None if *x >= 0 && *y >= 0 => clamp_cell(*x, *y),
                        None => party_cell,
                    };
                    location.marks.retain(|m| (m.x, m.y) != (cx, cy));
                    location.marks.push(crate::model::Mark { x: cx, y: cy, kind: kind.trim().to_lowercase(), color: color.trim().to_string() });
                    // A place keeps its newest marks; the oldest wear away.
                    let extra = location.marks.len().saturating_sub(40);
                    location.marks.drain(..extra);
                }
                Effect::Animate { id, motion } => {
                    let id = if id.trim().is_empty() { "party".to_string() } else { resolve(id) };
                    // Someone who is not here cannot move: the cue is dropped.
                    let known = matches!(id.as_str(), "party" | "hero" | "heroes" | "player" | "players" | "everyone" | "all") || !heroes_named(&id).is_empty() || location.creatures.iter().any(|c| c.id == id) || location.props.iter().any(|p| p.id == id);
                    if !known {
                        continue;
                    }
                    self.cue_seq += 1;
                    self.cues.push(crate::model::Cue { seq: self.cue_seq, place: here.clone(), id, motion: motion.trim().to_lowercase() });
                    let extra = self.cues.len().saturating_sub(16);
                    self.cues.drain(..extra);
                }
                Effect::Quest { goal, advance, done } => {
                    let quest = &mut self.quest;
                    if *advance && quest.act + 1 < quest.acts.len().max(1) {
                        quest.act += 1;
                    }
                    if !goal.trim().is_empty() {
                        quest.goal = goal.trim().to_string();
                    }
                    match done.trim().to_lowercase().as_str() {
                        "won" | "win" | "success" => quest.done = Some(true),
                        "lost" | "lose" | "failed" | "failure" => quest.done = Some(false),
                        _ => {}
                    }
                }
                Effect::QuestNew { quest } => {
                    // A quest still being played is not thrown away for another.
                    if !self.quest.is_set() || self.quest.done.is_some() {
                        let finished = self.quest.chapters_done + u32::from(self.quest.done.is_some());
                        let tasks = std::mem::take(&mut self.quest.tasks);
                        self.quest = crate::model::Quest { chapters_done: finished, tasks, ..quest.quest() };
                    } else {
                        notes.push("quest_new: the main quest is still being played".to_string());
                    }
                }
                Effect::Task { name, step, done } => {
                    let name = name.trim();
                    if !name.is_empty() {
                        match self.quest.tasks.iter_mut().find(|t| t.name.eq_ignore_ascii_case(name)) {
                            Some(task) => {
                                if !step.trim().is_empty() {
                                    task.step = step.trim().to_string();
                                }
                                task.done |= *done;
                            }
                            None => {
                                self.quest.tasks.push(crate::model::Task { name: name.to_string(), step: step.trim().to_string(), done: *done });
                                let extra = self.quest.tasks.len().saturating_sub(12);
                                self.quest.tasks.drain(..extra);
                            }
                        }
                    }
                }
                Effect::Shake { seconds } => {
                    // Shown like a movement: once, to everyone in the place.
                    let seconds = seconds.as_f64().filter(|s| *s > 0.0).map_or(1.2, |s| s.clamp(0.3, 6.0)) as f32;
                    self.cue_seq += 1;
                    self.cues.push(crate::model::Cue { seq: self.cue_seq, place: here.clone(), id: "screen".to_string(), motion: format!("quake {seconds}") });
                }
                Effect::Weather { weather } => location.weather = weather.trim().to_lowercase(),
                Effect::Music { mood } => location.mood = mood.trim().to_lowercase(),
                Effect::Check { .. } | Effect::Damage { .. } | Effect::Heal { .. } | Effect::Xp { .. } | Effect::Fight | Effect::Deed { .. } | Effect::Revive { .. } => {}
            }
        }
        self.moves.extend(moved);
        if let Some((to, name, heroes)) = travel {
            // A place nobody has heard of yet is named now and drawn when the party arrives.
            if !self.locations.contains_key(&to) && !self.stubs.contains_key(&to) {
                let name = if name.is_empty() { to.replace('_', " ") } else { name };
                self.stubs.insert(to.clone(), crate::model::Stub { name, description: travel_look });
            }
            self.travel = Some((to, heroes));
        }
        // Whatever changed, the party must not end up inside something.
        let (px, py) = self.player.cell();
        if let Some(location) = self.locations.get(&here)
            && location.blocked(px, py, &self.tiles)
        {
            let start = clamp_cell(px as i64, py as i64);
            let free = |(cx, cy): (usize, usize)| !location.blocked(cx as i32, cy as i32, &self.tiles);
            if let Some((cx, cy)) = nearest(start, free) {
                self.player = Player::in_cell(cx, cy);
            }
        }
        notes
    }

    /// Carries out the `index`-th prepared action of a thing: the outcome is recorded, the
    /// effects applied, and the action is used up. Returns the outcome to show.
    pub fn perform(&mut self, thing: &str, index: usize) -> Option<String> {
        let here = self.current.clone();
        let location = self.locations.get_mut(&here)?;
        let actions = match location.props.iter_mut().find(|p| p.id == thing) {
            Some(p) => &mut p.actions,
            None => &mut location.creatures.iter_mut().find(|c| c.id == thing)?.actions,
        };
        if index >= actions.len() {
            return None;
        }
        let action = actions.remove(index);
        self.story.say(Speaker::Player, &action.label);
        self.story.say(Speaker::Master, &action.outcome);
        self.apply(&action.effects, Some(thing));
        Some(action.outcome)
    }

    /// The prop or creature nearest the party within `reach` pixels: what an "interact"
    /// press refers to.
    pub fn thing_near(&self, reach: f32) -> Option<&str> {
        let here = self.here();
        let distance = |x: usize, y: usize| {
            let centre = Player::in_cell(x, y);
            (centre.x - self.player.x).hypot(centre.y - self.player.y)
        };
        let props = here.props.iter().map(|p| (p.id.as_str(), distance(p.x, p.y)));
        let creatures = here.creatures.iter().map(|c| (c.id.as_str(), distance(c.x, c.y)));
        props.chain(creatures).filter(|(_, d)| *d <= reach).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(id, _)| id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{WorldDraft, build_world};
    use crate::fake;
    use serde_json::json;

    fn world() -> World {
        let draft: WorldDraft = serde_json::from_str(&fake::world("story")).unwrap();
        build_world(&draft, "story", "English").unwrap().0
    }

    #[test]
    fn a_mark_lies_where_the_party_stands_or_where_it_is_put_and_replaces_one_there() {
        let mut w = world();
        let at = w.player.cell();
        w.apply(&effects(json!([{ "op": "mark", "kind": "Dung" }])), None);
        w.apply(&effects(json!([{ "op": "mark", "x": "3", "y": 4, "kind": "blood", "color": "#880000" }])), None);
        w.apply(&effects(json!([{ "op": "mark", "x": 3, "y": 4, "kind": "ash" }])), None);
        let marks = &w.here().marks;
        assert_eq!(marks.len(), 2);
        assert_eq!((marks[0].x as i32, marks[0].y as i32, marks[0].kind.as_str()), (at.0, at.1, "dung"));
        assert_eq!((marks[1].x, marks[1].y, marks[1].kind.as_str()), (3, 4, "ash"));
    }

    #[test]
    fn what_was_seen_of_a_place_is_remembered() {
        let mut w = world();
        let place = w.locations.get_mut(&w.current.clone()).unwrap();
        assert!(!place.was_seen(5, 5));
        assert!(place.reveal(5.5, 5.5, 2.0));
        assert!(place.was_seen(5, 5) && place.was_seen(6, 6) && !place.was_seen(8, 5));
        assert!(!place.reveal(5.5, 5.5, 2.0), "nothing new the second time");
    }

    #[test]
    fn the_story_moves_and_sends_only_the_heroes_it_names_or_those_here() {
        let mut w = world();
        let fighter = rules::srd().class("fighter").unwrap();
        let skills: Vec<String> = fighter.skill_options.iter().take(fighter.skill_picks).cloned().collect();
        for name in ["Ann", "Bob"] {
            w.party.push(rules::Character::create(name, "", "human", "fighter", [15, 14, 13, 12, 10, 8], &skills, &[]).unwrap());
        }
        w.owners = vec!["ann".into(), "bob".into()];
        w.apply(&effects(json!([{ "op": "move", "id": "Bob", "x": 3, "y": 3 }])), None);
        assert_eq!(w.moves.len(), 1);
        assert_eq!(w.moves[0].0, 1);
        w.moves.clear();
        w.apply(&effects(json!([{ "op": "move", "id": "party", "x": 4, "y": 4 }])), None);
        assert_eq!(w.moves.iter().map(|m| m.0).collect::<Vec<_>>(), [0, 1]);
        w.apply(&effects(json!([{ "op": "travel", "to": "far_hills", "name": "Far Hills", "who": ["Ann"] }])), None);
        assert_eq!(w.travel, Some(("far_hills".to_string(), vec![0])));
        assert!(w.stubs.contains_key("far_hills"));
    }

    #[test]
    fn a_stretch_of_ground_changes_at_once_and_a_new_place_keeps_its_look() {
        let mut w = world();
        let notes = w.apply(&effects(json!([{ "op": "tile", "x": 3, "y": 5, "to": [7, 5], "tile": "wood_floor" }])), None);
        assert!(notes.is_empty(), "{notes:?}");
        assert!((3..=7).all(|x| w.here().tile_id(x, 5) == Some("wood_floor")));
        w.apply(&effects(json!([{ "op": "travel", "to": "open_space", "name": "Open Space", "description": "Stars and a frozen world below." }])), None);
        assert_eq!(w.stubs["open_space"].description, "Stars and a frozen world below.");
    }

    #[test]
    fn movements_are_numbered_so_that_each_is_shown_once() {
        let mut w = world();
        w.apply(&effects(json!([{ "op": "animate", "id": "cat", "motion": "Jump" }, { "op": "animate", "motion": "crouch" }])), None);
        let seen: Vec<_> = w.cues.iter().map(|c| (c.seq, c.id.as_str(), c.motion.as_str())).collect();
        assert_eq!(seen, [(1, "cat", "jump"), (2, "party", "crouch")]);
        assert!(w.cues.iter().all(|c| c.place == w.current));
    }

    fn effects(value: Value) -> Vec<Effect> {
        let mut notes = Vec::new();
        let read = read_effects(value.as_array().unwrap(), &mut notes);
        assert!(notes.is_empty(), "{notes:?}");
        read
    }

    #[test]
    fn a_malformed_effect_is_dropped_and_the_rest_are_kept() {
        let mut notes = Vec::new();
        let read = read_effects(json!([{ "op": "fact", "text": "a" }, { "op": "explode" }, { "text": "no op" }, { "op": "give", "name": "Key" }]).as_array().unwrap(), &mut notes);
        assert_eq!(read, [Effect::Fact { text: "a".into() }, Effect::Give { name: "Key".into(), description: String::new(), to: String::new(), traits: ItemTraits::default() }]);
        assert_eq!(notes.len(), 2);
    }

    #[test]
    fn a_prepared_action_tells_its_outcome_changes_the_world_and_is_used_up() {
        let mut w = world();
        let before = w.here().creatures.len();
        assert_eq!(w.here().creatures[0].actions.len(), 2);
        let outcome = w.perform("cat", 1).unwrap();
        assert!(outcome.contains("slips away"));
        assert_eq!(w.here().creatures.len(), before - 1);
        assert_eq!(w.story.bags[0].inventory, [Item { name: "Cat hair".into(), description: "A tuft of grey fur.".into(), count: 1, gear: None, at: Some((0, 0)), ..Default::default() }]);
        assert_eq!(w.story.log.len(), 2);
        assert_eq!(w.story.log[0].speaker, Speaker::Player);
        // The cat is gone, and so are its actions.
        assert_eq!(w.perform("cat", 0), None);
    }

    #[test]
    fn an_action_that_does_not_exist_changes_nothing() {
        let mut w = world();
        let before = w.clone();
        assert_eq!(w.perform("fire", 9), None);
        assert_eq!(w.perform("nobody", 0), None);
        assert_eq!(w, before);
    }

    #[test]
    fn items_stack_and_are_taken_one_at_a_time() {
        let mut w = world();
        let notes = w.apply(&effects(json!([{ "op": "give", "name": "Arrow" }, { "op": "give", "name": "arrow" }, { "op": "take", "name": "ARROW" }, { "op": "take", "name": "Sword" }])), None);
        assert_eq!(w.story.bags[0].inventory[0].count, 1);
        assert_eq!(notes, ["take: the party has no 'Sword'"]);
        w.apply(&effects(json!([{ "op": "take", "name": "Arrow" }])), None);
        assert!(w.story.bags[0].inventory.is_empty());
    }

    #[test]
    fn effects_on_things_that_are_not_here_are_noted_and_skipped() {
        let mut w = world();
        let notes = w.apply(&effects(json!([{ "op": "remove", "id": "ghost" }, { "op": "attitude", "id": "ghost", "attitude": "hostile" }, { "op": "fact", "text": "The fire never dies." }])), None);
        assert_eq!(notes.len(), 2);
        assert_eq!(w.story.facts, ["The fire never dies."]);
    }

    #[test]
    fn a_spawned_creature_stands_on_free_ground_and_an_undrawn_sprite_gets_the_marker() {
        let mut w = world();
        let fire = w.here().props[0].clone();
        let notes = w.apply(&effects(json!([{ "op": "spawn", "kind": "creature", "id": "cat", "name": "Second cat", "x": fire.x, "y": fire.y, "sprite": "dragon" }])), None);
        let newcomer = w.here().creatures.iter().find(|c| c.id == "cat_2").expect("a second cat with its own id");
        assert_ne!((newcomer.x, newcomer.y), (fire.x, fire.y));
        assert_eq!(newcomer.sprite, MARKER_SPRITE);
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn a_tile_can_be_changed_to_one_the_place_did_not_use_before() {
        let mut w = world();
        assert!(!w.here().legend.values().any(|t| t == "stone_floor"));
        let notes = w.apply(&effects(json!([{ "op": "tile", "x": 3, "y": 3, "tile": "stone_floor" }, { "op": "tile", "x": 4, "y": 3, "tile": "lava" }])), None);
        assert_eq!(w.here().tile_id(3, 3), Some("stone_floor"));
        assert_eq!(notes, ["tile: there is no tile 'lava'"]);
        assert!(w.here().map.iter().all(|r| r.len() == MAP_W));
    }

    #[test]
    fn walling_the_party_in_moves_it_to_free_ground() {
        let mut w = world();
        let (x, y) = w.player.cell();
        w.apply(&effects(json!([{ "op": "tile", "x": x, "y": y, "tile": "stone_wall" }])), None);
        let (nx, ny) = w.player.cell();
        assert_ne!((nx, ny), (x, y));
        assert!(!w.here().blocked(nx, ny, &w.tiles));
    }

    #[test]
    fn a_new_exit_brings_a_place_to_go_to() {
        let mut w = world();
        let notes = w.apply(&effects(json!([{ "op": "exit", "x": 5, "y": 5, "to": "Hidden Cellar", "label": "A trapdoor" }, { "op": "exit", "x": 5, "y": 5, "to": "elsewhere" }])), None);
        assert_eq!(w.here().exit_at(5, 5).map(|e| e.to.as_str()), Some("hidden_cellar"));
        assert_eq!(w.stubs["hidden_cellar"].name, "A trapdoor");
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn the_nearest_thing_within_reach_is_the_one_interacted_with() {
        let mut w = world();
        let cat = w.here().creatures[0].clone();
        w.player = Player::in_cell(cat.x, cat.y + 1);
        assert_eq!(w.thing_near(24.0), Some("cat"));
        w.player = Player::in_cell(1, 1);
        assert_eq!(w.thing_near(24.0), None);
    }

    #[test]
    fn the_story_is_written_up_when_it_nears_the_budget_and_keeps_the_latest_lines() {
        let mut story = Story::default();
        for n in 0..40 {
            story.say(if n % 2 == 0 { Speaker::Player } else { Speaker::Master }, &"word ".repeat(30));
        }
        assert!(!story.needs_chapter(100_000));
        assert!(story.needs_chapter(2_500));
        story.close_chapter("  They talked a great deal.  ", &["The gate is locked.".into(), "the gate is locked.".into(), "".into()]);
        assert_eq!(story.chapters, ["They talked a great deal."]);
        assert_eq!(story.log.len(), KEPT_LINES);
        assert_eq!(story.facts, ["The gate is locked."]);
        // An empty chapter means the master failed to write one: nothing is thrown away.
        story.close_chapter("  ", &[]);
        assert_eq!((story.chapters.len(), story.log.len()), (1, KEPT_LINES));
    }

    #[test]
    fn a_short_story_is_never_written_up_however_small_the_budget() {
        let mut story = Story::default();
        for _ in 0..KEPT_LINES {
            story.say(Speaker::Master, &"long ".repeat(500));
        }
        assert!(!story.needs_chapter(10));
    }
}
