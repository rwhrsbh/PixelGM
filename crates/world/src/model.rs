//! The saved shape of a world.

use crate::party::{PendingCheck, Stats};
use crate::story::{Action, Story};
use rules::character::LevelUp;
use rules::{Character, Encounter};
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
    /// When this is the body of a creature that died: the creature, to rise again if the
    /// story brings it back.
    #[serde(default)]
    pub body: Option<Box<Creature>>,
    /// When this holds things: a chest, a crate, a corpse's purse.
    #[serde(default)]
    pub chest: Option<Chest>,
}

/// What a container holds, and how it is shut.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct Chest {
    pub items: Vec<crate::story::Item>,
    /// How hard the lock is to pick, or the lid to force; none when it is not locked.
    pub lock: i32,
    /// The name of the key that opens it, if any.
    pub key: String,
    pub open: bool,
}

/// Something hidden in a place: a secret door, a cache, a trap. Found by a sharp eye when
/// the party comes near, or — a trap — by stepping on it.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Secret {
    pub x: usize,
    pub y: usize,
    /// How hard it is to notice.
    pub dc: i32,
    /// "hidden" for what is found by looking, "trap" for what goes off underfoot.
    pub kind: String,
    /// What it is, in a few words, told when it is found.
    pub note: String,
    /// What finding it, or setting it off, does.
    #[serde(default)]
    pub effects: Vec<crate::story::Effect>,
    #[serde(default)]
    pub found: bool,
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
    /// The SRD monster whose numbers it fights with, when it is one.
    #[serde(default)]
    pub monster: Option<String>,
    /// Fighting numbers of its own, for creatures the SRD does not have.
    #[serde(default)]
    pub stats: Option<Stats>,
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
    /// The hour and the light of the place: day, dusk, night or dark. Empty is day.
    #[serde(default)]
    pub light: String,
    /// The sky over the place: clear, sun, rain, storm, snow or fog. Empty is clear.
    #[serde(default)]
    pub weather: String,
    /// What has been left on the ground.
    #[serde(default)]
    pub marks: Vec<Mark>,
    /// Things lying on the ground, dropped or left by the story.
    #[serde(default)]
    pub items: Vec<Lying>,
    /// What is hidden here.
    #[serde(default)]
    pub secrets: Vec<Secret>,
    /// The mood of the music here, as the master set it: calm, explore, tense, battle, boss,
    /// mystery, horror, sad, tavern, triumph, dungeon. Empty lets the game choose.
    #[serde(default)]
    pub mood: String,
    /// The cells the party has seen, one row of bits to a row of the map: in the dark, what
    /// was never seen stays black.
    #[serde(default)]
    pub seen: Vec<u32>,
}

/// A thing lying on a cell of a place.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Lying {
    pub x: usize,
    pub y: usize,
    pub item: crate::story::Item,
}

/// Something left on the ground of a cell.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Hash)]
pub struct Mark {
    pub x: usize,
    pub y: usize,
    /// What it is, in the master's words: blood, scorch, puddle, dung, ash…
    pub kind: String,
    /// A colour the master gave, as #rrggbb; empty to take it from the kind.
    #[serde(default)]
    pub color: String,
}

/// A movement someone makes, for every player to see once.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Cue {
    pub seq: u64,
    pub place: String,
    /// A creature or prop id, a hero's name, or "party".
    pub id: String,
    pub motion: String,
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

/// Another player's token: a name and a place.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Token {
    pub name: String,
    pub x: f32,
    pub y: f32,
    /// The place the token is in. Empty means wherever the host is.
    #[serde(default)]
    pub place: String,
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
    /// The player characters.
    #[serde(default)]
    pub party: Vec<Character>,
    /// A check the master called for that has not been rolled yet.
    #[serde(default)]
    pub pending: Option<PendingCheck>,
    /// The fight in progress, if any.
    #[serde(default)]
    pub fight: Option<Encounter>,
    /// Who plays each hero of the party, by player name, in the same order. Empty in a
    /// game played alone.
    #[serde(default)]
    pub owners: Vec<String>,
    /// The players at the table right now. A hero whose player is not among them is away:
    /// it follows the party but takes no part. Empty means nobody is away (a game alone).
    #[serde(default)]
    pub present: Vec<String>,
    /// The player in the game master's seat. Nobody means the model runs the game alone.
    #[serde(default)]
    pub gm: Option<String>,
    /// Where each guest is, by player name, when the party has split up. The host is
    /// always in `current`; a guest not listed is with the host.
    #[serde(default)]
    pub places: std::collections::BTreeMap<String, String>,
    /// While the world is looked at from another place: where the host really is.
    #[serde(skip)]
    pub host_place: Option<String>,
    /// The place the fight in progress is in.
    #[serde(default)]
    pub fight_place: String,
    /// The hero whose player the master is answering, for whose bag a thing goes into.
    #[serde(skip)]
    pub acting: Option<usize>,
    /// A place the story sent heroes to, and which heroes: to be gone to once the telling is
    /// read, each by their own player.
    #[serde(skip)]
    pub travel: Option<(String, Vec<usize>)>,
    /// Cells the story put heroes in, by hero: each player's own token is moved.
    #[serde(skip)]
    pub moves: Vec<(usize, (usize, usize))>,
    /// Movements to be shown, newest last, numbered so that each is shown once.
    #[serde(default)]
    pub cues: Vec<Cue>,
    #[serde(default)]
    pub cue_seq: u64,
    /// The place the check that is waiting was called in.
    #[serde(default)]
    pub pending_place: String,
    /// How this table plays, chosen by the host when the world was made.
    #[serde(default)]
    pub table: TableRules,
    /// Abilities the game master has offered and a player has yet to choose from.
    #[serde(default)]
    pub offers: Vec<FeatOffer>,
    /// Heroes whose unusual deed earned a secret skill the master has yet to write.
    #[serde(default)]
    pub secrets_due: Vec<usize>,
    /// The other players' tokens in the place the party is in. Not saved: they are where
    /// their players last said they were.
    #[serde(skip)]
    pub others: Vec<Token>,
    /// Levels gained since the interface last looked: shown once, not saved.
    #[serde(skip)]
    pub level_ups: Vec<(String, LevelUp)>,
}

fn yes() -> bool {
    true
}

/// The host's choices about the rules of a table.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct TableRules {
    /// Only what the book has: SRD monsters, no invented abilities.
    pub book_only: bool,
    /// The caps that keep a generous master in check are lifted.
    pub no_limits: bool,
    /// How ability scores are made for every character of this world.
    pub method: rules::AbilityMethod,
    /// The 2024 edition of the reference rules instead of the 2014 one. Still a beta.
    pub srd_2024: bool,
}

/// Abilities offered to one hero. The player picks one.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct FeatOffer {
    pub hero: usize,
    pub feats: Vec<rules::Feat>,
}

impl World {
    /// The place a player is in.
    pub fn place_of(&self, player: &str) -> &str {
        self.places.get(player).filter(|place| self.locations.contains_key(*place)).map_or(self.host_place(), String::as_str)
    }

    /// Where the host is, whichever place the world is being looked at from.
    pub fn host_place(&self) -> &str {
        self.host_place.as_deref().unwrap_or(self.current.as_str())
    }

    /// The place a hero is in: where its player is, or with the host when nobody plays it.
    pub fn hero_place(&self, hero: usize) -> &str {
        match self.owners.get(hero) {
            Some(owner) if !owner.is_empty() => self.place_of(owner),
            _ => self.host_place(),
        }
    }

    /// The other players' tokens in the place being looked at.
    pub fn tokens_here(&self) -> impl Iterator<Item = &Token> {
        self.others.iter().filter(|t| t.place.is_empty() || t.place == self.current)
    }

    /// Looks at the world from another place, as a player standing at `at` there: what is
    /// done until `leave` happens in that place. A place that does not exist is not entered.
    pub fn enter(&mut self, place: &str, at: Player) -> (String, Player) {
        let home = (self.current.clone(), self.player);
        if self.locations.contains_key(place) && self.host_place.is_none() {
            self.host_place = Some(self.current.clone());
            self.current = place.to_string();
            self.player = at;
        }
        home
    }

    pub fn leave(&mut self, home: (String, Player)) {
        (self.current, self.player) = home;
        self.host_place = None;
    }

    /// Whether the fight in progress is in the place being looked at.
    pub fn fight_here(&self) -> bool {
        self.fight.is_some() && (self.fight_place.is_empty() || self.fight_place == self.current)
    }

    /// Whether the check that is waiting was called in the place being looked at.
    pub fn pending_here(&self) -> bool {
        self.pending.is_some() && (self.pending_place.is_empty() || self.pending_place == self.current)
    }

    pub fn here(&self) -> &Location {
        &self.locations[&self.current]
    }

    pub fn here_mut(&mut self) -> &mut Location {
        self.locations.get_mut(&self.current).expect("the party is always in a place that exists")
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
