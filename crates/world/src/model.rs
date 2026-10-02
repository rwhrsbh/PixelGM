//! The saved shape of a world.

use crate::story::{Action, Story};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Side of a map cell in pixels.
pub const TILE: u32 = 16;
/// A location fills the screen exactly: 30 by 16 cells, with a strip left for the interface.
pub const MAP_W: usize = 30;
pub const MAP_H: usize = 16;

/// Most colours a world may use, counting the transparent index 0.
pub const PALETTE_MAX: usize = 32;

/// Palette indices are written as one character each: `0`–`9`, then `a`–`v`.
pub fn index_of(ch: char) -> Option<u8> {
    match ch {
        '0'..='9' => Some(ch as u8 - b'0'),
        'a'..='v' => Some(ch as u8 - b'a' + 10),
        _ => None,
    }
}

pub fn char_of(index: u8) -> char {
    if index < 10 { (b'0' + index) as char } else { (b'a' + index - 10) as char }
}

/// The colours of a world. Index 0 is always transparent and is not stored.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Palette {
    /// Colour of index 1, 2, … as red, green, blue.
    pub colors: Vec<[u8; 3]>,
}

impl Palette {
    /// Number of indices in use, counting index 0.
    pub fn len(&self) -> usize {
        self.colors.len() + 1
    }

    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    /// The colour of an index; transparent for 0 and for indices past the end.
    pub fn rgba(&self, index: u8) -> [u8; 4] {
        match index.checked_sub(1).and_then(|i| self.colors.get(i as usize)) {
            Some([r, g, b]) => [*r, *g, *b, 255],
            None => [0, 0, 0, 0],
        }
    }
}

/// A picture as rows of palette index characters, all rows the same length.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Sprite {
    pub rows: Vec<String>,
}

impl Sprite {
    pub fn width(&self) -> u32 {
        self.rows.first().map_or(0, |r| r.len() as u32)
    }

    pub fn height(&self) -> u32 {
        self.rows.len() as u32
    }

    pub fn index_at(&self, x: u32, y: u32) -> u8 {
        self.rows.get(y as usize).and_then(|r| r.as_bytes().get(x as usize)).and_then(|b| index_of(*b as char)).unwrap_or(0)
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct TileDef {
    pub name: String,
    pub walkable: bool,
    pub sprite: Sprite,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Prop {
    pub id: String,
    pub name: String,
    pub description: String,
    pub x: usize,
    pub y: usize,
    pub blocking: bool,
    pub sprite: String,
    /// What the players can do with it without asking the game master.
    #[serde(default)]
    pub actions: Vec<Action>,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Attitude {
    Friendly,
    #[default]
    Neutral,
    Hostile,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Creature {
    pub id: String,
    pub name: String,
    pub description: String,
    pub x: usize,
    pub y: usize,
    pub sprite: String,
    pub attitude: Attitude,
    #[serde(default)]
    pub actions: Vec<Action>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Exit {
    pub x: usize,
    pub y: usize,
    /// Id of the location, drawn or still a stub, this exit leads to.
    pub to: String,
    pub label: String,
}

/// A place that is known to exist but has not been drawn yet.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Stub {
    pub name: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Location {
    pub name: String,
    pub description: String,
    /// One character per cell; `legend` says which tile a character stands for.
    pub map: Vec<String>,
    pub legend: BTreeMap<char, String>,
    pub spawn: (usize, usize),
    pub props: Vec<Prop>,
    pub creatures: Vec<Creature>,
    pub exits: Vec<Exit>,
}

impl Location {
    pub fn tile_id(&self, x: usize, y: usize) -> Option<&str> {
        let ch = *self.map.get(y)?.as_bytes().get(x)? as char;
        self.legend.get(&ch).map(String::as_str)
    }

    /// Whether a cell cannot be walked into: outside the map, a solid tile, or something
    /// standing there.
    pub fn blocked(&self, x: i32, y: i32, tiles: &BTreeMap<String, TileDef>) -> bool {
        if x < 0 || y < 0 || x >= MAP_W as i32 || y >= MAP_H as i32 {
            return true;
        }
        let (x, y) = (x as usize, y as usize);
        let solid = self.tile_id(x, y).and_then(|id| tiles.get(id)).is_none_or(|t| !t.walkable);
        solid || self.props.iter().any(|p| p.blocking && p.x == x && p.y == y) || self.creatures.iter().any(|c| c.x == x && c.y == y)
    }

    pub fn exit_at(&self, x: usize, y: usize) -> Option<&Exit> {
        self.exits.iter().find(|e| e.x == x && e.y == y)
    }
}

/// Where the player stands, in pixels from the top-left corner of the location.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct Player {
    pub x: f32,
    pub y: f32,
}

impl Player {
    /// Standing in the middle of a cell.
    pub fn in_cell(x: usize, y: usize) -> Self {
        Self { x: (x as f32 + 0.5) * TILE as f32, y: (y as f32 + 0.5) * TILE as f32 }
    }

    pub fn cell(&self) -> (i32, i32) {
        ((self.x / TILE as f32).floor() as i32, (self.y / TILE as f32).floor() as i32)
    }
}

pub const SAVE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct World {
    pub version: u32,
    pub title: String,
    /// The setting in a few sentences, sent to the game master with every request.
    pub summary: String,
    pub tone: String,
    /// What the host asked for when creating the world; empty when it was left to the master.
    pub request: String,
    /// Language the master writes names and descriptions in.
    pub language: String,
    pub palette: Palette,
    pub tiles: BTreeMap<String, TileDef>,
    pub sprites: BTreeMap<String, Sprite>,
    pub locations: BTreeMap<String, Location>,
    pub stubs: BTreeMap<String, Stub>,
    pub current: String,
    pub player: Player,
    /// Inventory, the story so far and what must be remembered.
    #[serde(default)]
    pub story: Story,
    /// Whether the table wants graphic violence and sexual content in the telling. Both are
    /// on unless the host turned them off when creating the world.
    #[serde(default = "yes")]
    pub violence: bool,
    #[serde(default = "yes")]
    pub sex: bool,
}

fn yes() -> bool {
    true
}

impl World {
    pub fn here(&self) -> &Location {
        &self.locations[&self.current]
    }

    /// Name of a place, drawn or not.
    pub fn place_name<'a>(&'a self, id: &'a str) -> &'a str {
        self.locations.get(id).map(|l| l.name.as_str()).or_else(|| self.stubs.get(id).map(|s| s.name.as_str())).unwrap_or(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_characters_round_trip_over_the_whole_palette() {
        for i in 0..PALETTE_MAX as u8 {
            assert_eq!(index_of(char_of(i)), Some(i));
        }
        assert_eq!(index_of('w'), None);
        assert_eq!(index_of('A'), None);
    }

    #[test]
    fn palette_index_zero_and_missing_indices_are_transparent() {
        let p = Palette { colors: vec![[10, 20, 30]] };
        assert_eq!(p.rgba(0), [0, 0, 0, 0]);
        assert_eq!(p.rgba(1), [10, 20, 30, 255]);
        assert_eq!(p.rgba(2), [0, 0, 0, 0]);
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn sprite_reads_indices_and_treats_outside_as_transparent() {
        let s = Sprite { rows: vec!["0a".into(), "v1".into()] };
        assert_eq!((s.width(), s.height()), (2, 2));
        assert_eq!([s.index_at(1, 0), s.index_at(0, 1), s.index_at(5, 5)], [10, 31, 0]);
    }

    #[test]
    fn player_cell_follows_the_pixel_position() {
        let p = Player::in_cell(3, 2);
        assert_eq!((p.x, p.y), (56.0, 40.0));
        assert_eq!(p.cell(), (3, 2));
        assert_eq!(Player { x: -1.0, y: 0.0 }.cell(), (-1, 0));
    }
}
