//! Buying and selling. Someone who keeps a shop has a stock the master gave them once; a
//! part of it is laid out each day. Prices are what things are worth, moved by a haggle.

use crate::model::{Creature, World};
use crate::story::Item;
use rules::{Ability, Rng};

/// How many kinds of ware a shop lays out at once.
pub const WARES: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopTrouble {
    /// There is no such shop here, or no such ware.
    Nothing,
    Closed,
    NoGold,
    NoRoom,
    /// The keeper has no use for it: a key, a thing of a quest.
    Unwanted,
}

impl World {
    fn keeper(&self, id: &str) -> Option<&Creature> {
        self.here().creatures.iter().find(|c| c.id == id && !c.stock.is_empty())
    }

    fn keeper_mut(&mut self, id: &str) -> Option<&mut Creature> {
        let here = self.current.clone();
        self.locations.get_mut(&here)?.creatures.iter_mut().find(|c| c.id == id && !c.stock.is_empty())
    }

    /// Whether someone here keeps a shop.
    pub fn keeps_shop(&self, id: &str) -> bool {
        self.keeper(id).is_some()
    }

    /// Shops are shut at night.
    pub fn shop_open(&self) -> bool {
        !self.is_night()
    }

    /// Lays out the day's wares: once a day, a stretch of the stock that moves on each day.
    /// A new day also forgets yesterday's haggling.
    pub fn restock(&mut self, id: &str) {
        let day = self.time().0;
        let Some(keeper) = self.keeper_mut(id) else { return };
        if keeper.shop_day == day {
            return;
        }
        keeper.shop_day = day;
        keeper.haggle = 0;
        keeper.haggled = false;
        let n = keeper.stock.len();
        let start = (day as usize * 3) % n;
        keeper.wares = (0..n.min(WARES)).map(|k| keeper.stock[(start + k) % n].clone()).collect();
    }

    /// What is laid out in a shop, each with what it costs now.
    pub fn wares(&self, id: &str) -> Vec<(&Item, u32)> {
        self.keeper(id).map(|k| k.wares.iter().map(|w| (w, self.asking(k, w))).collect()).unwrap_or_default()
    }

    fn asking(&self, keeper: &Creature, item: &Item) -> u32 {
        (World::worth(item) as i64 * (100 + keeper.haggle as i64 + self.standing_markup(keeper)) / 100).max(1) as u32
    }

    /// How the keeper's regard for the party moves prices, in hundredths: none until the
    /// world keeps standings.
    fn standing_markup(&self, _keeper: &Creature) -> i64 {
        0
    }

    /// What a keeper gives for a thing of the party's: half its worth.
    pub fn offer(&self, id: &str, item: &Item) -> Option<u32> {
        let keeper = self.keeper(id)?;
        let worth = World::worth(item);
        (worth > 0).then(|| ((worth as i64 * (100 - keeper.haggle as i64 - self.standing_markup(keeper)) / 200).max(1)) as u32)
    }

    /// A hero buys one of the ware at place `n`. Gives the name of what was bought.
    pub fn buy(&mut self, hero: usize, id: &str, n: usize) -> Result<String, ShopTrouble> {
        self.ensure_bags();
        if !self.shop_open() {
            return Err(ShopTrouble::Closed);
        }
        let (mut one, price) = {
            let keeper = self.keeper(id).ok_or(ShopTrouble::Nothing)?;
            let ware = keeper.wares.get(n).ok_or(ShopTrouble::Nothing)?;
            (ware.clone(), self.asking(keeper, ware))
        };
        one.count = 1;
        one.at = None;
        let bag = self.story.bags.get_mut(hero).ok_or(ShopTrouble::Nothing)?;
        if bag.gold < price {
            return Err(ShopTrouble::NoGold);
        }
        // More of what is already carried stacks; anything else needs room.
        let stacks = bag.inventory.iter().any(|i| i.name == one.name && i.gear == one.gear);
        if !stacks && bag.free_spot(one.size(), None).is_none() {
            return Err(ShopTrouble::NoRoom);
        }
        bag.gold -= price;
        let name = one.name.clone();
        bag.receive(one);
        if let Some(keeper) = self.keeper_mut(id) {
            if keeper.wares[n].count > 1 {
                keeper.wares[n].count -= 1;
            } else {
                keeper.wares.remove(n);
            }
        }
        Ok(name)
    }

    /// A hero sells one of the thing at place `i` of their bag. Gives its name and the coins.
    pub fn sell(&mut self, hero: usize, id: &str, i: usize) -> Result<(String, u32), ShopTrouble> {
        self.ensure_bags();
        if !self.shop_open() {
            return Err(ShopTrouble::Closed);
        }
        let item = self.story.bags.get(hero).and_then(|b| b.inventory.get(i)).cloned().ok_or(ShopTrouble::Nothing)?;
        let coins = self.offer(id, &item).ok_or(if self.keeps_shop(id) { ShopTrouble::Unwanted } else { ShopTrouble::Nothing })?;
        let bag = &mut self.story.bags[hero];
        if bag.inventory[i].count > 1 {
            bag.inventory[i].count -= 1;
        } else {
            bag.inventory.remove(i);
        }
        bag.gold = bag.gold.saturating_add(coins);
        // It lies among the wares now, to be bought back or by another.
        if let Some(keeper) = self.keeper_mut(id) {
            match keeper.wares.iter_mut().find(|w| w.name == item.name && w.gear == item.gear) {
                Some(ware) => ware.count += 1,
                None => keeper.wares.push(Item { count: 1, at: None, ..item.clone() }),
            }
        }
        Ok((item.name, coins))
    }

    /// One try a day at a better price: Charisma (persuasion) against 12. A bargain takes a
    /// fifth off; a blunder puts a tenth on. `None` when it was already tried today.
    pub fn haggle(&mut self, hero: usize, id: &str, rng: &mut Rng) -> Option<bool> {
        let bonus = self.party.get(hero)?.check_bonus(Ability::Cha, Some("persuasion"));
        let keeper = self.keeper_mut(id)?;
        if keeper.haggled {
            return None;
        }
        keeper.haggled = true;
        let won = rng.die(20) as i32 + bonus >= 12;
        keeper.haggle = if won { -20 } else { 10 };
        Some(won)
    }
}
