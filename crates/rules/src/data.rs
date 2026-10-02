//! The System Reference Document as data: peoples, classes with their twenty levels, arms and
//! armour, spells, monsters. Read once from the JSON files shipped with the game.

use crate::character::Ability;
use crate::dice::Dice;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

macro_rules! srd_file {
    ($name:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/srd/2014/en/5e-SRD-", $name, ".json"))
    };
}

#[derive(Debug, Clone)]
pub struct Race {
    pub index: String,
    pub name: String,
    /// Feet per turn.
    pub speed: u32,
    /// Added to the six ability scores, in the order of `Ability`.
    pub bonuses: [i32; 6],
}

/// What a class gives at one level.
#[derive(Debug, Clone, Default)]
pub struct Level {
    pub prof_bonus: i32,
    /// Names of the features gained at this level.
    pub features: Vec<String>,
    /// Spell slots of levels 1 to 9.
    pub slots: [u8; 9],
    pub cantrips: u8,
    /// For classes that know a fixed number of spells; the others prepare theirs.
    pub spells_known: Option<u8>,
    /// How many ability score improvements the class has granted up to this level.
    pub ability_improvements: u8,
    /// Attacks made with the Attack action beyond the first.
    pub extra_attacks: u8,
}

#[derive(Debug, Clone)]
pub struct Class {
    pub index: String,
    pub name: String,
    pub hit_die: u32,
    pub saves: Vec<Ability>,
    /// How many skills to choose, and from which.
    pub skill_picks: usize,
    pub skill_options: Vec<String>,
    pub casting: Option<Ability>,
    /// Level 1 is `levels[0]`.
    pub levels: Vec<Level>,
    /// The gear a new character of this class starts with.
    pub kit: Kit,
}

#[derive(Debug, Clone, Default)]
pub struct Kit {
    pub weapons: Vec<&'static str>,
    pub armor: Option<&'static str>,
    pub shield: bool,
}

#[derive(Debug, Clone)]
pub struct Weapon {
    pub index: String,
    pub name: String,
    pub dice: Dice,
    pub damage_type: String,
    pub ranged: bool,
    pub finesse: bool,
    pub two_handed: bool,
    pub thrown: bool,
    /// Normal and long range in feet; melee weapons reach 5.
    pub range: (u32, u32),
}

#[derive(Debug, Clone)]
pub struct Armor {
    pub index: String,
    pub name: String,
    pub base: i32,
    pub adds_dex: bool,
    pub max_dex: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellAttack {
    Melee,
    Ranged,
}

#[derive(Debug, Clone)]
pub struct Spell {
    pub index: String,
    pub name: String,
    /// 0 is a cantrip.
    pub level: u8,
    pub classes: Vec<String>,
    pub attack: Option<SpellAttack>,
    /// Damage dice by slot level (levelled spells) or by character level (cantrips).
    pub damage: BTreeMap<u8, String>,
    pub damage_by_character_level: bool,
    pub damage_type: String,
    /// The save that resists it, and whether a success still takes half.
    pub save: Option<(Ability, bool)>,
    /// Healing dice by slot level; `MOD` stands for the caster's ability modifier.
    pub heal: BTreeMap<u8, String>,
    /// Feet; touch is 5, self is 0.
    pub range: u32,
    /// Radius or length of the area in feet; 0 for a single target.
    pub area: u32,
    pub concentration: bool,
    pub description: String,
}

impl Spell {
    /// Whether the rules engine can carry the spell out by itself. The others are told to
    /// the game master, who decides what they do.
    pub fn mechanised(&self) -> bool {
        (!self.damage.is_empty() && (self.attack.is_some() || self.save.is_some())) || !self.heal.is_empty()
    }

    /// The dice for a casting with a slot of `slot` (levelled) by a character of `level`
    /// (cantrips): the entry at or below, as the SRD tables are written.
    pub fn dice_at(table: &BTreeMap<u8, String>, key: u8, modifier: i32) -> Option<Dice> {
        table.range(..=key).next_back().and_then(|(_, text)| Dice::parse(text, modifier))
    }
}

#[derive(Debug, Clone)]
pub struct MonsterAttack {
    pub name: String,
    pub bonus: i32,
    pub damage: Vec<Dice>,
    /// Feet: reach for melee, normal range for ranged.
    pub range: u32,
    /// Area attacks (a dragon's breath) are resisted by a save instead of rolled to hit:
    /// ability, difficulty, and whether a success still takes half.
    pub save: Option<(Ability, i32, bool)>,
}

#[derive(Debug, Clone)]
pub struct Monster {
    pub index: String,
    pub name: String,
    pub ac: i32,
    pub hp: i32,
    pub speed: u32,
    pub abilities: [i32; 6],
    pub attacks: Vec<MonsterAttack>,
    /// The attacks made with the Multiattack action, by name, when the monster has one.
    pub multiattack: Vec<(String, u8)>,
    pub challenge: f32,
    pub xp: u32,
    /// Cells on a side: Large is 2, Huge 3, Gargantuan 4.
    pub size: u32,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub index: String,
    pub name: String,
    pub ability: Ability,
}

pub struct Srd {
    pub races: Vec<Race>,
    pub classes: Vec<Class>,
    pub weapons: HashMap<String, Weapon>,
    pub armors: HashMap<String, Armor>,
    pub spells: HashMap<String, Spell>,
    pub monsters: HashMap<String, Monster>,
    pub skills: Vec<Skill>,
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn ability(v: &Value) -> Option<Ability> {
    Ability::from_index(v["index"].as_str()?)
}

fn list(file: &str) -> Vec<Value> {
    serde_json::from_str(file).expect("the SRD files shipped with the game are valid JSON")
}

/// The starting gear of each class: a sensible pick from the options the SRD offers.
fn kit(class: &str) -> Kit {
    let (weapons, armor, shield): (&[&'static str], Option<&'static str>, bool) = match class {
        "barbarian" => (&["greataxe", "handaxe"], None, false),
        "bard" => (&["rapier", "dagger"], Some("leather-armor"), false),
        "cleric" => (&["mace", "crossbow-light"], Some("scale-mail"), true),
        "druid" => (&["scimitar"], Some("leather-armor"), true),
        "fighter" => (&["longsword", "crossbow-light"], Some("chain-mail"), true),
        "monk" => (&["shortsword", "dart"], None, false),
        "paladin" => (&["longsword", "javelin"], Some("chain-mail"), true),
        "ranger" => (&["longbow", "shortsword"], Some("scale-mail"), false),
        "rogue" => (&["rapier", "shortbow", "dagger"], Some("leather-armor"), false),
        "sorcerer" => (&["dagger", "crossbow-light"], None, false),
        "warlock" => (&["crossbow-light", "dagger"], Some("leather-armor"), false),
        _ => (&["quarterstaff", "dagger"], None, false),
    };
    Kit { weapons: weapons.to_vec(), armor, shield }
}

fn load_classes() -> Vec<Class> {
    let levels = list(srd_file!("Levels"));
    list(srd_file!("Classes"))
        .iter()
        .map(|c| {
            let index = text(&c["index"]);
            let mut table = vec![Level::default(); 20];
            let mut improvements = 0;
            let mut own: Vec<&Value> = levels.iter().filter(|l| l["class"]["index"] == index.as_str() && l.get("subclass").is_none()).collect();
            own.sort_by_key(|l| l["level"].as_u64());
            for l in own {
                let n = l["level"].as_u64().unwrap_or(1).clamp(1, 20) as usize;
                // The count only ever grows; a level that omits or understates it keeps the last.
                improvements = improvements.max(l["ability_score_bonuses"].as_u64().unwrap_or(0) as u8);
                let casting = &l["spellcasting"];
                let mut slots = [0u8; 9];
                for (i, slot) in slots.iter_mut().enumerate() {
                    *slot = casting[format!("spell_slots_level_{}", i + 1)].as_u64().unwrap_or(0) as u8;
                }
                table[n - 1] = Level {
                    prof_bonus: l["prof_bonus"].as_i64().unwrap_or(2) as i32,
                    features: l["features"].as_array().map(|f| f.iter().map(|x| text(&x["name"])).collect()).unwrap_or_default(),
                    slots,
                    cantrips: casting["cantrips_known"].as_u64().unwrap_or(0) as u8,
                    spells_known: casting["spells_known"].as_u64().map(|n| n as u8),
                    ability_improvements: improvements,
                    extra_attacks: l["class_specific"]["extra_attacks"].as_u64().unwrap_or(0) as u8,
                };
            }
            let picks = &c["proficiency_choices"][0];
            let skill_options = picks["from"]["options"]
                .as_array()
                .map(|o| o.iter().filter_map(|x| x["item"]["index"].as_str()?.strip_prefix("skill-").map(str::to_string)).collect())
                .unwrap_or_default();
            Class {
                kit: kit(&index),
                name: text(&c["name"]),
                hit_die: c["hit_die"].as_u64().unwrap_or(8) as u32,
                saves: c["saving_throws"].as_array().map(|s| s.iter().filter_map(ability).collect()).unwrap_or_default(),
                skill_picks: picks["choose"].as_u64().unwrap_or(2) as usize,
                skill_options,
                casting: ability(&c["spellcasting"]["spellcasting_ability"]),
                levels: table,
                index,
            }
        })
        .collect()
}

fn load_races() -> Vec<Race> {
    list(srd_file!("Races"))
        .iter()
        .map(|r| {
            let mut bonuses = [0; 6];
            for b in r["ability_bonuses"].as_array().into_iter().flatten() {
                if let Some(a) = ability(&b["ability_score"]) {
                    bonuses[a as usize] = b["bonus"].as_i64().unwrap_or(0) as i32;
                }
            }
            Race { index: text(&r["index"]), name: text(&r["name"]), speed: r["speed"].as_u64().unwrap_or(30) as u32, bonuses }
        })
        .collect()
}

fn load_gear() -> (HashMap<String, Weapon>, HashMap<String, Armor>) {
    let (mut weapons, mut armors) = (HashMap::new(), HashMap::new());
    for e in list(srd_file!("Equipment")) {
        let index = text(&e["index"]);
        match e["equipment_category"]["index"].as_str() {
            Some("weapon") => {
                let has = |property: &str| e["properties"].as_array().is_some_and(|p| p.iter().any(|x| x["index"] == property));
                let Some(dice) = e["damage"]["damage_dice"].as_str().and_then(|d| Dice::parse(d, 0)) else { continue };
                let normal = e["range"]["normal"].as_u64().unwrap_or(5) as u32;
                let thrown = has("thrown");
                let long = if thrown { e["throw_range"]["long"].as_u64() } else { e["range"]["long"].as_u64() };
                weapons.insert(
                    index.clone(),
                    Weapon {
                        index,
                        name: text(&e["name"]),
                        dice,
                        damage_type: text(&e["damage"]["damage_type"]["index"]),
                        ranged: e["weapon_range"] == "Ranged",
                        finesse: has("finesse"),
                        two_handed: has("two-handed"),
                        thrown,
                        range: (normal, long.unwrap_or(normal as u64) as u32),
                    },
                );
            }
            Some("armor") => {
                let ac = &e["armor_class"];
                armors.insert(
                    index.clone(),
                    Armor { index, name: text(&e["name"]), base: ac["base"].as_i64().unwrap_or(10) as i32, adds_dex: ac["dex_bonus"].as_bool().unwrap_or(false), max_dex: ac["max_bonus"].as_i64().map(|n| n as i32) },
                );
            }
            _ => {}
        }
    }
    (weapons, armors)
}

/// The first number followed by "feet"/"ft." in a range text; touch is 5, self is 0.
fn feet(range: &str) -> u32 {
    let lower = range.to_lowercase();
    if lower.starts_with("touch") {
        return 5;
    }
    lower.split(|c: char| !c.is_ascii_digit()).find(|part| !part.is_empty()).and_then(|n| n.parse().ok()).unwrap_or(0)
}

fn dice_table(v: &Value) -> BTreeMap<u8, String> {
    v.as_object().map(|o| o.iter().filter_map(|(k, d)| Some((k.parse().ok()?, d.as_str()?.to_string()))).collect()).unwrap_or_default()
}

fn load_spells() -> HashMap<String, Spell> {
    list(srd_file!("Spells"))
        .iter()
        .map(|s| {
            // Some entries hold the damage in a one-element list, others directly.
            let damage = if s["damage"].is_array() { &s["damage"][0] } else { &s["damage"] };
            let by_slot = dice_table(&damage["damage_at_slot_level"]);
            let by_level = dice_table(&damage["damage_at_character_level"]);
            let spell = Spell {
                index: text(&s["index"]),
                name: text(&s["name"]),
                level: s["level"].as_u64().unwrap_or(0) as u8,
                classes: s["classes"].as_array().map(|c| c.iter().map(|x| text(&x["index"])).collect()).unwrap_or_default(),
                attack: match s["attack_type"].as_str() {
                    Some("melee") => Some(SpellAttack::Melee),
                    Some("ranged") => Some(SpellAttack::Ranged),
                    _ => None,
                },
                damage_by_character_level: by_slot.is_empty() && !by_level.is_empty(),
                damage: if by_slot.is_empty() { by_level } else { by_slot },
                damage_type: text(&damage["damage_type"]["index"]),
                save: ability(&s["dc"]["dc_type"]).map(|a| (a, s["dc"]["dc_success"] == "half")),
                heal: dice_table(&s["heal_at_slot_level"]),
                range: feet(s["range"].as_str().unwrap_or_default()),
                area: s["area_of_effect"]["size"].as_u64().unwrap_or(0) as u32,
                concentration: s["concentration"].as_bool().unwrap_or(false),
                description: s["desc"].as_array().map(|d| d.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n")).unwrap_or_default(),
            };
            (spell.index.clone(), spell)
        })
        .collect()
}

fn load_monsters() -> HashMap<String, Monster> {
    list(srd_file!("Monsters"))
        .iter()
        .map(|m| {
            let actions = m["actions"].as_array().cloned().unwrap_or_default();
            let attacks = actions
                .iter()
                .filter_map(|a| {
                    let damage: Vec<Dice> = a["damage"].as_array()?.iter().filter_map(|d| Dice::parse(d["damage_dice"].as_str()?, 0)).collect();
                    if damage.is_empty() {
                        return None;
                    }
                    let description = a["desc"].as_str().unwrap_or_default();
                    // The data marks some breath weapons as all-or-nothing while their text says
                    // "half as much"; the text is the rule.
                    let half = a["dc"]["success_type"] == "half" || description.contains("half as much");
                    let save = ability(&a["dc"]["dc_type"]).map(|ab| (ab, a["dc"]["dc_value"].as_i64().unwrap_or(10) as i32, half));
                    if a["attack_bonus"].is_null() && save.is_none() {
                        return None;
                    }
                    let range = description.find("range ").or_else(|| description.find("reach ")).map(|at| feet(&description[at + 6..])).filter(|r| *r > 0).unwrap_or(5);
                    Some(MonsterAttack { name: text(&a["name"]), bonus: a["attack_bonus"].as_i64().unwrap_or(0) as i32, damage, range, save })
                })
                .collect();
            let multiattack = actions
                .iter()
                .find(|a| a["name"] == "Multiattack")
                .and_then(|a| a["actions"].as_array())
                .map(|parts| parts.iter().map(|p| (text(&p["action_name"]), p["count"].as_u64().unwrap_or(1) as u8)).collect())
                .unwrap_or_default();
            let score = |name: &str| m[name].as_i64().unwrap_or(10) as i32;
            let monster = Monster {
                index: text(&m["index"]),
                name: text(&m["name"]),
                ac: m["armor_class"][0]["value"].as_i64().unwrap_or(10) as i32,
                hp: m["hit_points"].as_i64().unwrap_or(1) as i32,
                speed: feet(m["speed"]["walk"].as_str().unwrap_or("30 ft.")),
                abilities: [score("strength"), score("dexterity"), score("constitution"), score("intelligence"), score("wisdom"), score("charisma")],
                attacks,
                multiattack,
                challenge: m["challenge_rating"].as_f64().unwrap_or(0.0) as f32,
                xp: m["xp"].as_u64().unwrap_or(0) as u32,
                size: match m["size"].as_str() {
                    Some("Large") => 2,
                    Some("Huge") => 3,
                    Some("Gargantuan") => 4,
                    _ => 1,
                },
            };
            (monster.index.clone(), monster)
        })
        .collect()
}

fn load_skills() -> Vec<Skill> {
    list(srd_file!("Skills")).iter().filter_map(|s| Some(Skill { index: text(&s["index"]), name: text(&s["name"]), ability: ability(&s["ability_score"])? })).collect()
}

/// The rules data, read on first use.
pub fn srd() -> &'static Srd {
    static SRD: OnceLock<Srd> = OnceLock::new();
    SRD.get_or_init(|| {
        let (weapons, armors) = load_gear();
        Srd { races: load_races(), classes: load_classes(), weapons, armors, spells: load_spells(), monsters: load_monsters(), skills: load_skills() }
    })
}

impl Srd {
    pub fn class(&self, index: &str) -> Option<&Class> {
        self.classes.iter().find(|c| c.index == index)
    }

    pub fn race(&self, index: &str) -> Option<&Race> {
        self.races.iter().find(|r| r.index == index)
    }

    pub fn skill(&self, index: &str) -> Option<&Skill> {
        self.skills.iter().find(|s| s.index == index)
    }

    /// The spells a class can cast with slots up to `max_level` (0 for cantrips only),
    /// cantrips first, then by level and name.
    pub fn class_spells(&self, class: &str, max_level: u8) -> Vec<&Spell> {
        let mut spells: Vec<&Spell> = self.spells.values().filter(|s| s.level <= max_level && s.classes.iter().any(|c| c == class)).collect();
        spells.sort_by(|a, b| (a.level, &a.name).cmp(&(b.level, &b.name)));
        spells
    }
}

/// The Russian translation of an SRD string, or the string itself when there is none or
/// the language is not Russian.
pub fn tr(russian: bool, english: &str) -> &str {
    static MEMORY: OnceLock<HashMap<String, String>> = OnceLock::new();
    if !russian {
        return english;
    }
    let memory = MEMORY.get_or_init(|| {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/srd/i18n/ru.tm.jsonl"))
            .lines()
            .filter_map(|line| {
                let entry: Value = serde_json::from_str(line).ok()?;
                Some((entry["src"].as_str()?.to_string(), entry["ru"].as_str()?.to_string()))
            })
            .collect()
    });
    memory.get(english).map_or(english, String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_whole_document_loads() {
        let s = srd();
        assert_eq!((s.races.len(), s.classes.len(), s.skills.len()), (9, 12, 18));
        assert_eq!(s.spells.len(), 319);
        assert_eq!(s.monsters.len(), 334);
        // Thirty-seven weapons are listed; the net deals no damage and is left out.
        assert_eq!(s.weapons.len(), 36);
        assert_eq!(s.armors.len(), 13);
    }

    #[test]
    fn every_class_has_twenty_levels_with_a_rising_proficiency_bonus() {
        for class in &srd().classes {
            assert_eq!(class.levels.len(), 20, "{}", class.index);
            assert_eq!(class.levels[0].prof_bonus, 2, "{}", class.index);
            assert_eq!(class.levels[19].prof_bonus, 6, "{}", class.index);
            assert!(class.levels.windows(2).all(|w| w[1].prof_bonus >= w[0].prof_bonus));
            assert!(class.levels.windows(2).all(|w| w[1].ability_improvements >= w[0].ability_improvements));
            assert!(class.skill_picks >= 2 && class.skill_options.len() >= class.skill_picks, "{}", class.index);
            assert_eq!(class.saves.len(), 2, "{}", class.index);
        }
    }

    #[test]
    fn every_starting_kit_names_gear_that_exists() {
        let s = srd();
        for class in &s.classes {
            assert!(!class.kit.weapons.is_empty());
            for weapon in &class.kit.weapons {
                assert!(s.weapons.contains_key(*weapon), "{}: {weapon}", class.index);
            }
            if let Some(armor) = class.kit.armor {
                assert!(s.armors.contains_key(armor), "{}: {armor}", class.index);
            }
        }
        assert!(s.armors.contains_key("shield"));
    }

    #[test]
    fn a_wizard_gains_slots_and_a_fighter_gains_attacks() {
        let s = srd();
        let wizard = s.class("wizard").unwrap();
        assert_eq!(wizard.casting, Some(Ability::Int));
        assert_eq!((wizard.levels[0].cantrips, wizard.levels[0].slots[0]), (3, 2));
        assert_eq!(wizard.levels[4].slots[..3], [4, 3, 2]);
        assert_eq!(wizard.levels[19].slots[8], 1);
        let fighter = s.class("fighter").unwrap();
        assert_eq!(fighter.casting, None);
        assert_eq!([fighter.levels[3].extra_attacks, fighter.levels[4].extra_attacks, fighter.levels[19].extra_attacks], [0, 1, 3]);
        assert_eq!(s.class("bard").unwrap().levels[0].spells_known, Some(4));
        assert!(fighter.levels[0].features.iter().any(|f| f == "Second Wind"));
    }

    #[test]
    fn weapons_and_armour_carry_what_combat_needs() {
        let s = srd();
        let longsword = &s.weapons["longsword"];
        assert_eq!((longsword.dice, longsword.ranged, longsword.range.0), (Dice::new(1, 8, 0), false, 5));
        let shortbow = &s.weapons["shortbow"];
        assert_eq!((shortbow.ranged, shortbow.range), (true, (80, 320)));
        assert!(s.weapons["rapier"].finesse && s.weapons["greataxe"].two_handed && s.weapons["dagger"].thrown);
        assert_eq!(s.weapons["dagger"].range, (5, 60));
        let chain = &s.armors["chain-mail"];
        assert_eq!((chain.base, chain.adds_dex), (16, false));
        assert_eq!(s.armors["scale-mail"].max_dex, Some(2));
    }

    #[test]
    fn spells_say_how_they_hit_and_what_they_do() {
        let s = srd();
        let bolt = &s.spells["fire-bolt"];
        assert_eq!((bolt.level, bolt.attack, bolt.range, bolt.damage_by_character_level), (0, Some(SpellAttack::Ranged), 120, true));
        assert_eq!(Spell::dice_at(&bolt.damage, 1, 0), Some(Dice::new(1, 10, 0)));
        assert_eq!(Spell::dice_at(&bolt.damage, 7, 0), Some(Dice::new(2, 10, 0)));
        let fireball = &s.spells["fireball"];
        assert_eq!((fireball.level, fireball.save, fireball.area, fireball.range), (3, Some((Ability::Dex, true)), 20, 150));
        assert_eq!(Spell::dice_at(&fireball.damage, 4, 0), Some(Dice::new(9, 6, 0)));
        assert_eq!(Spell::dice_at(&fireball.damage, 2, 0), None);
        let cure = &s.spells["cure-wounds"];
        assert_eq!((cure.range, Spell::dice_at(&cure.heal, 2, 3)), (5, Some(Dice::new(2, 8, 3))));
        assert!(bolt.mechanised() && fireball.mechanised() && cure.mechanised());
        assert!(!s.spells["charm-person"].mechanised());
        assert!(s.class_spells("wizard", 1).iter().any(|sp| sp.index == "magic-missile"));
        assert!(s.class_spells("wizard", 0).iter().all(|sp| sp.level == 0));
    }

    #[test]
    fn monsters_carry_their_attacks_and_multiattacks() {
        let s = srd();
        let goblin = &s.monsters["goblin"];
        assert_eq!((goblin.ac, goblin.hp, goblin.speed, goblin.xp, goblin.size), (15, 7, 30, 50, 1));
        assert_eq!(goblin.attacks[0].bonus, 4);
        assert_eq!(goblin.attacks[0].damage, [Dice::new(1, 6, 2)]);
        assert_eq!((goblin.attacks[0].range, goblin.attacks[1].range), (5, 80));
        let owlbear = &s.monsters["owlbear"];
        assert_eq!(owlbear.multiattack, [("Beak".to_string(), 1), ("Claws".to_string(), 1)]);
        assert_eq!(owlbear.size, 2);
        let dragon = &s.monsters["adult-red-dragon"];
        let breath = dragon.attacks.iter().find(|a| a.name == "Fire Breath").unwrap();
        assert_eq!(breath.save, Some((Ability::Dex, 21, true)));
        assert!(s.monsters.values().all(|m| m.hp > 0 && m.ac > 0));
    }

    #[test]
    fn srd_names_have_russian_translations() {
        assert_eq!(tr(false, "Fireball"), "Fireball");
        assert_ne!(tr(true, "Fireball"), "Fireball");
        assert!(tr(true, "Goblin").chars().any(|c| ('а'..='я').contains(&c) || ('А'..='Я').contains(&c)));
        assert_eq!(tr(true, "not an SRD string"), "not an SRD string");
    }
}
