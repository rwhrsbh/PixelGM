//! The party's bag as a grid, the way an old dungeon crawler keeps it: each thing takes a
//! block of cells by its shape — a ring one, a sword one by three, a coat of mail two by
//! three — and lies where it was put. Weapons, armour and shields in the bag can be put on a
//! hero, and what a hero wears can be taken off into the bag.

use crate::model::World;
use crate::story::{Item, Story};
use rules::{Character, srd, tr};
use serde::{Deserialize, Serialize};

pub const BAG_W: u8 = 12;
pub const BAG_H: u8 = 10;

/// Where on a hero a thing is worn.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Slot {
    /// The weapon in hand: the first of the hero's weapons.
    Main,
    /// The weapon carried to switch to: the second.
    Spare,
    Shield,
    Body,
}

impl Slot {
    pub const ALL: [Slot; 4] = [Slot::Main, Slot::Spare, Slot::Shield, Slot::Body];
}

/// A thing picked up: from the bag, or off a hero.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub enum Held {
    Item(usize),
    Worn(usize, Slot),
}

/// Where a thing is put down: the cell of the bag its top left corner goes in, or on a hero.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Spot {
    Bag(u8, u8),
    Worn(usize, Slot),
}

/// What kind of gear an index of the rules names.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gear {
    Weapon,
    Armor,
    Shield,
}

pub fn gear_kind(index: &str) -> Option<Gear> {
    if index == "shield" {
        Some(Gear::Shield)
    } else if srd().weapons.contains_key(index) {
        Some(Gear::Weapon)
    } else if srd().armors.contains_key(index) {
        Some(Gear::Armor)
    } else {
        None
    }
}

/// The rules' name of a piece of gear, in the language asked for.
pub fn gear_name(index: &str, russian: bool) -> String {
    let data = srd();
    let english = data.weapons.get(index).map(|w| w.name.as_str()).or_else(|| data.armors.get(index).map(|a| a.name.as_str())).unwrap_or(if index == "shield" { "Shield" } else { index });
    tr(russian, english).to_string()
}

/// The gear a name the master wrote stands for, in either language: "Longsword", "длинный меч".
pub fn gear_named(name: &str) -> Option<String> {
    let wanted = name.trim().to_lowercase();
    if wanted.is_empty() {
        return None;
    }
    if ["shield", "щит"].contains(&wanted.as_str()) {
        return Some("shield".into());
    }
    let data = srd();
    let names = data.weapons.values().map(|w| (&w.index, &w.name)).chain(data.armors.values().map(|a| (&a.index, &a.name)));
    for (index, english) in names {
        if english.to_lowercase() == wanted || tr(true, english).to_lowercase() == wanted {
            return Some(index.clone());
        }
    }
    None
}

const WORDS_1X1: [&str; 30] = [
    "potion", "vial", "flask", "coin", "ring", "key", "gem", "jewel", "amulet", "hair", "tooth", "fang", "seed", "stone", "pebble", "зель", "флакон", "монет", "кольц", "ключ",
    "самоцвет", "камен", "камуш", "амулет", "шерст", "клык", "зуб", "семя", "перо", "feather",
];
const WORDS_1X2: [&str; 14] = ["scroll", "letter", "map", "wand", "torch", "candle", "dagger", "свит", "письм", "карт", "жезл", "факел", "свеч", "кинжал"];
const WORDS_2X2: [&str; 14] = ["book", "tome", "rope", "helm", "box", "bag", "pouch", "head", "книг", "том", "верёв", "веревк", "шлем", "шкатул"];
const WORDS_2X3: [&str; 6] = ["cloak", "armor", "armour", "плащ", "доспех", "кольчуг"];
const WORDS_1X3: [&str; 6] = ["sword", "club", "меч", "дубин", "посох", "staff"];

impl Item {
    /// How many cells across and down the thing takes.
    pub fn size(&self) -> (u8, u8) {
        if let Some(index) = &self.gear {
            return gear_size(index);
        }
        let name = self.name.to_lowercase();
        let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
        if has(&WORDS_2X3) {
            (2, 3)
        } else if has(&WORDS_1X3) {
            (1, 3)
        } else if has(&WORDS_2X2) {
            (2, 2)
        } else if has(&WORDS_1X2) {
            (1, 2)
        } else if has(&WORDS_1X1) {
            (1, 1)
        } else {
            (1, 2)
        }
    }
}

pub fn gear_size(index: &str) -> (u8, u8) {
    let data = srd();
    if index == "shield" {
        return (2, 2);
    }
    if data.armors.contains_key(index) {
        return (2, 3);
    }
    let Some(weapon) = data.weapons.get(index) else { return (1, 2) };
    let i = weapon.index.as_str();
    if i.contains("bow") || i.contains("axe") && weapon.two_handed || i == "maul" || i == "warhammer" || i == "glaive" || i == "halberd" {
        (2, 3)
    } else if i.contains("spear") || i.contains("pike") || i.contains("lance") || i.contains("staff") || i == "trident" || i == "javelin" {
        (1, 4)
    } else if i == "dagger" || i == "sickle" || i == "dart" || i == "sling" || i.contains("hammer") && !weapon.two_handed || i == "club" {
        (1, 2)
    } else if i.contains("axe") || i.contains("mace") || i == "morningstar" || i == "flail" || i.contains("pick") {
        (2, 2)
    } else {
        (1, 3)
    }
}

/// The cells a thing of `size` covers with its corner at `at`.
fn covers(at: (u8, u8), size: (u8, u8), cell: (u8, u8)) -> bool {
    cell.0 >= at.0 && cell.0 < at.0 + size.0 && cell.1 >= at.1 && cell.1 < at.1 + size.1
}

impl Story {
    /// Whether a thing of `size` fits at `at`, leaving out the item `ignore`.
    pub fn fits(&self, size: (u8, u8), at: (u8, u8), ignore: Option<usize>) -> bool {
        if at.0 + size.0 > BAG_W || at.1 + size.1 > BAG_H {
            return false;
        }
        !self.inventory.iter().enumerate().any(|(i, other)| {
            Some(i) != ignore
                && other.at.is_some_and(|o| {
                    let s = other.size();
                    at.0 < o.0 + s.0 && o.0 < at.0 + size.0 && at.1 < o.1 + s.1 && o.1 < at.1 + size.1
                })
        })
    }

    /// The first place a thing of `size` fits, down each column from the left.
    pub fn free_spot(&self, size: (u8, u8), ignore: Option<usize>) -> Option<(u8, u8)> {
        (0..BAG_W).flat_map(|x| (0..BAG_H).map(move |y| (x, y))).find(|at| self.fits(size, *at, ignore))
    }

    /// The item lying on a cell of the bag.
    pub fn item_at(&self, cell: (u8, u8)) -> Option<usize> {
        self.inventory.iter().position(|item| item.at.is_some_and(|at| covers(at, item.size(), cell)))
    }

    /// Gives every thing that has no place in the bag the first place it fits. What does not
    /// fit stays without one and is shown beside the bag.
    pub fn pack(&mut self) {
        for i in 0..self.inventory.len() {
            let misplaced = self.inventory[i].at.is_none_or(|at| !self.fits(self.inventory[i].size(), at, Some(i)));
            if misplaced {
                self.inventory[i].at = None;
                self.inventory[i].at = self.free_spot(self.inventory[i].size(), Some(i));
            }
        }
    }

    /// A piece of gear into the bag, where it fits or beside it.
    pub fn stow(&mut self, index: &str) {
        let item = Item { name: gear_name(index, false), description: String::new(), count: 1, gear: Some(index.to_string()), at: None };
        self.inventory.push(item);
        let last = self.inventory.len() - 1;
        self.inventory[last].at = self.free_spot(self.inventory[last].size(), Some(last));
    }
}

/// What a hero wears in a slot.
pub fn worn(hero: &Character, slot: Slot) -> Option<String> {
    match slot {
        Slot::Main => hero.weapons.first().cloned(),
        Slot::Spare => hero.weapons.get(1).cloned(),
        Slot::Shield => hero.shield.then(|| "shield".to_string()),
        Slot::Body => hero.armor.clone(),
    }
}

fn may_wear(hero: &Character, slot: Slot, index: &str) -> bool {
    let two_handed = |i: &str| srd().weapons.get(i).is_some_and(|w| w.two_handed);
    match (slot, gear_kind(index)) {
        // A two-handed weapon in hand leaves no hand for a shield, and the other way round.
        (Slot::Main, Some(Gear::Weapon)) => !(hero.shield && two_handed(index)),
        (Slot::Spare, Some(Gear::Weapon)) => true,
        (Slot::Shield, Some(Gear::Shield)) => !hero.weapons.first().is_some_and(|w| two_handed(w)),
        (Slot::Body, Some(Gear::Armor)) => true,
        _ => false,
    }
}

fn take_off(hero: &mut Character, slot: Slot) -> Option<String> {
    match slot {
        Slot::Main if !hero.weapons.is_empty() => Some(hero.weapons.remove(0)),
        Slot::Spare if hero.weapons.len() > 1 => Some(hero.weapons.remove(1)),
        Slot::Shield if hero.shield => {
            hero.shield = false;
            Some("shield".into())
        }
        Slot::Body => hero.armor.take(),
        _ => None,
    }
}

fn put_on(hero: &mut Character, slot: Slot, index: String) {
    match slot {
        Slot::Main => hero.weapons.insert(0, index),
        Slot::Spare => hero.weapons.insert(1.min(hero.weapons.len()), index),
        Slot::Shield => hero.shield = true,
        Slot::Body => hero.armor = Some(index),
    }
}

impl World {
    /// Moves a thing from where it was picked up to where it is put. Nothing changes when the
    /// move cannot be made: a thing that does not fit, gear that does not go in that slot.
    /// A thing put where another is worn swaps with it.
    pub fn arrange(&mut self, held: &Held, to: Spot) -> bool {
        match (held.clone(), to) {
            (Held::Item(i), Spot::Bag(x, y)) => {
                let Some(item) = self.story.inventory.get(i) else { return false };
                if !self.story.fits(item.size(), (x, y), Some(i)) {
                    return false;
                }
                self.story.inventory[i].at = Some((x, y));
                true
            }
            (Held::Item(i), Spot::Worn(h, slot)) => {
                let Some(index) = self.story.inventory.get(i).and_then(|item| item.gear.clone()) else { return false };
                let Some(hero) = self.party.get_mut(h) else { return false };
                let mut trial = hero.clone();
                let old = take_off(&mut trial, slot);
                if !may_wear(&trial, slot, &index) {
                    return false;
                }
                // What was worn goes where the new thing lay, or wherever it fits.
                let had = self.story.inventory.remove(i);
                if let Some(old) = &old {
                    self.story.stow(old);
                    let last = self.story.inventory.len() - 1;
                    if let Some(at) = had.at
                        && self.story.fits(gear_size(old), at, Some(last))
                    {
                        self.story.inventory[last].at = Some(at);
                    }
                }
                put_on(&mut trial, slot, index);
                *self.party.get_mut(h).expect("checked above") = trial;
                true
            }
            (Held::Worn(h, slot), Spot::Bag(x, y)) => {
                let Some(index) = self.party.get(h).and_then(|hero| worn(hero, slot)) else { return false };
                if !self.story.fits(gear_size(&index), (x, y), None) {
                    return false;
                }
                let hero = self.party.get_mut(h).expect("checked above");
                take_off(hero, slot);
                self.story.stow(&index);
                let last = self.story.inventory.len() - 1;
                self.story.inventory[last].at = Some((x, y));
                true
            }
            (Held::Worn(h, from), Spot::Worn(to_hero, to)) if h == to_hero => {
                // Between the two weapon slots the weapons change places.
                let Some(hero) = self.party.get_mut(h) else { return false };
                if matches!((from, to), (Slot::Main, Slot::Spare) | (Slot::Spare, Slot::Main)) && hero.weapons.len() > 1 {
                    hero.weapons.swap(0, 1);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Heroes carry two weapons in reach; what else they started with goes into the bag.
    pub fn tidy_gear(&mut self) {
        for h in 0..self.party.len() {
            while self.party[h].weapons.len() > 2 {
                let extra = self.party[h].weapons.remove(2);
                self.story.stow(&extra);
            }
        }
        self.story.pack();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let draft: crate::WorldDraft = serde_json::from_str(&crate::fake::world("bag")).unwrap();
        let mut w = crate::build_world(&draft, "bag", "English").unwrap().0;
        let fighter = srd().class("fighter").unwrap();
        let skills: Vec<String> = fighter.skill_options.iter().take(fighter.skill_picks).cloned().collect();
        w.party.push(Character::create("Ann", "", "human", "fighter", [15, 14, 13, 12, 10, 8], &skills, &[]).unwrap());
        w
    }

    fn named(name: &str) -> Item {
        Item { name: name.into(), description: String::new(), count: 1, gear: gear_named(name), at: None }
    }

    #[test]
    fn things_take_cells_by_their_shape() {
        assert_eq!(named("Ash ring").size(), (1, 1));
        assert_eq!(named("Old map").size(), (1, 2));
        assert_eq!(named("Longsword").size(), (1, 3));
        assert_eq!(named("Chain mail").size(), (2, 3));
        assert_eq!(named("Shield").size(), (2, 2));
        assert_eq!(named("Зелье лечения").size(), (1, 1));
    }

    #[test]
    fn the_masters_words_for_gear_are_read_in_either_language() {
        assert_eq!(gear_named("longsword").as_deref(), Some("longsword"));
        assert_eq!(gear_named("Щит").as_deref(), Some("shield"));
        assert_eq!(gear_named("A strange idol"), None);
    }

    #[test]
    fn things_given_fill_the_bag_down_each_column_and_never_overlap() {
        let mut w = world();
        for name in ["Longsword", "Ash ring", "Chain mail", "Old map", "Shield"] {
            w.story.inventory.push(named(name));
        }
        w.story.pack();
        let at: Vec<_> = w.story.inventory.iter().map(|i| i.at.unwrap()).collect();
        assert_eq!(at, [(0, 0), (0, 3), (0, 4), (0, 7), (1, 0)]);
        for x in 0..BAG_W {
            for y in 0..BAG_H {
                let on = w.story.inventory.iter().filter(|i| covers(i.at.unwrap(), i.size(), (x, y))).count();
                assert!(on <= 1, "two things on {x},{y}");
            }
        }
    }

    #[test]
    fn a_thing_moves_only_where_it_fits() {
        let mut w = world();
        w.story.inventory = vec![named("Longsword"), named("Ash ring")];
        w.story.pack();
        assert!(!w.arrange(&Held::Item(1), Spot::Bag(0, 2)), "on the sword");
        assert!(!w.arrange(&Held::Item(0), Spot::Bag(11, BAG_H - 2)), "past the bottom");
        assert!(w.arrange(&Held::Item(0), Spot::Bag(11, BAG_H - 3)));
        assert_eq!(w.story.item_at((11, BAG_H - 1)), Some(0));
    }

    #[test]
    fn armour_put_on_changes_the_heros_armour_class_and_the_old_coat_goes_in_the_bag() {
        let mut w = world();
        let before = w.party[0].ac();
        assert_eq!(w.party[0].armor.as_deref(), Some("chain-mail"));
        w.story.inventory = vec![named("Leather armor")];
        w.story.pack();
        assert!(w.arrange(&Held::Item(0), Spot::Worn(0, Slot::Body)));
        assert_eq!(w.party[0].armor.as_deref(), Some("leather-armor"));
        assert!(w.party[0].ac() < before);
        assert_eq!(w.story.inventory.len(), 1);
        assert_eq!(w.story.inventory[0].gear.as_deref(), Some("chain-mail"));
        assert!(w.story.inventory[0].at.is_some());
    }

    #[test]
    fn a_weapon_taken_off_lies_where_it_was_put_and_a_ring_cannot_be_wielded() {
        let mut w = world();
        let sword = w.party[0].weapons[0].clone();
        assert!(w.arrange(&Held::Worn(0, Slot::Main), Spot::Bag(5, 2)));
        assert!(!w.party[0].weapons.contains(&sword));
        assert_eq!(w.story.item_at((5, 4)), Some(0));
        w.story.inventory.push(named("Ash ring"));
        w.story.pack();
        assert!(!w.arrange(&Held::Item(1), Spot::Worn(0, Slot::Main)));
        assert!(w.arrange(&Held::Item(0), Spot::Worn(0, Slot::Main)));
        assert_eq!(w.party[0].weapons[0], sword);
    }

    #[test]
    fn a_shield_and_a_two_handed_weapon_do_not_go_together() {
        let mut w = world();
        assert!(w.party[0].shield);
        w.story.inventory = vec![named("Greatsword")];
        w.story.pack();
        assert!(!w.arrange(&Held::Item(0), Spot::Worn(0, Slot::Main)));
        assert!(w.arrange(&Held::Worn(0, Slot::Shield), Spot::Bag(6, 0)));
        assert!(w.arrange(&Held::Item(0), Spot::Worn(0, Slot::Main)));
    }

    #[test]
    fn the_two_weapon_slots_swap() {
        let mut w = world();
        let (a, b) = (w.party[0].weapons[0].clone(), w.party[0].weapons[1].clone());
        assert!(w.arrange(&Held::Worn(0, Slot::Spare), Spot::Worn(0, Slot::Main)));
        assert_eq!(w.party[0].weapons[..2], [b, a]);
    }

    #[test]
    fn heroes_keep_two_weapons_in_reach_and_the_rest_go_in_the_bag() {
        let mut w = world();
        w.party[0].weapons.push("handaxe".into());
        w.party[0].weapons.push("dagger".into());
        w.tidy_gear();
        assert_eq!(w.party[0].weapons.len(), 2);
        assert_eq!(w.story.inventory.iter().filter(|i| i.gear.is_some()).count(), 2);
    }
}
