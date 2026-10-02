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

/// What a character gained on reaching a new level.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct LevelUp {
    pub level: u32,
    pub hp_gained: i32,
    pub features: Vec<String>,
    /// The class grants two points to put into ability scores at this level.
    pub ability_improvement: bool,
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
        for (score, bonus) in scores.iter_mut().zip(race_data.bonuses) {
            *score += bonus;
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
        };
        character.max_hp = (class_data.hit_die as i32 + character.modifier(Ability::Con)).max(1);
        character.hp = character.max_hp;
        character.set_spells(spells)?;
        Ok(character)
    }

    fn class_data(&self) -> &'static Class {
        srd().class(&self.class).unwrap_or(&srd().classes[0])
    }

    fn level_data(&self) -> &'static crate::data::Level {
        &self.class_data().levels[(self.level.clamp(1, 20) - 1) as usize]
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
        body + if self.shield { 2 } else { 0 }
    }

    /// Feet per turn.
    pub fn speed(&self) -> u32 {
        srd().race(&self.race).map_or(30, |r| r.speed)
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

    /// Attacks made with one Attack action.
    pub fn attacks_per_action(&self) -> u32 {
        1 + self.level_data().extra_attacks as u32
    }

    pub fn casting_ability(&self) -> Option<Ability> {
        self.class_data().casting
    }

    pub fn spell_dc(&self) -> i32 {
        8 + self.proficiency() + self.casting_ability().map_or(0, |a| self.modifier(a))
    }

    pub fn spell_attack(&self) -> i32 {
        self.proficiency() + self.casting_ability().map_or(0, |a| self.modifier(a))
    }

    /// Slots of each level the character has when rested.
    pub fn slots(&self) -> [u8; 9] {
        self.level_data().slots
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
        self.level_data().cantrips as usize
    }

    /// How many levelled spells the character may have ready: a fixed number for classes
    /// that learn their spells, ability modifier plus level for those that prepare them.
    pub fn spells_allowed(&self) -> usize {
        match (self.level_data().spells_known, self.casting_ability()) {
            _ if self.highest_slot() == 0 => 0,
            (Some(known), _) => known as usize,
            (None, Some(ability)) => {
                // Paladins and rangers prepare with half their level.
                let levels = if matches!(self.class.as_str(), "paladin" | "ranger") { self.level / 2 } else { self.level };
                (self.modifier(ability) + levels as i32).max(1) as usize
            }
            (None, None) => 0,
        }
    }

    /// Replaces the character's spells, checking that the class can cast each and that the
    /// numbers fit.
    pub fn set_spells(&mut self, spells: &[String]) -> Result<(), String> {
        let data = srd();
        let known: Vec<&Spell> = spells.iter().map(|s| data.spells.get(s).ok_or_else(|| format!("there is no spell '{s}'"))).collect::<Result<_, _>>()?;
        if let Some(wrong) = known.iter().find(|s| !s.classes.contains(&self.class) || s.level > self.highest_slot()) {
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
            let before = self.level_data().ability_improvements;
            self.level += 1;
            let hp_gained = (self.class_data().hit_die as i32 / 2 + 1 + self.modifier(Ability::Con)).max(1);
            self.max_hp += hp_gained;
            self.hp += hp_gained;
            let improvement = self.level_data().ability_improvements > before;
            if improvement {
                self.points_to_place += 2;
            }
            gained.push(LevelUp { level: self.level, hp_gained, features: self.level_data().features.clone(), ability_improvement: improvement });
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

    /// A night's rest: hit points and spell slots come back.
    pub fn long_rest(&mut self) {
        if !self.dead {
            self.hp = self.max_hp;
            self.slots_used = [0; 9];
            self.death = DeathSaves::default();
        }
    }
}

/// Spells a new character of `class` starts with when the player does not choose: damage and
/// healing first, since those are the ones the rules engine can carry out by itself.
pub fn suggested_spells(data: &Srd, character: &Character) -> Vec<String> {
    let mut pool = data.class_spells(&character.class, character.highest_slot());
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
}
