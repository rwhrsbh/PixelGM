//! What every world starts with before the game master draws anything: a palette whose first
//! fifteen colours have fixed jobs, and a small set of tiles drawn in those colours.
//!
//! The master recolours the palette to suit the world (purple grass, red sand) and the base
//! tiles follow, so even a world it never drew a tile for looks like its own place.

use crate::model::{Palette, Sprite, TILE, TileDef, char_of};
use std::collections::BTreeMap;

/// The jobs of palette indices 1–15, as told to the game master.
pub const SLOTS: [(&str, [u8; 3]); 15] = [
    ("outline, the darkest colour", [0x1b, 0x14, 0x12]),
    ("shadow", [0x3a, 0x30, 0x36]),
    ("highlight, the lightest colour", [0xf2, 0xea, 0xd3]),
    ("bare ground, dark", [0x6b, 0x4a, 0x2f]),
    ("bare ground, light", [0x9c, 0x78, 0x4e]),
    ("plants, dark", [0x2f, 0x5a, 0x2c]),
    ("plants, light", [0x5a, 0x8f, 0x3c]),
    ("stone, dark", [0x55, 0x55, 0x60]),
    ("stone, light", [0x8a, 0x8a, 0x92]),
    ("wood, dark", [0x4c, 0x2e, 0x1b]),
    ("wood, light", [0x8a, 0x5a, 0x32]),
    ("water, dark", [0x24, 0x4a, 0x7a]),
    ("water, light", [0x4a, 0x86, 0xb8]),
    ("accent, the colour this world is remembered by", [0xc8, 0x96, 0x3c]),
    ("skin or pale cloth", [0xe0, 0xb0, 0x8a]),
];

// Slot indices, named for the drawing code below.
const OUTLINE: u8 = 1;
const SHADOW: u8 = 2;
const LIGHT: u8 = 3;
const GROUND_DARK: u8 = 4;
const GROUND: u8 = 5;
const PLANT_DARK: u8 = 6;
const PLANT: u8 = 7;
const STONE_DARK: u8 = 8;
const STONE: u8 = 9;
const WOOD_DARK: u8 = 10;
const WOOD: u8 = 11;
const WATER_DARK: u8 = 12;
const WATER: u8 = 13;
const ACCENT: u8 = 14;
const SKIN: u8 = 15;

pub fn default_palette() -> Palette {
    Palette { colors: SLOTS.iter().map(|(_, c)| *c).collect() }
}

struct Grid {
    cells: [[u8; TILE as usize]; TILE as usize],
}

impl Grid {
    fn filled(index: u8) -> Self {
        Self { cells: [[index; TILE as usize]; TILE as usize] }
    }

    fn set(&mut self, x: i32, y: i32, index: u8) {
        if (0..TILE as i32).contains(&x) && (0..TILE as i32).contains(&y) {
            self.cells[y as usize][x as usize] = index;
        }
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, index: u8) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, index);
            }
        }
    }

    /// A filled blob: every cell within `r` of the centre, with the given outline around it.
    fn blob(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, fill: u8, edge: u8) {
        for y in 0..TILE as i32 {
            for x in 0..TILE as i32 {
                let d = ((x as f32 + 0.5 - cx) / rx).powi(2) + ((y as f32 + 0.5 - cy) / ry).powi(2);
                if d <= 1.0 {
                    self.set(x, y, if d > 0.62 { edge } else { fill });
                }
            }
        }
    }

    fn sprite(&self) -> Sprite {
        Sprite { rows: self.cells.iter().map(|row| row.iter().map(|i| char_of(*i)).collect()).collect() }
    }
}

/// Repeatable speckle: the same tile looks the same on every machine and in every save.
fn speck(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA6B) ^ seed.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 13)
}

fn speckled(fill: u8, fleck: u8, one_in: u32, seed: u32) -> Grid {
    let mut g = Grid::filled(fill);
    for y in 0..TILE as i32 {
        for x in 0..TILE as i32 {
            if speck(x, y, seed) % one_in == 0 {
                g.set(x, y, fleck);
            }
        }
    }
    g
}

fn grass() -> Grid {
    let mut g = speckled(PLANT, PLANT_DARK, 9, 1);
    // A few blades: a dark pixel with a lighter one above it.
    for (x, y) in [(3, 4), (11, 2), (7, 10), (13, 12), (2, 13)] {
        g.set(x, y, PLANT_DARK);
        g.set(x, y - 1, LIGHT);
    }
    g
}

fn stone_floor() -> Grid {
    let mut g = speckled(STONE, STONE_DARK, 23, 2);
    for i in 0..TILE as i32 {
        g.set(i, 0, STONE_DARK);
        g.set(i, 8, STONE_DARK);
    }
    for y in 0..8 {
        g.set(0, y, STONE_DARK);
        g.set(8, y + 8, STONE_DARK);
    }
    g
}

fn wood_floor() -> Grid {
    let mut g = speckled(WOOD, WOOD_DARK, 19, 3);
    for y in [0, 5, 10, 15] {
        g.rect(0, y, TILE as i32, 1, WOOD_DARK);
    }
    for (x, y) in [(5, 1), (12, 6), (3, 11)] {
        g.rect(x, y, 1, 4, WOOD_DARK);
    }
    g
}

fn water() -> Grid {
    let mut g = Grid::filled(WATER_DARK);
    for (x, y, w) in [(1, 3, 5), (9, 6, 6), (3, 10, 4), (10, 13, 5)] {
        g.rect(x, y, w, 1, WATER);
    }
    g.set(4, 2, LIGHT);
    g.set(12, 5, LIGHT);
    g
}

fn stone_wall() -> Grid {
    let mut g = Grid::filled(STONE_DARK);
    // Courses of brick with staggered joints, a lit top edge and a shadowed foot.
    for course in 0..4 {
        let y = course * 4;
        g.rect(0, y, TILE as i32, 1, OUTLINE);
        let shift = if course % 2 == 0 { 0 } else { 4 };
        for x in [shift, shift + 8] {
            g.rect(x, y, 1, 4, OUTLINE);
        }
        g.rect(0, y + 1, TILE as i32, 1, STONE);
        for x in [shift, shift + 8] {
            g.set(x, y + 1, OUTLINE);
        }
    }
    g.rect(0, 15, TILE as i32, 1, SHADOW);
    g
}

fn wood_wall() -> Grid {
    let mut g = Grid::filled(WOOD_DARK);
    for x in [0, 4, 8, 12] {
        g.rect(x, 0, 1, TILE as i32, OUTLINE);
        g.rect(x + 1, 0, 1, TILE as i32, WOOD);
    }
    g.rect(0, 0, TILE as i32, 1, OUTLINE);
    g.rect(0, 15, TILE as i32, 1, SHADOW);
    g
}

fn tree() -> Grid {
    let mut g = grass();
    g.rect(7, 10, 2, 5, WOOD_DARK);
    g.set(8, 11, WOOD);
    g.blob(8.0, 6.5, 6.5, 6.0, PLANT_DARK, OUTLINE);
    g.blob(7.0, 5.5, 3.5, 3.0, PLANT, PLANT);
    g.set(5, 4, LIGHT);
    g
}

fn bush() -> Grid {
    let mut g = grass();
    g.blob(8.0, 10.0, 5.5, 4.5, PLANT_DARK, OUTLINE);
    g.blob(7.0, 9.0, 2.5, 2.0, PLANT, PLANT);
    g.set(10, 11, ACCENT);
    g.set(5, 10, ACCENT);
    g
}

fn rock() -> Grid {
    let mut g = speckled(GROUND, GROUND_DARK, 11, 4);
    g.blob(8.0, 9.5, 5.5, 4.5, STONE_DARK, OUTLINE);
    g.blob(7.0, 8.5, 3.0, 2.2, STONE, STONE);
    g.set(6, 7, LIGHT);
    g
}

fn door() -> Grid {
    let mut g = Grid::filled(WOOD);
    g.rect(0, 0, TILE as i32, 1, OUTLINE);
    g.rect(0, 0, 2, TILE as i32, WOOD_DARK);
    g.rect(14, 0, 2, TILE as i32, WOOD_DARK);
    g.rect(7, 1, 1, 15, WOOD_DARK);
    g.set(10, 9, ACCENT);
    g.set(10, 10, OUTLINE);
    g
}

fn hero() -> Grid {
    let mut g = Grid::filled(0);
    // Head, hair, tunic in the world's accent colour, legs, an outline all round.
    g.rect(5, 1, 6, 6, OUTLINE);
    g.rect(6, 2, 4, 4, SKIN);
    g.rect(6, 2, 4, 1, WOOD_DARK);
    g.set(7, 4, OUTLINE);
    g.set(9, 4, OUTLINE);
    g.rect(4, 7, 8, 6, OUTLINE);
    g.rect(5, 7, 6, 5, ACCENT);
    g.rect(5, 11, 6, 1, WOOD_DARK);
    g.set(4, 9, SKIN);
    g.set(11, 9, SKIN);
    g.rect(5, 13, 6, 3, OUTLINE);
    g.rect(6, 13, 1, 2, SHADOW);
    g.rect(9, 13, 1, 2, SHADOW);
    g
}

fn marker() -> Grid {
    let mut g = Grid::filled(0);
    // A tied sack: what stands in for a thing the master named but did not draw.
    g.blob(8.0, 10.0, 5.0, 5.0, GROUND, OUTLINE);
    g.rect(6, 4, 4, 2, OUTLINE);
    g.rect(7, 4, 2, 1, ACCENT);
    g.set(7, 9, LIGHT);
    g
}

/// Id of the tile used when the master names one that does not exist.
pub const FALLBACK_TILE: &str = "grass";
pub const HERO_SPRITE: &str = "hero";
/// Id of the sprite used when the master names one it did not draw.
pub const MARKER_SPRITE: &str = "marker";

/// The built-in tiles: id, description for the master, whether it can be walked on.
pub fn base_tiles() -> BTreeMap<String, TileDef> {
    let tiles: [(&str, &str, bool, Grid); 12] = [
        ("grass", "grass", true, grass()),
        ("dirt", "bare earth", true, speckled(GROUND, GROUND_DARK, 7, 5)),
        ("sand", "sand", true, speckled(GROUND, LIGHT, 13, 6)),
        ("stone_floor", "flagstones", true, stone_floor()),
        ("wood_floor", "floorboards", true, wood_floor()),
        ("door", "a doorway", true, door()),
        ("water", "deep water", false, water()),
        ("stone_wall", "a stone wall", false, stone_wall()),
        ("wood_wall", "a wooden wall", false, wood_wall()),
        ("tree", "a tree", false, tree()),
        ("bush", "a thick bush", false, bush()),
        ("rock", "a boulder", false, rock()),
    ];
    tiles.into_iter().map(|(id, name, walkable, grid)| (id.to_string(), TileDef { name: name.to_string(), walkable, sprite: grid.sprite() })).collect()
}

pub fn base_sprites() -> BTreeMap<String, Sprite> {
    [(HERO_SPRITE, hero()), (MARKER_SPRITE, marker())].into_iter().map(|(id, grid)| (id.to_string(), grid.sprite())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_base_picture_is_one_cell_and_uses_only_the_fixed_slots() {
        let sprites = base_tiles().into_values().map(|t| t.sprite).chain(base_sprites().into_values());
        for sprite in sprites {
            assert_eq!((sprite.width(), sprite.height()), (TILE, TILE));
            for y in 0..TILE {
                for x in 0..TILE {
                    assert!(sprite.index_at(x, y) as usize <= SLOTS.len());
                }
            }
        }
    }

    #[test]
    fn tiles_cover_the_whole_cell_and_sprites_keep_a_transparent_surround() {
        for tile in base_tiles().values() {
            assert!((0..TILE).all(|y| (0..TILE).all(|x| tile.sprite.index_at(x, y) != 0)), "{} has holes", tile.name);
        }
        for sprite in base_sprites().values() {
            assert_eq!(sprite.index_at(0, 0), 0);
        }
    }

    #[test]
    fn the_fallback_tile_exists_and_can_be_walked_on() {
        assert!(base_tiles()[FALLBACK_TILE].walkable);
        assert!(base_sprites().contains_key(HERO_SPRITE) && base_sprites().contains_key(MARKER_SPRITE));
    }

    #[test]
    fn default_palette_has_a_colour_for_every_slot() {
        assert_eq!(default_palette().len(), SLOTS.len() + 1);
    }
}
