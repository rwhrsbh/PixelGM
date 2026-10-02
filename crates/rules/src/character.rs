//! A player character: the numbers on the sheet and everything derived from them.

use crate::data::{Class, Spell, Srd, srd};
use crate::dice::{Dice, Edge, Rng, Roll};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Ability {
    Str,
    Dex,
    Con,
    Int,
    Wis,
    Cha,
}

impl Ability {
    pub const ALL: [Ability; 6] = [Ability::Str, Ability::Dex, Ability::Con, Ability::Int, Ability::Wis, Ability::Cha];

    /// The three-letter code the SRD data uses.
    pub fn index(self) -> &'static str {
        ["str", "dex", "con", "int", "wis", "cha"][self as usize]
    }

    pub fn from_index(index: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.index() == index.to_lowercase())
    }

    /// The name as the SRD writes it, which is also the key of its translation.
    pub fn name(self) -> &'static str {
        ["Strength", "Dexterity", "Constitution", "Intelligence", "Wisdom", "Charisma"][self as usize]
    }
}

pub fn modifier(score: i32) -> i32 {
    (score - 10).div_euclid(2)
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AbilityMethod {
    #[default]
    PointBuy,
    StandardArray,
    Roll,
}

pub const STANDARD_ARRAY: [i32; 6] = [15, 14, 13, 12, 10, 8];
pub const POINT_BUDGET: u32 = 27;

/// What a score costs under point buy; scores outside 8–15 cannot be bought.
pub fn point_cost(score: i32) -> Option<u32> {
    [0, 1, 2, 3, 4, 5, 7, 9].get(usize::try_from(score - 8).ok()?).copied()
}

pub fn points_spent(scores: &[i32; 6]) -> Option<u32> {
    scores.iter().map(|s| point_cost(*s)).sum()
}

/// Six scores, each the best three of four six-sided dice, highest first.
pub fn roll_scores(rng: &mut Rng) -> [i32; 6] {
    let mut scores = [0; 6];
    for score in &mut scores {
        let mut dice = [rng.die(6), rng.die(6), rng.die(6), rng.die(6)];
        dice.sort_unstable();
        *score = dice[1..].iter().sum::<u32>() as i32;
    }
    scores.sort_unstable_by(|a, b| b.cmp(a));
    scores
}

/// Which abilities matter most to a class, most important first.
fn priorities(class: &str) -> [Ability; 6] {
    use Ability::*;
    match class {
        "barbarian" | "fighter" => [Str, Con, Dex, Wis, Cha, Int],
        "paladin" => [Str, Cha, Con, Wis, Dex, Int],
        "bard" | "sorcerer" | "warlock" => [Cha, Dex, Con, Wis, Int, Str],
        "cleric" => [Wis, Con, Str, Dex, Cha, Int],
        "druid" => [Wis, Con, Dex, Int, Cha, Str],
        "monk" | "ranger" => [Dex, Wis, Con, Str, Int, Cha],
        "rogue" => [Dex, Con, Int, Wis, Cha, Str],
        _ => [Int, Con, Dex, Wis, Cha, Str],
    }
}

/// Hands out `values` (any order) so that the class's most important abilities get the
/// highest: a starting point the player can change.
pub fn suggested_scores(class: &str, values: [i32; 6]) -> [i32; 6] {
    let mut sorted = values;
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    let mut scores = [0; 6];
    for (value, ability) in sorted.into_iter().zip(priorities(class)) {
        scores[ability as usize] = value;
    }
    scores
}

const XP_FOR_LEVEL: [u32; 20] = [0, 300, 900, 2_700, 6_500, 14_000, 23_000, 34_000, 48_000, 64_000, 85_000, 100_000, 120_000, 140_000, 165_000, 195_000, 225_000, 265_000, 305_000, 355_000];

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct DeathSaves {
    pub successes: u8,
    pub failures: u8,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeathRoll {
    /// Still dying.
    Holding,
    /// Three successes: unconscious but no longer dying.
    Stable,
    /// A natural 20: back on their feet with 1 hit point.
    Revived,
    Dead,
}

/// Levels taken in a class beside the one the character began in.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct ClassLevels {
    pub class: String,
    pub levels: u32,
}

/// What a character gained on reaching a new level.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
pub struct LevelUp {
    /// The class the level was taken in.
    #[serde(default)]
    pub class: String,
    pub level: u32,
    pub hp_gained: i32,
    pub features: Vec<String>,
    /// The class grants two points to put into ability scores at this level.
    pub ability_improvement: bool,
}

/// An ability outside the book: one the game master invented for a character, or a secret
/// skill earned by an unusual deed. It is put together from things the rules can carry out
/// (harm, healing, a few plain numbers) and cut to what the character's level allows.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Feat {
    pub name: String,
    pub description: String,
    /// Dice of harm or healing; empty for a feat that only changes numbers.
    pub dice: String,
    pub heals: bool,
    /// Feet.
    pub range: u32,
    /// Feet around the target; 0 for one target.
    pub area: u32,
    /// Targets roll this save instead of the character rolling to hit.
    pub save: Option<Ability>,
    /// Times between long rests; 0 is at will.
    pub uses: u8,
    pub used: u8,
    /// Plain numbers added for as long as the character has the feat.
    pub ac: i32,
    pub hp: i32,
    /// Feet per turn.
    pub speed: i32,
    /// Earned by an unusual deed rather than chosen at a level.
    pub secret: bool,
}

impl Feat {
    /// Cuts the feat down to what a character of `level` may have: an average roll within
    /// the level's budget (smaller at will, smaller over an area), sane ranges, small
    /// passive numbers. A table that plays without limits skips the budget, not the sanity.
    pub fn fit(&mut self, level: u32, limits: bool) {
        let level = level.clamp(1, 20) as f32;
        self.name = self.name.trim().chars().take(40).collect();
        self.description = self.description.trim().chars().take(300).collect();
        self.uses = self.uses.min(5);
        self.used = 0;
        self.range = self.range.clamp(5, 150);
        self.area = self.area.min(30);
        match Dice::parse(&self.dice, 0) {
            Some(mut dice) if dice.average() > 0.0 => {
                dice.count = dice.count.min(20);
                dice.sides = dice.sides.clamp(0, 12);
                dice.bonus = dice.bonus.clamp(0, 20);
                if limits {
                    let mut budget = if self.uses == 0 { 3.0 + 2.0 * level } else { 8.0 + 5.0 * level };
                    if self.area > 0 {
                        budget *= 0.6;
                    }
                    dice.bonus = dice.bonus.min((budget / 3.0) as i32);
                    let each = (dice.sides as f32 + 1.0) / 2.0;
                    if dice.sides > 0 && dice.average() > budget {
                        dice.count = (((budget - dice.bonus as f32) / each).floor() as u32).max(1);
                    }
                    // One die can still be too much: then it is a smaller die.
                    if dice.sides > 0 && dice.average() > budget {
                        dice.sides = (((budget - dice.bonus as f32) * 2.0 - 1.0).floor() as u32).clamp(2, 12);
                    }
                }
                self.dice = dice.to_string();
            }
            _ => {
                self.dice.clear();
                self.heals = false;
                self.uses = 0;
            }
        }
        let (ac, hp, speed) = if limits { (1 + level as i32 / 10, 2 * level as i32, 10) } else { (5, 100, 60) };
        self.ac = self.ac.clamp(0, ac);
        self.hp = self.hp.clamp(0, hp);
        self.speed = self.speed.clamp(0, speed) / 5 * 5;
    }

    pub fn uses_left(&self) -> Option<u8> {
        (self.uses > 0).then(|| self.uses.saturating_sub(self.used))
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Character {
    pub name: String,
    /// Free text the player wrote; the game master uses it for the story.
    pub description: String,
    pub race: String,
    pub class: String,
    pub level: u32,
    pub xp: u32,
    /// The six scores with the people's bonuses already added.
    pub scores: [i32; 6],
    pub max_hp: i32,
    pub hp: i32,
    /// Indices of the skills the character is proficient in.
    pub skills: Vec<String>,
    pub weapons: Vec<String>,
    pub armor: Option<String>,
    pub shield: bool,
    /// Cantrips and spells known or prepared.
    pub spells: Vec<String>,
    /// Slots of levels 1–9 already spent since the last long rest.
    pub slots_used: [u8; 9],
    pub death: DeathSaves,
    /// Ability score points earned by levels and not yet placed.
    #[serde(default)]
    pub points_to_place: u8,
    #[serde(default)]
    pub dead: bool,
    /// Hit dice spent on short rests since the last long one. A character has as many
    /// hit dice as levels.
    #[serde(default)]
    pub hit_dice_used: u32,
    /// Abilities outside the book.
    #[serde(default)]
    pub feats: Vec<Feat>,
    /// The character reached a level at which the game master may offer an ability of
    /// its own, and has not been offered one yet.
    #[serde(default)]
    pub feat_due: bool,
    /// Levels in other classes. `level` is the whole of the character; what is not listed
    /// here is in `class`.
    #[serde(default)]
    pub extra: Vec<ClassLevels>,
    /// The class the next levels go to; the first class when not set.
    #[serde(default)]
    pub advancing: Option<String>,
}

impl Character {
    /// A new first-level character. `base` are the scores before the people's bonuses.
    pub fn create(name: &str, description: &str, race: &str, class: &str, base: [i32; 6], skills: &[String], spells: &[String]) -> Result<Self, String> {
        let data = srd();
        let race_data = data.race(race).ok_or_else(|| format!("there is no people '{race}'"))?;
        let class_data = data.class(class).ok_or_else(|| format!("there is no class '{class}'"))?;
        if name.trim().is_empty() {
            return Err("a character needs a name".into());
        }
        if skills.len() != class_data.skill_picks || skills.iter().any(|s| !class_data.skill_options.contains(s)) {
            return Err(format!("choose {} skills from the class's list", class_data.skill_picks));
        }
        if (1..skills.len()).any(|i| skills[..i].contains(&skills[i])) {
            return Err("a skill was chosen twice".into());
        }
        let mut scores = base;
        // The people's bonuses, or, in the edition where a people gives none, a background's.
        for ((score, bonus), background) in scores.iter_mut().zip(race_data.bonuses).zip(class_data.background) {
            *score += bonus + background;
        }
        let mut character = Self {
            name: name.trim().to_string(),
            description: description.trim().to_string(),
            race: race.to_string(),
            class: class.to_string(),
            level: 1,
            xp: 0,
            scores,
            max_hp: 1,
            hp: 1,
            skills: skills.to_vec(),
            weapons: class_data.kit.weapons.iter().map(|w| w.to_string()).collect(),
            armor: class_data.kit.armor.map(str::to_string),
            shield: class_data.kit.shield,
            spells: Vec::new(),
            slots_used: [0; 9],
            death: DeathSaves::default(),
            points_to_place: 0,
            dead: false,
            hit_dice_used: 0,
            feats: Vec::new(),
            feat_due: false,
            extra: Vec::new(),
            advancing: None,
        };
        character.max_hp = (class_data.hit_die as i32 + character.modifier(Ability::Con)).max(1);
        character.hp = character.max_hp;
        character.set_spells(spells)?;
        Ok(character)
    }

    fn class_data(&self) -> &'static Class {
        srd().class(&self.class).unwrap_or(&srd().classes[0])
    }

    /// The table row of the whole character's level in its first class: what does not
    /// depend on how the levels are split, such as the proficiency bonus.
    fn level_data(&self) -> &'static crate::data::Level {
        &self.class_data().levels[(self.level.clamp(1, 20) - 1) as usize]
    }

    /// The levels the character has in each of its classes, the first class first.
    pub fn class_levels(&self) -> Vec<(&'static Class, u32)> {
        let elsewhere: u32 = self.extra.iter().map(|e| e.levels).sum();
        let mut split = vec![(self.class_data(), self.level.saturating_sub(elsewhere).max(1))];
        split.extend(self.extra.iter().filter_map(|e| Some((srd().class(&e.class)?, e.levels.clamp(1, 20)))));
        split
    }

    fn row(class: &'static Class, level: u32) -> &'static crate::data::Level {
        &class.levels[(level.clamp(1, 20) - 1) as usize]
    }

    /// Whether the character's scores allow levels in `class` beside the first: 13 in what
    /// each of the two classes is built on.
    pub fn may_take(&self, class: &str) -> bool {
        if class == self.class || self.extra.iter().any(|e| e.class == class) {
            return true;
        }
        let Some(wanted) = srd().class(class) else { return false };
        let enough = |index: &str| crate::data::primary_abilities(index).iter().take(if matches!(index, "monk" | "paladin" | "ranger") { 2 } else { 1 }).all(|a| self.scores[*a as usize] >= 13);
        enough(&wanted.index) && enough(&self.class)
    }

    /// The class the next level will be taken in.
    pub fn advancing(&self) -> &str {
        self.advancing.as_deref().filter(|c| self.may_take(c)).unwrap_or(&self.class)
    }

    /// Turns the next levels towards another class the character may take: the one after
    /// the current choice in the list of classes.
    pub fn advance_next_class(&mut self) -> &str {
        let classes = &srd().classes;
        let now = classes.iter().position(|c| c.index == self.advancing()).unwrap_or(0);
        let next = (1..=classes.len()).map(|step| &classes[(now + step) % classes.len()]).find(|c| self.may_take(&c.index));
        self.advancing = next.map(|c| c.index.clone()).filter(|c| *c != self.class);
        self.advancing()
    }

    pub fn modifier(&self, ability: Ability) -> i32 {
        modifier(self.scores[ability as usize])
    }

    pub fn proficiency(&self) -> i32 {
        self.level_data().prof_bonus
    }

    pub fn save_bonus(&self, ability: Ability) -> i32 {
        self.modifier(ability) + if self.class_data().saves.contains(&ability) { self.proficiency() } else { 0 }
    }

    /// The bonus to a check with `ability`; `skill` adds the proficiency bonus when the
    /// character is trained in it.
    pub fn check_bonus(&self, ability: Ability, skill: Option<&str>) -> i32 {
        self.modifier(ability) + if skill.is_some_and(|s| self.skills.iter().any(|own| own == s)) { self.proficiency() } else { 0 }
    }

    pub fn check(&self, rng: &mut Rng, ability: Ability, skill: Option<&str>, edge: Edge) -> Roll {
        Roll::d20(rng, self.check_bonus(ability, skill), edge)
    }

    /// Armour class: armour and shield, or the unarmoured defence of barbarians and monks.
    pub fn ac(&self) -> i32 {
        let dex = self.modifier(Ability::Dex);
        let body = match self.armor.as_deref().and_then(|a| srd().armors.get(a)) {
            Some(armor) => armor.base + if armor.adds_dex { armor.max_dex.map_or(dex, |max| dex.min(max)) } else { 0 },
            None => match self.class.as_str() {
                "barbarian" => 10 + dex + self.modifier(Ability::Con),
                "monk" if !self.shield => 10 + dex + self.modifier(Ability::Wis),
                _ => 10 + dex,
            },
        };
        body + if self.shield { 2 } else { 0 } + self.feats.iter().map(|f| f.ac).sum::<i32>()
    }

    /// Feet per turn.
    pub fn speed(&self) -> u32 {
        srd().race(&self.race).map_or(30, |r| r.speed) + self.feats.iter().map(|f| f.speed.max(0) as u32).sum::<u32>()
    }

    /// Takes an ability from outside the book, cut to the character's level.
    pub fn grant(&mut self, mut feat: Feat, limits: bool) {
        feat.fit(self.level, limits);
        if feat.name.is_empty() {
            return;
        }
        self.max_hp += feat.hp;
        self.hp += feat.hp;
        self.feats.push(feat);
    }

    /// The best of the abilities a feat could be driven by.
    pub fn feat_modifier(&self) -> i32 {
        let casting = self.casting_ability().map_or(i32::MIN, |a| self.modifier(a));
        casting.max(self.modifier(Ability::Str)).max(self.modifier(Ability::Dex))
    }

    pub fn hit_dice_left(&self) -> u32 {
        self.level.saturating_sub(self.hit_dice_used)
    }

    /// An hour's rest: hit dice are rolled, one at a time, until the character is whole or
    /// has none left. A warlock's slots come back. Gives the hit points regained.
    pub fn short_rest(&mut self, rng: &mut Rng) -> i32 {
        if self.dead {
            return 0;
        }
        let before = self.hp.max(0);
        let die = self.class_data().hit_die;
        while self.hp < self.max_hp && self.hit_dice_left() > 0 {
            self.hit_dice_used += 1;
            let rolled = (rng.die(die) as i32 + self.modifier(Ability::Con)).max(1);
            self.hp = (self.hp.max(0) + rolled).min(self.max_hp);
        }
        if self.hp > 0 {
            self.death = DeathSaves::default();
        }
        if self.class == "warlock" {
            self.slots_used = [0; 9];
        }
        self.hp.max(0) - before
    }

    /// The ability a weapon attack uses: Dexterity for ranged weapons, the better of the two
    /// for finesse weapons, Strength otherwise.
    fn weapon_ability(&self, weapon: &crate::data::Weapon) -> Ability {
        let finesse_dex = weapon.finesse && self.modifier(Ability::Dex) > self.modifier(Ability::Str);
        if weapon.ranged || finesse_dex { Ability::Dex } else { Ability::Str }
    }

    pub fn attack_bonus(&self, weapon: &str) -> i32 {
        srd().weapons.get(weapon).map_or(self.modifier(Ability::Str), |w| self.modifier(self.weapon_ability(w))) + self.proficiency()
    }

    pub fn damage(&self, weapon: &str) -> Dice {
        match srd().weapons.get(weapon) {
            Some(w) => Dice { bonus: w.dice.bonus + self.modifier(self.weapon_ability(w)), ..w.dice },
            // Bare hands.
            None => Dice::new(0, 0, 1 + self.modifier(Ability::Str)),
        }
    }

    /// Attacks made with one Attack action: the most any one of the classes gives.
    pub fn attacks_per_action(&self) -> u32 {
        1 + self.class_levels().iter().map(|(class, level)| Self::row(class, *level).extra_attacks as u32).max().unwrap_or(0)
    }

    /// The ability spells are cast with: that of the first class that casts.
    pub fn casting_ability(&self) -> Option<Ability> {
        self.class_levels().iter().find_map(|(class, _)| class.casting)
    }

    pub fn spell_dc(&self) -> i32 {
        8 + self.proficiency() + self.casting_ability().map_or(0, |a| self.modifier(a))
    }

    pub fn spell_attack(&self) -> i32 {
        self.proficiency() + self.casting_ability().map_or(0, |a| self.modifier(a))
    }

    /// Slots of each level the character has when rested. A character of one class reads
    /// them off the class's table. One of several adds its casting levels together — whole
    /// for full casters, half for paladins and rangers — and reads the table of a full
    /// caster at that level.
    pub fn slots(&self) -> [u8; 9] {
        let split = self.class_levels();
        if split.len() == 1 {
            return Self::row(split[0].0, split[0].1).slots;
        }
        let casting: u32 = split.iter().filter(|(class, _)| class.casting.is_some()).map(|(class, level)| if matches!(class.index.as_str(), "paladin" | "ranger") { level / 2 } else { *level }).sum();
        match (casting, srd().class("wizard")) {
            (1.., Some(full)) => Self::row(full, casting).slots,
            _ => [0; 9],
        }
    }

    pub fn slots_left(&self, level: u8) -> u8 {
        let i = (level.clamp(1, 9) - 1) as usize;
        self.slots()[i].saturating_sub(self.slots_used[i])
    }

    /// The highest level of spell the character has slots for; 0 when it has none.
    pub fn highest_slot(&self) -> u8 {
        self.slots().iter().rposition(|n| *n > 0).map_or(0, |i| i as u8 + 1)
    }

    pub fn spend_slot(&mut self, level: u8) -> bool {
        if level == 0 || self.slots_left(level) == 0 {
            return level == 0;
        }
        self.slots_used[(level - 1) as usize] += 1;
        true
    }

    pub fn cantrips_allowed(&self) -> usize {
        self.class_levels().iter().map(|(class, level)| Self::row(class, *level).cantrips as usize).sum()
    }

    /// How many levelled spells the character may have ready: for each of its classes, a
    /// fixed number if the class learns its spells, ability modifier plus level if it
    /// prepares them.
    pub fn spells_allowed(&self) -> usize {
        if self.highest_slot() == 0 {
            return 0;
        }
        self.class_levels()
            .iter()
            .map(|(class, level)| match (Self::row(class, *level).spells_known, class.casting) {
                (Some(known), _) => known as usize,
                // A class that casts but has no slots of its own yet prepares nothing.
                (None, Some(_)) if Self::row(class, *level).slots == [0; 9] => 0,
                (None, Some(ability)) => {
                    // Paladins and rangers prepare with half their level.
                    let levels = if matches!(class.index.as_str(), "paladin" | "ranger") { level / 2 } else { *level };
                    (self.modifier(ability) + levels as i32).max(1) as usize
                }
                (None, None) => 0,
            })
            .sum()
    }

    /// Replaces the character's spells, checking that the class can cast each and that the
    /// numbers fit.
    pub fn set_spells(&mut self, spells: &[String]) -> Result<(), String> {
        let data = srd();
        let known: Vec<&Spell> = spells.iter().map(|s| data.spells.get(s).ok_or_else(|| format!("there is no spell '{s}'"))).collect::<Result<_, _>>()?;
        let mine = |spell: &Spell| self.class_levels().iter().any(|(class, _)| spell.classes.contains(&class.index));
        if let Some(wrong) = known.iter().find(|s| !mine(s) || s.level > self.highest_slot()) {
            return Err(format!("{} cannot be cast by this character", wrong.name));
        }
        let cantrips = known.iter().filter(|s| s.level == 0).count();
        if cantrips > self.cantrips_allowed() || known.len() - cantrips > self.spells_allowed() {
            return Err(format!("too many: {} cantrips and {} spells are allowed", self.cantrips_allowed(), self.spells_allowed()));
        }
        self.spells = spells.to_vec();
        Ok(())
    }

    pub fn xp_for_next_level(&self) -> Option<u32> {
        XP_FOR_LEVEL.get(self.level as usize).copied()
    }

    /// Adds experience and raises the level as far as it reaches. Hit points grow by the
    /// class's average roll, as the rules allow in place of rolling.
    pub fn gain_xp(&mut self, xp: u32) -> Vec<LevelUp> {
        self.xp = self.xp.saturating_add(xp);
        let mut gained = Vec::new();
        while self.xp_for_next_level().is_some_and(|needed| self.xp >= needed) {
            // The level goes to the class the character is turned towards.
            let class = srd().class(self.advancing()).unwrap_or(self.class_data());
            let had = self.class_levels().iter().find(|(c, _)| c.index == class.index).map_or(0, |(_, level)| *level);
            let before = if had == 0 { 0 } else { Self::row(class, had).ability_improvements };
            self.level += 1;
            if class.index != self.class {
                match self.extra.iter_mut().find(|e| e.class == class.index) {
                    Some(entry) => entry.levels += 1,
                    None => self.extra.push(ClassLevels { class: class.index.clone(), levels: 1 }),
                }
            }
            let hp_gained = (class.hit_die as i32 / 2 + 1 + self.modifier(Ability::Con)).max(1);
            self.max_hp += hp_gained;
            self.hp += hp_gained;
            let row = Self::row(class, had + 1);
            let improvement = row.ability_improvements > before;
            if improvement {
                self.points_to_place += 2;
            }
            // Every second level is a moment for something of the character's own.
            if self.level % 2 == 0 {
                self.feat_due = true;
            }
            gained.push(LevelUp { class: class.index.clone(), level: self.level, hp_gained, features: row.features.clone(), ability_improvement: improvement });
        }
        gained
    }

    /// Places one earned point into an ability; scores stop at 20. Constitution going up
    /// raises hit points for every level already gained.
    pub fn improve(&mut self, ability: Ability) -> bool {
        if self.points_to_place == 0 || self.scores[ability as usize] >= 20 {
            return false;
        }
        let before = self.modifier(Ability::Con);
        self.scores[ability as usize] += 1;
        self.points_to_place -= 1;
        let rise = (self.modifier(Ability::Con) - before) * self.level as i32;
        self.max_hp += rise;
        self.hp += rise;
        true
    }

    pub fn is_down(&self) -> bool {
        self.hp <= 0
    }

    /// Takes damage. Damage left over after dropping to 0 that equals the hit point maximum
    /// kills outright; damage while already down counts as a failed death save.
    pub fn take_damage(&mut self, amount: i32) {
        let amount = amount.max(0);
        if self.dead || amount == 0 {
            return;
        }
        if self.hp <= 0 {
            self.death.failures += 1;
            self.dead = self.death.failures >= 3;
            return;
        }
        let left_over = amount - self.hp;
        self.hp = (self.hp - amount).max(0);
        if left_over >= self.max_hp {
            self.dead = true;
        }
    }

    pub fn heal(&mut self, amount: i32) {
        if self.dead || amount <= 0 {
            return;
        }
        self.hp = (self.hp.max(0) + amount).min(self.max_hp);
        self.death = DeathSaves::default();
    }

    /// The roll a dying character makes at the start of its turn.
    pub fn death_save(&mut self, rng: &mut Rng) -> (u32, DeathRoll) {
        let die = rng.die(20);
        let outcome = match die {
            20 => {
                self.heal(1);
                DeathRoll::Revived
            }
            1 => {
                self.death.failures += 2;
                DeathRoll::Holding
            }
            10.. => {
                self.death.successes += 1;
                DeathRoll::Holding
            }
            _ => {
                self.death.failures += 1;
                DeathRoll::Holding
            }
        };
        if self.death.failures >= 3 {
            self.dead = true;
            return (die, DeathRoll::Dead);
        }
        if self.death.successes >= 3 {
            return (die, DeathRoll::Stable);
        }
        (die, outcome)
    }

    /// A night's rest: hit points, spell slots and the uses of feats come back, and half
    /// of the hit dice.
    pub fn long_rest(&mut self) {
        if !self.dead {
            self.hp = self.max_hp;
            self.slots_used = [0; 9];
            self.death = DeathSaves::default();
            self.hit_dice_used = self.hit_dice_used.saturating_sub((self.level / 2).max(1));
            for feat in &mut self.feats {
                feat.used = 0;
            }
        }
    }
}

/// Spells a new character of `class` starts with when the player does not choose: damage and
/// healing first, since those are the ones the rules engine can carry out by itself.
pub fn suggested_spells(data: &Srd, character: &Character) -> Vec<String> {
    let mut pool: Vec<&Spell> = Vec::new();
    for (class, _) in character.class_levels() {
        for spell in data.class_spells(&class.index, character.highest_slot()) {
            if !pool.iter().any(|known| known.index == spell.index) {
                pool.push(spell);
            }
        }
    }
    pool.sort_by_key(|s| (s.level, !s.mechanised(), s.name.clone()));
    let cantrips = pool.iter().filter(|s| s.level == 0).take(character.cantrips_allowed());
    let spells = pool.iter().filter(|s| s.level > 0).take(character.spells_allowed());
    cantrips.chain(spells).map(|s| s.index.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(class: &str, race: &str, base: [i32; 6]) -> Character {
        let data = srd();
        let skills: Vec<String> = data.class(class).unwrap().skill_options.iter().take(data.class(class).unwrap().skill_picks).cloned().collect();
        Character::create("Test", "", race, class, base, &skills, &[]).unwrap()
    }

    #[test]
    fn modifiers_follow_the_table() {
        assert_eq!([1, 8, 9, 10, 11, 15, 20, 30].map(modifier), [-5, -1, -1, 0, 0, 2, 5, 10]);
    }

    #[test]
    fn point_buy_costs_match_the_book_and_27_points_buy_the_usual_spread() {
        assert_eq!([8, 13, 14, 15].map(point_cost), [Some(0), Some(5), Some(7), Some(9)]);
        assert_eq!([7, 16].map(point_cost), [None, None]);
        assert_eq!(points_spent(&[15, 14, 13, 12, 10, 8]), Some(POINT_BUDGET));
        assert_eq!(points_spent(&[16, 8, 8, 8, 8, 8]), None);
    }

    #[test]
    fn rolled_scores_are_three_to_eighteen_highest_first() {
        let mut rng = Rng::seeded(5);
        for _ in 0..50 {
            let scores = roll_scores(&mut rng);
            assert!(scores.iter().all(|s| (3..=18).contains(s)));
            assert!(scores.windows(2).all(|w| w[0] >= w[1]));
        }
    }

    #[test]
    fn the_suggested_spread_puts_the_best_score_where_the_class_needs_it() {
        let wizard = suggested_scores("wizard", [8, 15, 10, 14, 12, 13]);
        assert_eq!(wizard[Ability::Int as usize], 15);
        assert_eq!(wizard[Ability::Str as usize], 8);
        assert_eq!(suggested_scores("fighter", STANDARD_ARRAY)[Ability::Str as usize], 15);
    }

    #[test]
    fn a_new_fighter_has_the_books_numbers() {
        // A hill-less dwarf: +2 Constitution.
        let f = make("fighter", "dwarf", [15, 12, 14, 8, 10, 13]);
        assert_eq!(f.scores[Ability::Con as usize], 16);
        assert_eq!((f.max_hp, f.hp), (13, 13));
        assert_eq!(f.proficiency(), 2);
        // Chain mail 16, shield 2; Dexterity does not count in heavy armour.
        assert_eq!(f.ac(), 18);
        assert_eq!(f.speed(), 25);
        assert_eq!(f.attack_bonus("longsword"), 4);
        assert_eq!(f.damage("longsword"), Dice::new(1, 8, 2));
        // A crossbow uses Dexterity.
        assert_eq!(f.attack_bonus("crossbow-light"), 3);
        assert_eq!((f.save_bonus(Ability::Str), f.save_bonus(Ability::Wis)), (4, 0));
        assert_eq!(f.attacks_per_action(), 1);
    }

    #[test]
    fn unarmoured_defence_and_finesse_are_honoured() {
        let barbarian = make("barbarian", "human", [14, 13, 15, 8, 10, 8]);
        // Human: +1 to everything. 10 + Dex 2 + Con 3.
        assert_eq!(barbarian.ac(), 15);
        let rogue = make("rogue", "elf", [8, 15, 12, 10, 10, 10]);
        // Elf Dex 17 (+3); the rapier is finesse; leather is 11 + Dex.
        assert_eq!((rogue.attack_bonus("rapier"), rogue.damage("rapier"), rogue.ac()), (5, Dice::new(1, 8, 3), 14));
        let monk = make("monk", "human", [10, 15, 10, 10, 15, 10]);
        assert_eq!(monk.ac(), 16);
    }

    #[test]
    fn skills_must_be_the_right_number_from_the_class_list() {
        let pick = |skills: &[&str]| Character::create("A", "", "human", "wizard", STANDARD_ARRAY, &skills.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &[]);
        assert!(pick(&["arcana", "history"]).is_ok());
        assert!(pick(&["arcana"]).is_err());
        assert!(pick(&["arcana", "arcana"]).is_err());
        assert!(pick(&["arcana", "athletics"]).is_err());
        assert!(Character::create(" ", "", "human", "wizard", STANDARD_ARRAY, &[], &[]).is_err());
    }

    #[test]
    fn a_trained_skill_adds_the_proficiency_bonus() {
        let w = make("wizard", "human", [8, 14, 13, 15, 12, 10]);
        // Int 16 (+3); arcana is among the first two wizard options.
        assert!(w.skills.contains(&"arcana".to_string()));
        assert_eq!(w.check_bonus(Ability::Int, Some("arcana")), 5);
        assert_eq!(w.check_bonus(Ability::Int, Some("nature")), 3);
        assert_eq!(w.check_bonus(Ability::Str, None), -1);
    }

    #[test]
    fn a_wizard_prepares_spells_and_spends_slots() {
        let mut w = make("wizard", "human", [8, 14, 13, 15, 12, 10]);
        assert_eq!((w.cantrips_allowed(), w.spells_allowed(), w.highest_slot()), (3, 4, 1));
        assert_eq!((w.spell_dc(), w.spell_attack()), (13, 5));
        let spells = suggested_spells(srd(), &w);
        assert_eq!(spells.len(), 7);
        w.set_spells(&spells).unwrap();
        assert!(w.set_spells(&["fireball".to_string()]).is_err(), "third-level spell at first level");
        assert!(w.set_spells(&["cure-wounds".to_string()]).is_err(), "not a wizard spell");
        assert!(w.spend_slot(1) && w.spend_slot(1));
        assert!(!w.spend_slot(1), "only two first-level slots");
        assert!(w.spend_slot(0), "cantrips cost nothing");
        w.long_rest();
        assert_eq!(w.slots_left(1), 2);
    }

    #[test]
    fn classes_without_magic_have_no_spells_and_half_casters_wait_a_level() {
        let fighter = make("fighter", "human", STANDARD_ARRAY);
        assert_eq!((fighter.cantrips_allowed(), fighter.spells_allowed(), fighter.highest_slot()), (0, 0, 0));
        let mut paladin = make("paladin", "human", [15, 8, 13, 10, 12, 14]);
        assert_eq!(paladin.spells_allowed(), 0);
        paladin.gain_xp(300);
        // Charisma 15 (+2) plus half of level 2.
        assert_eq!((paladin.highest_slot(), paladin.spells_allowed()), (1, 3));
    }

    #[test]
    fn experience_raises_levels_hit_points_and_what_the_class_gives() {
        let mut f = make("fighter", "human", [15, 12, 14, 8, 10, 13]);
        assert_eq!(f.xp_for_next_level(), Some(300));
        assert!(f.gain_xp(299).is_empty());
        let ups = f.gain_xp(6_500);
        // 6799 xp is level 5.
        assert_eq!(ups.iter().map(|u| u.level).collect::<Vec<_>>(), [2, 3, 4, 5]);
        assert_eq!(f.level, 5);
        // Con 15 (+2): 12 at first level, then 6 + 2 a level.
        assert_eq!(f.max_hp, 12 + 4 * 8);
        assert_eq!(f.proficiency(), 3);
        assert_eq!(f.attacks_per_action(), 2);
        assert!(ups[2].ability_improvement && !ups[0].ability_improvement);
        assert!(ups[0].features.iter().any(|x| x.starts_with("Action Surge")));
        assert_eq!(f.points_to_place, 2);
    }

    #[test]
    fn placed_points_raise_scores_to_twenty_and_constitution_raises_hit_points() {
        let mut f = make("fighter", "human", [15, 12, 14, 8, 10, 13]);
        f.gain_xp(2_700);
        let before = f.max_hp;
        // Con 15 -> 16 lifts the modifier from +2 to +3 for all four levels.
        assert!(f.improve(Ability::Con));
        assert_eq!(f.max_hp, before + 4);
        assert!(f.improve(Ability::Str));
        assert!(!f.improve(Ability::Str), "no points left");
        f.points_to_place = 1;
        f.scores[0] = 20;
        assert!(!f.improve(Ability::Str), "scores stop at 20");
    }

    #[test]
    fn level_twenty_is_the_top() {
        let mut f = make("fighter", "human", STANDARD_ARRAY);
        f.gain_xp(1_000_000);
        assert_eq!(f.level, 20);
        assert_eq!(f.xp_for_next_level(), None);
        assert_eq!(f.proficiency(), 6);
    }

    #[test]
    fn damage_downs_a_character_and_massive_damage_kills() {
        let mut w = make("wizard", "human", [8, 14, 13, 15, 12, 10]);
        assert_eq!(w.max_hp, 8);
        w.take_damage(5);
        assert_eq!(w.hp, 3);
        w.take_damage(6);
        assert!(w.is_down() && !w.dead);
        w.heal(2);
        assert_eq!((w.hp, w.death), (2, DeathSaves::default()));
        // 2 hit points left, 10 damage: 8 left over equals the maximum.
        w.take_damage(10);
        assert!(w.dead);
        w.heal(5);
        assert_eq!(w.hp, 0, "the dead are not healed");
    }

    #[test]
    fn damage_while_down_is_a_failed_death_save() {
        let mut w = make("wizard", "human", [8, 14, 13, 15, 12, 10]);
        w.take_damage(8);
        w.take_damage(1);
        w.take_damage(1);
        assert_eq!(w.death.failures, 2);
        assert!(!w.dead);
        w.take_damage(1);
        assert!(w.dead);
    }

    #[test]
    fn death_saves_end_in_death_stability_or_a_natural_twenty() {
        let mut ends = [0; 3];
        for seed in 0..400 {
            let mut w = make("wizard", "human", [8, 14, 13, 15, 12, 10]);
            w.take_damage(8);
            let mut rng = Rng::seeded(seed);
            loop {
                match w.death_save(&mut rng).1 {
                    DeathRoll::Holding => continue,
                    DeathRoll::Dead => {
                        assert!(w.dead);
                        ends[0] += 1;
                    }
                    DeathRoll::Stable => {
                        assert!(!w.dead && w.hp == 0);
                        ends[1] += 1;
                    }
                    DeathRoll::Revived => {
                        assert_eq!(w.hp, 1);
                        ends[2] += 1;
                    }
                }
                break;
            }
        }
        assert!(ends.iter().all(|n| *n > 20), "{ends:?}");
    }

    #[test]
    fn a_sheet_survives_being_saved() {
        let f = make("fighter", "dwarf", [15, 12, 14, 8, 10, 13]);
        let back: Character = serde_json::from_str(&serde_json::to_string(&f).unwrap()).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn a_short_rest_spends_hit_dice_and_a_long_one_gives_half_back() {
        let mut hero = make("fighter", "human", [15, 12, 14, 8, 10, 13]);
        hero.gain_xp(2_700);
        assert_eq!(hero.level, 4);
        hero.hp = 1;
        let gained = hero.short_rest(&mut Rng::seeded(2));
        assert!(gained > 0 && hero.hit_dice_used > 0 && hero.hit_dice_used <= 4, "{gained} {}", hero.hit_dice_used);
        assert_eq!(hero.hp, 1 + gained);
        // A whole character rolls nothing.
        let used = hero.hit_dice_used;
        hero.hp = hero.max_hp;
        assert_eq!(hero.short_rest(&mut Rng::seeded(3)), 0);
        assert_eq!(hero.hit_dice_used, used);
        hero.hit_dice_used = 4;
        hero.hp = 1;
        assert_eq!(hero.short_rest(&mut Rng::seeded(3)), 0, "no dice left");
        hero.long_rest();
        assert_eq!((hero.hp, hero.hit_dice_used), (hero.max_hp, 2));
    }

    #[test]
    fn a_feat_is_cut_to_the_level_and_its_numbers_count() {
        let mut hero = make("fighter", "human", [15, 12, 14, 8, 10, 13]);
        let (ac, hp, speed) = (hero.ac(), hero.max_hp, hero.speed());
        hero.grant(Feat { name: "Storm".into(), dice: "20d12+50".into(), range: 900, area: 10, uses: 9, ac: 7, hp: 500, speed: 44, ..Feat::default() }, true);
        let feat = &hero.feats[0];
        let dice = Dice::parse(&feat.dice, 0).unwrap();
        assert!(dice.average() <= (8.0 + 5.0) * 0.6 + 0.01, "{}", feat.dice);
        assert_eq!((feat.range, feat.uses, feat.ac, feat.hp, feat.speed), (150, 5, 1, 2, 10));
        assert_eq!((hero.ac(), hero.max_hp, hero.speed()), (ac + 1, hp + 2, speed + 10));
        // Without limits the dice stand, within sanity.
        let mut wild = Feat { name: "Wild".into(), dice: "6d10".into(), uses: 1, ..Feat::default() };
        wild.fit(1, false);
        assert_eq!(wild.dice, "6d10");
        // A feat without a name is not a feat.
        hero.grant(Feat::default(), true);
        assert_eq!(hero.feats.len(), 1);
    }

    #[test]
    fn every_second_level_calls_for_a_feat() {
        let mut hero = make("fighter", "human", [15, 12, 14, 8, 10, 13]);
        hero.gain_xp(300);
        assert!(hero.feat_due, "level 2");
        hero.feat_due = false;
        hero.gain_xp(600);
        assert!(!hero.feat_due, "level 3");
    }


    #[test]
    fn levels_can_be_taken_in_a_second_class_when_the_scores_allow() {
        let mut hero = make("fighter", "human", [15, 12, 14, 13, 10, 8]);
        assert!(hero.may_take("wizard"), "strength 16 and intelligence 14");
        assert!(!hero.may_take("bard"), "charisma 9");
        assert!(!hero.may_take("monk"), "a monk needs wisdom as well as dexterity");
        hero.advancing = Some("wizard".into());
        let ups = hero.gain_xp(300);
        assert_eq!((ups[0].class.as_str(), hero.level), ("wizard", 2));
        assert_eq!(hero.class_levels().iter().map(|(c, l)| (c.index.as_str(), *l)).collect::<Vec<_>>(), [("fighter", 1), ("wizard", 1)]);
        // A wizard's hit die, not a fighter's: 6/2+1 plus constitution.
        assert_eq!(ups[0].hp_gained, 4 + hero.modifier(Ability::Con));
        assert_eq!(hero.slots()[0], 2, "one level of a full caster");
        assert_eq!(hero.cantrips_allowed(), 3);
        assert!(hero.set_spells(&["fire-bolt".to_string(), "magic-missile".to_string()]).is_ok());
        assert_eq!(hero.casting_ability(), Some(Ability::Int));
        // Back to the first class: the fighter's second level.
        hero.advancing = None;
        let ups = hero.gain_xp(600);
        assert_eq!((ups[0].class.as_str(), hero.level), ("fighter", 3));
        assert_eq!(hero.class_levels()[0].1, 2);
        assert_eq!(hero.proficiency(), 2);
    }

    #[test]
    fn a_choice_the_scores_do_not_allow_falls_back_to_the_first_class() {
        let mut hero = make("fighter", "human", [15, 12, 14, 8, 10, 13]);
        hero.advancing = Some("wizard".into());
        assert_eq!(hero.advancing(), "fighter", "intelligence 9");
        hero.gain_xp(300);
        assert!(hero.extra.is_empty());
        // Turning through the classes only ever stops at ones that may be taken.
        let mut seen = vec![hero.advancing().to_string()];
        for _ in 0..12 {
            seen.push(hero.advance_next_class().to_string());
        }
        assert!(seen.iter().all(|c| hero.may_take(c)), "{seen:?}");
        assert!(seen.iter().any(|c| c != "fighter"), "charisma 14 opens something: {seen:?}");
    }

    #[test]
    fn half_casters_count_half_towards_the_slots_of_a_mixed_character() {
        let mut hero = make("paladin", "human", [15, 10, 13, 8, 12, 14]);
        hero.gain_xp(900);
        assert_eq!(hero.level, 3);
        let alone = hero.slots();
        hero.advancing = Some("sorcerer".into());
        hero.gain_xp(1_800);
        assert_eq!(hero.level, 4);
        // Paladin 3 counts as one casting level, sorcerer 1 as one: a second-level caster.
        assert_eq!(hero.slots()[0], 3, "was {alone:?}");
    }

}
