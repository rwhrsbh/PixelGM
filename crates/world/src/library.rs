//! What a human master makes, kept to be used again in any world: tiles of ground with
//! whether they can be walked, and pictures of creatures and things. A picture is kept with
//! its own colours, so that in a world of another palette it looks the same.

use crate::model::{PALETTE_MAX, World, char_of};
use serde::{Deserialize, Serialize};

/// A tile or a sprite of a master's library.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Art {
    pub id: String,
    pub name: String,
    /// A tile of ground; otherwise a sprite of a creature or a thing.
    pub tile: bool,
    /// Whether a tile can be walked on.
    pub walkable: bool,
    /// Rows of palette characters, as every picture is drawn.
    pub rows: Vec<String>,
    /// The colour behind each palette character the rows use, as red, green, blue.
    pub colors: Vec<(char, [u8; 3])>,
}

impl Art {
    fn with_colors(world: &World, id: &str, name: &str, tile: bool, walkable: bool, rows: &[String]) -> Art {
        let mut colors: Vec<(char, [u8; 3])> = Vec::new();
        for ch in rows.iter().flat_map(|r| r.chars()) {
            if ch == '0' || colors.iter().any(|(c, _)| *c == ch) {
                continue;
            }
            let index = (0..PALETTE_MAX as u8).find(|i| char_of(*i) == ch).unwrap_or(0);
            let [r, g, b, _] = world.palette.rgba(index);
            colors.push((ch, [r, g, b]));
        }
        Art { id: id.to_string(), name: name.to_string(), tile, walkable, rows: rows.to_vec(), colors }
    }

    /// A tile of a world, as a library keeps it.
    pub fn of_tile(world: &World, id: &str) -> Option<Art> {
        let def = world.tiles.get(id)?;
        Some(Self::with_colors(world, id, &def.name, true, def.walkable, &def.sprite.rows))
    }

    /// A sprite of a world, as a library keeps it.
    pub fn of_sprite(world: &World, id: &str) -> Option<Art> {
        let sprite = world.sprites.get(id)?;
        Some(Self::with_colors(world, id, &id.replace('_', " "), false, false, &sprite.rows))
    }
}

/// A creature or an object the master made, with everything the game lets it be: what the
/// `spawn` of it says (all but where), and its picture.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub creature: bool,
    /// The spawn effect without its cell: kind, name, description, attitude, monster or
    /// stats, count, faction, shop; or blocking, contents, lock, key, actions.
    pub spawn: serde_json::Value,
    pub art: Art,
}

impl Entity {
    /// The effect that puts it on a cell; its picture must be in the world already.
    pub fn spawn_at(&self, x: usize, y: usize) -> serde_json::Value {
        let mut spawn = self.spawn.clone();
        spawn["op"] = "spawn".into();
        spawn["kind"] = if self.creature { "creature" } else { "prop" }.into();
        spawn["id"] = crate::build::slug(&self.id).into();
        spawn["sprite"] = crate::build::slug(&self.art.id).into();
        spawn["x"] = x.into();
        spawn["y"] = y.into();
        spawn
    }
}

impl World {
    /// Puts a creature or object of the library on a cell, its picture brought in first.
    /// Gives what went wrong, if anything.
    pub fn place_entity(&mut self, entity: &Entity, x: usize, y: usize) -> Vec<String> {
        if !self.import_art(&entity.art) {
            return vec![format!("the picture of '{}' could not be read", entity.id)];
        }
        let mut notes = Vec::new();
        let effects = crate::story::read_effects(&[entity.spawn_at(x, y)], &mut notes);
        notes.extend(self.apply(&effects, None));
        notes
    }

    /// The palette index of a colour: the same colour where the world has it, a new one
    /// while there is room, otherwise the nearest.
    fn index_for(&mut self, color: [u8; 3]) -> u8 {
        if let Some(i) = self.palette.colors.iter().position(|c| *c == color) {
            return i as u8 + 1;
        }
        if self.palette.len() < PALETTE_MAX {
            self.palette.colors.push(color);
            return self.palette.colors.len() as u8;
        }
        let distance = |c: &[u8; 3]| (0..3).map(|k| (c[k] as i32 - color[k] as i32).pow(2)).sum::<i32>();
        self.palette.colors.iter().enumerate().min_by_key(|(_, c)| distance(c)).map_or(1, |(i, _)| i as u8 + 1)
    }

    /// Brings a tile or a sprite of the library into this world, its colours fitted to the
    /// world's palette. `false` when its picture cannot be read.
    pub fn import_art(&mut self, art: &Art) -> bool {
        let mut map: Vec<(char, char)> = Vec::new();
        for (ch, color) in &art.colors {
            let index = self.index_for(*color);
            map.push((*ch, char_of(index)));
        }
        let rows: Vec<String> = art.rows.iter().map(|r| r.chars().map(|c| map.iter().find(|(from, _)| *from == c).map_or(c, |(_, to)| *to)).collect()).collect();
        if art.tile { self.add_drawn_tile(&art.id, &art.name, art.walkable, &rows) } else { self.add_drawn_sprite(&art.id, &rows) }
    }

    /// Whether a tile can be walked on, as the master sets it. `false` for a tile there is not.
    pub fn set_walkable(&mut self, tile: &str, walkable: bool) -> bool {
        match self.tiles.get_mut(tile) {
            Some(def) => {
                def.walkable = walkable;
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WorldDraft, build_world, fake};

    fn world(name: &str) -> World {
        let draft: WorldDraft = serde_json::from_str(&fake::world(name)).unwrap();
        build_world(&draft, name, "English").unwrap().0
    }

    #[test]
    fn a_tile_kept_in_the_library_comes_into_another_world_looking_the_same() {
        let mut first = world("first");
        let rows: Vec<String> = (0..16).map(|y| (0..16).map(|x| if (x + y) % 2 == 0 { '1' } else { '2' }).collect()).collect();
        assert!(first.add_drawn_tile("log_bridge", "Log bridge", true, &rows));
        let art = Art::of_tile(&first, "log_bridge").unwrap();
        assert!(art.tile && art.walkable && art.colors.len() == 2);
        // A world whose palette is other: the colours are found or added, not mistaken.
        let mut second = world("second");
        second.palette.colors.reverse();
        assert!(second.import_art(&art));
        let def = &second.tiles["log_bridge"];
        assert!(def.walkable);
        let seen = |w: &World, def: &crate::model::TileDef| w.palette.rgba(def.sprite.index_at(0, 0));
        assert_eq!(seen(&second, def), seen(&first, &first.tiles["log_bridge"]), "the same colour in the other world");
        // Through the library's file and back.
        let back: Art = serde_json::from_str(&serde_json::to_string(&art).unwrap()).unwrap();
        assert_eq!(back, art);
    }

    #[test]
    fn a_creature_made_with_every_setting_is_placed_as_made() {
        let mut maker = world("maker");
        let rows: Vec<String> = (0..16).map(|y| (0..16).map(|x| if x > 3 && x < 12 && y > 2 { '2' } else { '0' }).collect()).collect();
        assert!(maker.add_drawn_sprite("bog_merchant", &rows));
        let entity = Entity {
            id: "bog_merchant".into(),
            name: "Bog merchant".into(),
            creature: true,
            spawn: serde_json::json!({
                "name": "Bog merchant", "description": "Sells what the bog gives.", "attitude": "friendly",
                "stats": { "ac": 12, "hp": 18, "speed": 25, "xp": 50, "attacks": [{ "name": "Staff", "bonus": 3, "damage": "1d6+1", "range": 5 }] },
                "count": 1, "shop": [{ "name": "Dried eel", "kind": "food", "price": 3 }]
            }),
            art: Art::of_sprite(&maker, "bog_merchant").unwrap(),
        };
        let back: Entity = serde_json::from_str(&serde_json::to_string(&entity).unwrap()).unwrap();
        let mut w = world("other");
        let (x, y) = w.here().spawn;
        let notes = w.place_entity(&back, x + 2, y);
        assert!(notes.is_empty(), "{notes:?}");
        let made = w.here().creatures.iter().find(|c| c.id == "bog_merchant").expect("it stands there");
        assert_eq!(made.attitude, crate::Attitude::Friendly);
        assert!(made.x.abs_diff(x + 2) + made.y.abs_diff(y) <= 2, "on the cell asked or the nearest free one");
        assert!(made.stock.iter().any(|i| i.name == "Dried eel") || made.wares.iter().any(|i| i.name == "Dried eel"), "it sells what it was given");
        assert!(w.sprites.contains_key("bog_merchant"));
    }

    #[test]
    fn the_master_sets_whether_a_tile_is_walked_on() {
        let mut w = world("walk");
        let id = w.tiles.keys().next().unwrap().clone();
        let was = w.tiles[&id].walkable;
        assert!(w.set_walkable(&id, !was));
        assert_eq!(w.tiles[&id].walkable, !was);
        assert!(!w.set_walkable("no_such_tile", true));
    }
}
