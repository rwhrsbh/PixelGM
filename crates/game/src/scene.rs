//! The picture of a place. The game master says what stands where — which tile in which
//! cell, in which colours — and this module paints it: ground that runs unbroken from cell
//! to cell, edges where grass spills over earth, walls with a face and a shadow, water that
//! moves, trees that rise above their cell, and the light of the hour.
//!
//! Everything is drawn at twice the resolution the master draws in, from the world's own
//! palette, so a world of purple grass and red sand is painted in purple and red.

use crate::art::Pixels;
use crate::theme::{CLEAR, Rgba};
use world::base;
use world::{Location, MAP_H, MAP_W, World};

/// Picture pixels to a pixel of the world's own measure.
pub const PX: i32 = 2;
/// The side of a cell in picture pixels.
pub const CELL: i32 = world::TILE as i32 * PX;
pub const WATER_FRAMES: usize = 4;

const BAYER: [[u32; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

fn hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA6B) ^ seed.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

fn unit(x: i32, y: i32, seed: u32) -> f32 {
    (hash(x, y, seed) & 0xffff) as f32 / 65_535.0
}

/// Smooth noise between 0 and 1: values on a grid of `size` pixels, blended between.
fn cloud(x: i32, y: i32, size: i32, seed: u32) -> f32 {
    let (gx, gy) = (x.div_euclid(size), y.div_euclid(size));
    let (fx, fy) = (x.rem_euclid(size) as f32 / size as f32, y.rem_euclid(size) as f32 / size as f32);
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    let (fx, fy) = (ease(fx), ease(fy));
    let top = unit(gx, gy, seed) * (1.0 - fx) + unit(gx + 1, gy, seed) * fx;
    let bottom = unit(gx, gy + 1, seed) * (1.0 - fx) + unit(gx + 1, gy + 1, seed) * fx;
    top * (1.0 - fy) + bottom * fy
}

/// A threshold that turns a smooth value into a pattern of flat pixels.
/// Smooth noise from 0 to 1 that repeats every `wrap` pixels across, with blobs about
/// `size` pixels wide: for layers that slide and loop.
pub fn cloud_noise(x: i32, y: i32, wrap: i32, size: i32) -> f32 {
    let cells = (wrap / size).max(1);
    let (gx, gy) = (x.div_euclid(size), y.div_euclid(size));
    let (fx, fy) = ((x.rem_euclid(size)) as f32 / size as f32, (y.rem_euclid(size)) as f32 / size as f32);
    let corner = |cx: i32, cy: i32| unit(cx.rem_euclid(cells), cy, 77);
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (smooth(fx), smooth(fy));
    let top = corner(gx, gy) * (1.0 - sx) + corner(gx + 1, gy) * sx;
    let bottom = corner(gx, gy + 1) * (1.0 - sx) + corner(gx + 1, gy + 1) * sx;
    top * (1.0 - sy) + bottom * sy
}

fn dither(x: i32, y: i32) -> f32 {
    (BAYER[(y & 3) as usize][(x & 3) as usize] as f32 + 0.5) / 16.0
}

pub fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let blend = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    [blend(a[0], b[0]), blend(a[1], b[1]), blend(a[2], b[2]), a[3].max(b[3])]
}

pub fn shade(c: Rgba, factor: f32) -> Rgba {
    let scale = |v: u8| (v as f32 * factor).round().clamp(0.0, 255.0) as u8;
    [scale(c[0]), scale(c[1]), scale(c[2]), c[3]]
}

/// A touch more contrast and colour than the palette as written: the grade of the picture.
fn grade(c: Rgba) -> Rgba {
    if c[3] == 0 {
        return c;
    }
    let [r, g, b] = [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0];
    let grey = r * 0.3 + g * 0.59 + b * 0.11;
    let fix = |v: f32| {
        let saturated = grey + (v - grey) * 1.12;
        let curved = (saturated - 0.5) * 1.07 + 0.5;
        (curved.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    [fix(r), fix(g), fix(b), c[3]]
}

/// The colours of the world's fixed palette jobs, and the tones painted between them.
struct Paint {
    outline: Rgba,
    shadow: Rgba,
    light: Rgba,
    ground_dark: Rgba,
    ground: Rgba,
    plant_dark: Rgba,
    plant: Rgba,
    stone_dark: Rgba,
    stone: Rgba,
    wood_dark: Rgba,
    wood: Rgba,
    water_dark: Rgba,
    water: Rgba,
    accent: Rgba,
}

impl Paint {
    fn of(world: &World) -> Self {
        let c = |slot: u8| world.palette.rgba(slot);
        Self {
            outline: c(base::OUTLINE),
            shadow: c(base::SHADOW),
            light: c(base::LIGHT),
            ground_dark: c(base::GROUND_DARK),
            ground: c(base::GROUND),
            plant_dark: c(base::PLANT_DARK),
            plant: c(base::PLANT),
            stone_dark: c(base::STONE_DARK),
            stone: c(base::STONE),
            wood_dark: c(base::WOOD_DARK),
            wood: c(base::WOOD),
            water_dark: c(base::WATER_DARK),
            water: c(base::WATER),
            accent: c(base::ACCENT),
        }
    }
}

/// What the ground of a cell is made of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ground {
    Grass,
    Dirt,
    Sand,
    StoneFloor,
    WoodFloor,
    Door,
    Water,
    StoneWall,
    WoodWall,
    /// A tile the master drew: painted as drawn, only enlarged.
    Drawn,
}

/// What rises from the ground of a cell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Growth {
    Tree,
    Bush,
    Rock,
}

impl Ground {
    /// Which ground spills over which where they meet: the higher over the lower.
    fn rank(self) -> i32 {
        match self {
            Ground::Grass => 5,
            Ground::Dirt => 4,
            Ground::Sand => 3,
            Ground::StoneFloor => 2,
            Ground::WoodFloor => 1,
            _ => 0,
        }
    }

    fn wall(self) -> bool {
        matches!(self, Ground::StoneWall | Ground::WoodWall)
    }
}

/// What a tile the master made is, by the words of its id and name, in either language. A
/// kind that is walked on is taken only for a tile that can be walked on, and a wall, water or
/// something standing only for one that cannot: the picture must never say otherwise than the
/// rules.
fn classify(id: &str, def: &world::TileDef) -> Option<(Ground, Option<Growth>)> {
    let words = format!("{} {}", id, def.name).to_lowercase().replace(['_', '-'], " ");
    let has = |list: &[&str]| list.iter().any(|w| words.contains(w));
    let wooden = has(&["wood", "plank", "board", "timber", "log", "pier", "dock", "deck", "деревян", "доск", "бревн", "причал", "настил", "пристан"]);
    let tree = !wooden && (has(&["tree", "forest", "лес"]) || words.contains("дерево") || words.contains("деревья"));
    let solid = !def.walkable;
    let found = if has(&["water", "sea", "river", "lake", "pond", "ocean", "вода", "воды", "море", "река", "реки", "озер", "пруд", "болот"]) {
        Some((Ground::Water, None))
    } else if has(&["door", "gate", "дверь", "двери", "ворота", "проход"]) {
        Some((Ground::Door, None))
    } else if tree || has(&["bush", "shrub", "hedge", "куст", "rock", "boulder", "валун", "глыба", "idol", "statue", "pillar", "column", "статуя", "колонн", "столб"]) {
        // Something standing the master drew for this world stays the master's drawing:
        // a stock green tree on a floor of flesh is wrong.
        None
    } else if has(&["wall", "стена", "стены", "стен ", "cliff", "скала", "утёс", "fence", "забор", "частокол", "palisade"]) {
        Some((if wooden || has(&["fence", "забор", "частокол", "palisade"]) { Ground::WoodWall } else { Ground::StoneWall }, None))
    } else if wooden {
        Some((if solid { Ground::WoodWall } else { Ground::WoodFloor }, None))
    } else if has(&["stone", "cobble", "brick", "flag", "pave", "tile", "marble", "slab", "камен", "камн", "брусчат", "булыжн", "плит", "кирпич", "мостов", "мрамор"]) {
        Some((if solid { Ground::StoneWall } else { Ground::StoneFloor }, None))
    } else if has(&["grass", "meadow", "lawn", "moss", "трав", "луг", "мох", "газон"]) {
        Some((Ground::Grass, None))
    } else if has(&["dirt", "mud", "path", "road", "earth", "soil", "trail", "земл", "гряз", "дорог", "троп", "почв"]) {
        Some((Ground::Dirt, None))
    } else if has(&["sand", "beach", "dune", "песок", "песч", "пляж", "дюн"]) {
        Some((Ground::Sand, None))
    } else {
        None
    };
    let (ground, growth) = found?;
    let walked_on = growth.is_none() && matches!(ground, Ground::Grass | Ground::Dirt | Ground::Sand | Ground::StoneFloor | Ground::WoodFloor | Ground::Door);
    (walked_on != solid).then_some((ground, growth))
}

struct Cells {
    ground: Vec<Ground>,
    growth: Vec<Option<Growth>>,
    /// For drawn tiles: the id of the tile.
    drawn: Vec<Option<String>>,
    /// Cells nobody can walk into, by what the tile says of itself.
    solid: Vec<bool>,
}

impl Cells {
    fn read(location: &Location, world: &World) -> Self {
        let stock = base::base_tiles();
        let mut cells = Self { ground: Vec::new(), growth: Vec::new(), drawn: Vec::new(), solid: Vec::new() };
        for cy in 0..MAP_H {
            for cx in 0..MAP_W {
                let id = location.tile_id(cx, cy).unwrap_or(base::FALLBACK_TILE);
                // A stock tile the master has redrawn under the same name is the master's.
                let stock_look = world.tiles.get(id).zip(stock.get(id)).is_some_and(|(now, then)| now.sprite == then.sprite);
                let (ground, growth) = match (id, stock_look) {
                    ("grass", true) => (Ground::Grass, None),
                    ("dirt", true) => (Ground::Dirt, None),
                    ("sand", true) => (Ground::Sand, None),
                    ("stone_floor", true) => (Ground::StoneFloor, None),
                    ("wood_floor", true) => (Ground::WoodFloor, None),
                    ("door", true) => (Ground::Door, None),
                    ("water", true) => (Ground::Water, None),
                    ("stone_wall", true) => (Ground::StoneWall, None),
                    ("wood_wall", true) => (Ground::WoodWall, None),
                    ("tree", true) => (Ground::Grass, Some(Growth::Tree)),
                    ("bush", true) => (Ground::Grass, Some(Growth::Bush)),
                    ("rock", true) => (Ground::Grass, Some(Growth::Rock)),
                    // A tile the master made is painted like the stock ground it is, when its
                    // name says what it is; only what cannot be told is shown as drawn.
                    _ => world.tiles.get(id).and_then(|def| classify(id, def)).unwrap_or((Ground::Drawn, None)),
                };
                cells.drawn.push((ground == Ground::Drawn).then(|| id.to_string()));
                cells.solid.push(world.tiles.get(id).is_none_or(|t| !t.walkable));
                cells.ground.push(ground);
                cells.growth.push(growth);
            }
        }
        cells
    }

    /// Whether a cell stands up from the ground: a wall, or a drawn tile nobody can walk into.
    /// Such a cell shows a face and throws a shadow, so that what blocks the way looks it.
    fn raised(&self, cx: i32, cy: i32) -> bool {
        if cx < 0 || cy < 0 || cx >= MAP_W as i32 || cy >= MAP_H as i32 {
            return false;
        }
        let at = cy as usize * MAP_W + cx as usize;
        self.ground[at].wall() || self.ground[at] == Ground::Drawn && self.solid[at]
    }

    /// The ground of a cell; past the edge of the map the nearest cell goes on.
    fn at(&self, cx: i32, cy: i32) -> Ground {
        let (x, y) = (cx.clamp(0, MAP_W as i32 - 1) as usize, cy.clamp(0, MAP_H as i32 - 1) as usize);
        self.ground[y * MAP_W + x]
    }
}

// ---- what each kind of ground looks like at one pixel ----

fn grass(p: &Paint, x: i32, y: i32) -> Rgba {
    let patch = cloud(x, y, 22, 11) * 0.7 + cloud(x, y, 7, 12) * 0.3;
    let mut c = if patch > 0.5 + dither(x, y) * 0.22 {
        mix(p.plant, p.light, 0.1)
    } else if patch < 0.3 + dither(x, y) * 0.2 {
        mix(p.plant, p.plant_dark, 0.55)
    } else {
        p.plant
    };
    // Blades: a dark stroke with a light tip.
    if hash(x, y, 13) % 41 == 0 {
        c = p.plant_dark;
    } else if hash(x, y + 1, 13) % 41 == 0 {
        c = mix(p.plant, p.light, 0.45);
    }
    // A flower now and then: two pixels of the world's own colour on a dark stalk.
    let (fx, fy) = (x.div_euclid(2), y.div_euclid(2));
    if hash(fx, fy, 14) % 190 == 0 {
        c = if hash(fx, fy, 15) % 3 == 0 { p.light } else { p.accent };
    } else if hash(fx, fy - 1, 14) % 190 == 0 && y.rem_euclid(2) == 0 {
        c = p.plant_dark;
    }
    c
}

fn dirt(p: &Paint, x: i32, y: i32) -> Rgba {
    let patch = cloud(x, y, 18, 21) * 0.65 + cloud(x, y, 5, 22) * 0.35;
    let mut c = if patch < 0.36 + dither(x, y) * 0.2 { mix(p.ground, p.ground_dark, 0.6) } else if patch > 0.72 { mix(p.ground, p.light, 0.14) } else { p.ground };
    // Pebbles: a light top over a dark foot.
    let (px, py) = (x.div_euclid(2), y);
    if hash(px, py, 23) % 97 == 0 {
        c = mix(p.stone, p.ground, 0.3);
    } else if hash(px, py - 1, 23) % 97 == 0 {
        c = p.ground_dark;
    }
    c
}

fn sand(p: &Paint, x: i32, y: i32) -> Rgba {
    let pale = mix(p.ground, p.light, 0.42);
    // Ripples the wind left: long shallow waves.
    let wave = (y as f32 + (x as f32 * 0.21).sin() * 2.6 + cloud(x, y, 30, 31) * 9.0).rem_euclid(9.0);
    let mut c = if wave < 1.0 { mix(pale, p.ground, 0.5) } else if wave < 2.0 { mix(pale, p.light, 0.25) } else { pale };
    if hash(x, y, 32) % 53 == 0 {
        c = mix(pale, p.ground_dark, 0.35);
    }
    c
}

fn stone_floor(p: &Paint, x: i32, y: i32) -> Rgba {
    // Flagstones in courses, every other course shifted by half a stone.
    const W: i32 = 16;
    const H: i32 = 16;
    let course = y.div_euclid(H);
    let shifted = x + if course.rem_euclid(2) == 1 { W / 2 } else { 0 };
    let (slab, in_x, in_y) = (shifted.div_euclid(W), shifted.rem_euclid(W), y.rem_euclid(H));
    if in_x == 0 || in_y == 0 {
        return mix(p.stone_dark, p.outline, 0.35);
    }
    let tone = unit(slab, course, 41);
    let mut c = mix(p.stone_dark, p.stone, 0.55 + tone * 0.45);
    if in_x == 1 || in_y == 1 {
        c = mix(c, p.light, 0.22);
    } else if in_x == W - 1 || in_y == H - 1 {
        c = mix(c, p.stone_dark, 0.5);
    }
    // Wear: specks, and a crack across a stone now and then.
    if hash(x, y, 42) % 29 == 0 {
        c = mix(c, p.stone_dark, 0.45);
    }
    if hash(slab, course, 43) % 5 == 0 && (in_x + in_y / 2 == 9 || (in_x + in_y / 2 == 10 && in_y > 6)) {
        c = p.stone_dark;
    }
    c
}

fn wood_floor(p: &Paint, x: i32, y: i32) -> Rgba {
    const BOARD: i32 = 8;
    let board = y.div_euclid(BOARD);
    let within = y.rem_euclid(BOARD);
    let joint = (hash(board, 0, 51) % 64) as i32;
    if within == 0 || (x + joint).rem_euclid(64) == 0 {
        return mix(p.wood_dark, p.outline, 0.4);
    }
    let tone = unit(board, (x + joint).div_euclid(64), 52);
    let mut c = mix(p.wood_dark, p.wood, 0.5 + tone * 0.5);
    if within == 1 {
        c = mix(c, p.light, 0.16);
    }
    // Grain: long thin streaks along the board.
    if hash(x.div_euclid(11 + (hash(board, within, 53) % 9) as i32), y, 54) % 5 == 0 && within > 1 {
        c = mix(c, p.wood_dark, 0.4);
    }
    c
}

fn door(p: &Paint, x: i32, y: i32) -> Rgba {
    let (in_x, in_y) = (x.rem_euclid(CELL), y.rem_euclid(CELL));
    // A threshold of worn boards between two posts.
    if !(4..CELL - 4).contains(&in_x) {
        let post = if in_x < 2 || in_x >= CELL - 2 { p.outline } else { p.wood_dark };
        return if in_y < 2 { mix(post, p.light, 0.2) } else { post };
    }
    let board = in_x / 6;
    let mut c = mix(p.wood, p.light, 0.1 + unit(board, y.div_euclid(CELL), 61) * 0.18);
    if in_x % 6 == 4 {
        c = p.wood_dark;
    }
    if in_y < 2 || in_y >= CELL - 2 {
        c = mix(p.accent, p.wood_dark, 0.35);
    }
    c
}

fn water(p: &Paint, x: i32, y: i32, frame: usize) -> Rgba {
    let drift = frame as i32 * 3;
    let deep = cloud(x + drift, y, 26, 71);
    let mut c = mix(p.water_dark, p.water, 0.18 + deep * 0.3);
    // Streaks of lighter water that slide along, each row at its own pace.
    let row = y.div_euclid(5);
    let slide = drift * (1 + (hash(row, 0, 72) % 2) as i32);
    let along = (x + slide + (hash(row, 1, 72) % 40) as i32).rem_euclid(40);
    if y.rem_euclid(5) == (hash(row, 2, 72) % 5) as i32 && along < 9 + (hash(row, 3, 72) % 8) as i32 {
        c = mix(p.water, p.light, if along < 3 { 0.45 } else { 0.12 });
    }
    // Glints that come and go.
    if hash(x, y, 73 + frame as u32) % 331 == 0 {
        c = p.light;
    }
    c
}

/// How close a pixel of a cell is to each of its sides: left, right, top, bottom.
fn sides(x: i32, y: i32) -> [(i32, i32, i32); 4] {
    let (ix, iy) = (x.rem_euclid(CELL), y.rem_euclid(CELL));
    [(-1, 0, ix), (1, 0, CELL - 1 - ix), (0, -1, iy), (0, 1, CELL - 1 - iy)]
}

fn stone_wall(p: &Paint, cells: &Cells, x: i32, y: i32) -> Rgba {
    let (cx, cy) = (x.div_euclid(CELL), y.div_euclid(CELL));
    let in_y = y.rem_euclid(CELL);
    const CAP: i32 = 14;
    let faces_south = !cells.at(cx, cy + 1).wall();
    if faces_south && in_y >= CAP {
        // The face: courses of brick, lit from above, dark at the foot.
        const BRICK_W: i32 = 12;
        const BRICK_H: i32 = 6;
        let face_y = in_y - CAP;
        let course = face_y / BRICK_H;
        let shifted = x + if course % 2 == 1 { BRICK_W / 2 } else { 0 };
        if face_y % BRICK_H == 0 || shifted.rem_euclid(BRICK_W) == 0 {
            return mix(p.stone_dark, p.outline, 0.6);
        }
        let tone = unit(shifted.div_euclid(BRICK_W), course + cy * 7, 81);
        let mut c = mix(p.stone_dark, p.stone, 0.35 + tone * 0.5);
        if face_y % BRICK_H == 1 {
            c = mix(c, p.light, 0.18);
        }
        if in_y >= CELL - 3 {
            c = mix(c, p.outline, 0.45);
        }
        return c;
    }
    // The top of the wall: dark, with a lit rim where it ends.
    let mut c = mix(p.stone_dark, p.outline, 0.45 + cloud(x, y, 9, 82) * 0.15);
    if hash(x, y, 83) % 23 == 0 {
        c = mix(c, p.stone, 0.4);
    }
    for (dx, dy, near) in sides(x, y) {
        if dy == 1 && faces_south {
            continue;
        }
        if !cells.at(cx + dx, cy + dy).wall() && near < 2 {
            c = if near == 0 { mix(p.stone, p.light, 0.3) } else { p.stone };
        }
    }
    if faces_south && in_y == CAP - 1 {
        c = mix(p.stone, p.light, 0.35);
    }
    c
}

fn wood_wall(p: &Paint, cells: &Cells, x: i32, y: i32) -> Rgba {
    let (cx, cy) = (x.div_euclid(CELL), y.div_euclid(CELL));
    let in_y = y.rem_euclid(CELL);
    const CAP: i32 = 14;
    let faces_south = !cells.at(cx, cy + 1).wall();
    if faces_south && in_y >= CAP {
        // The face: upright planks under a beam.
        const PLANK: i32 = 8;
        if x.rem_euclid(PLANK) == 0 {
            return mix(p.wood_dark, p.outline, 0.55);
        }
        let tone = unit(x.div_euclid(PLANK), cy, 91);
        let mut c = mix(p.wood_dark, p.wood, 0.4 + tone * 0.5);
        if x.rem_euclid(PLANK) == 1 {
            c = mix(c, p.light, 0.15);
        }
        if in_y < CAP + 3 {
            c = mix(p.wood_dark, p.outline, if in_y == CAP + 2 { 0.6 } else { 0.25 });
        }
        if in_y >= CELL - 3 {
            c = mix(c, p.outline, 0.45);
        }
        if hash(x, y, 92) % 37 == 0 {
            c = mix(c, p.wood_dark, 0.5);
        }
        return c;
    }
    let mut c = mix(p.wood_dark, p.outline, 0.4 + cloud(x, y, 8, 93) * 0.15);
    for (dx, dy, near) in sides(x, y) {
        if dy == 1 && faces_south {
            continue;
        }
        if !cells.at(cx + dx, cy + dy).wall() && near < 2 {
            c = if near == 0 { mix(p.wood, p.light, 0.25) } else { p.wood };
        }
    }
    if faces_south && in_y == CAP - 1 {
        c = mix(p.wood, p.light, 0.3);
    }
    c
}

fn open_ground(p: &Paint, ground: Ground, x: i32, y: i32, frame: usize) -> Option<Rgba> {
    Some(match ground {
        Ground::Grass => grass(p, x, y),
        Ground::Dirt => dirt(p, x, y),
        Ground::Sand => sand(p, x, y),
        Ground::StoneFloor => stone_floor(p, x, y),
        Ground::WoodFloor => wood_floor(p, x, y),
        Ground::Door => door(p, x, y),
        Ground::Water => water(p, x, y, frame),
        _ => return None,
    })
}

/// The ragged reach of a higher ground into a lower one, in pixels, at a point along an edge.
fn reach(along: i32, across_seed: i32) -> i32 {
    2 + (cloud(along, across_seed * 37, 5, 101) * 5.0) as i32
}

struct Painter<'a> {
    paint: Paint,
    cells: Cells,
    world: &'a World,
    /// Tiles the master drew, enlarged: one picture for each id.
    drawn: std::collections::HashMap<String, Pixels>,
}

impl Painter<'_> {
    fn pixel(&self, x: i32, y: i32, frame: usize) -> Rgba {
        let p = &self.paint;
        let (cx, cy) = (x.div_euclid(CELL), y.div_euclid(CELL));
        let here = self.cells.at(cx, cy);
        let mut c = match here {
            Ground::StoneWall => stone_wall(p, &self.cells, x, y),
            Ground::WoodWall => wood_wall(p, &self.cells, x, y),
            Ground::Drawn => {
                let id = self.cells.drawn[cy as usize * MAP_W + cx as usize].as_deref().unwrap_or_default();
                self.drawn.get(id).map_or(p.outline, |picture| picture.get(x.rem_euclid(CELL), y.rem_euclid(CELL)))
            }
            ground => open_ground(p, ground, x, y, frame).unwrap_or(p.outline),
        };
        if here.wall() {
            return c;
        }
        let (ix, iy) = (x.rem_euclid(CELL), y.rem_euclid(CELL));
        if here == Ground::Drawn && self.cells.raised(cx, cy) {
            // A block: a lit top edge, and a front face in shadow where nothing raised stands
            // in front of it.
            if !self.cells.raised(cx, cy + 1) && iy >= CELL - 7 {
                let face = mix(c, mix(p.shadow, p.outline, 0.4), 0.5);
                return if iy == CELL - 7 { mix(face, p.outline, 0.5) } else if iy == CELL - 1 { mix(face, p.outline, 0.6) } else { face };
            }
            if !self.cells.raised(cx, cy - 1) && iy < 2 {
                return mix(c, p.light, 0.35);
            }
            if !self.cells.raised(cx - 1, cy) && ix == 0 || !self.cells.raised(cx + 1, cy) && ix == CELL - 1 {
                return mix(c, p.outline, 0.45);
            }
            return c;
        }
        // A higher ground next door spills over the edge, raggedly, with a darker lip.
        let mut best: Option<(Ground, i32)> = None;
        for (dx, dy, near) in sides(x, y) {
            let next = self.cells.at(cx + dx, cy + dy);
            if next.rank() > here.rank() && (here.rank() > 0 || here == Ground::Drawn) || (here == Ground::Water && next.rank() > 0) {
                let along = if dx != 0 { y } else { x };
                let depth = reach(along, if dx != 0 { cx * 2 + dx } else { cy * 2 + dy + 1000 });
                if near < depth && best.is_none_or(|(_, n)| near < n) {
                    best = Some((next, near - depth));
                }
            }
        }
        // Corners: a higher ground touching only diagonally still rounds the corner off.
        if best.is_none() {
            for (dx, dy) in [(-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let next = self.cells.at(cx + dx, cy + dy);
                if !(next.rank() > here.rank() && (here.rank() > 0 || here == Ground::Water || here == Ground::Drawn)) {
                    continue;
                }
                let (nx, ny) = (if dx < 0 { ix } else { CELL - 1 - ix }, if dy < 0 { iy } else { CELL - 1 - iy });
                let depth = reach(x + y, cx * 3 + cy * 5);
                if nx * nx + ny * ny < depth * depth {
                    best = Some((next, -1));
                }
            }
        }
        if let Some((over, edge)) = best {
            c = open_ground(p, over, x, y, frame).unwrap_or(c);
            if edge == -1 {
                // The lip: where the overhang ends it is darker, and wet by the water.
                c = mix(c, if here == Ground::Water { p.ground_dark } else { p.outline }, 0.35);
            }
        } else if here == Ground::Water {
            // Foam along the bank: a pale line that breathes with the frames.
            for (dx, dy, near) in sides(x, y) {
                let next = self.cells.at(cx + dx, cy + dy);
                if next != Ground::Water {
                    let along = if dx != 0 { y } else { x };
                    let lap = 7 + ((along / 3 + frame as i32) % 4) + (cloud(along, 0, 6, 102) * 3.0) as i32;
                    if near == lap || (near == lap - 1 && (along + frame as i32) % 3 != 0) {
                        c = mix(p.water, p.light, 0.6);
                    } else if near < lap {
                        c = mix(c, p.water, 0.5);
                    }
                }
            }
        }
        // Walls throw a shadow on the ground at their foot and to their side.
        if here != Ground::Water {
            let mut dark: f32 = 0.0;
            if self.cells.raised(cx, cy - 1) {
                dark = dark.max(0.55 - iy as f32 * 0.055);
            }
            if self.cells.raised(cx - 1, cy) {
                dark = dark.max(0.4 - ix as f32 * 0.07);
            }
            if dark > 0.0 {
                let steps = (dark * 4.0 + dither(x, y) - 0.5).round().max(0.0) / 4.0;
                c = mix(c, mix(p.shadow, p.outline, 0.5), steps * 0.8);
            }
        }
        c
    }
}

/// Enlarges a picture twofold, keeping edges sharp and diagonals smooth (the EPX rule): a
/// drawing of sixteen pixels a side becomes one of thirty-two without turning to blocks.
pub fn enlarge(small: &Pixels) -> Pixels {
    let mut big = Pixels::new(small.w * 2, small.h * 2, CLEAR);
    for y in 0..small.h as i32 {
        for x in 0..small.w as i32 {
            let p = small.get(x, y);
            let (a, b, c, d) = (small.get_or(x, y - 1, p), small.get_or(x + 1, y, p), small.get_or(x - 1, y, p), small.get_or(x, y + 1, p));
            let (mut e0, mut e1, mut e2, mut e3) = (p, p, p, p);
            if c == a && c != d && a != b {
                e0 = a;
            }
            if a == b && a != c && b != d {
                e1 = b;
            }
            if d == c && d != b && c != a {
                e2 = c;
            }
            if b == d && b != a && d != c {
                e3 = d;
            }
            big.set(x * 2, y * 2, e0);
            big.set(x * 2 + 1, y * 2, e1);
            big.set(x * 2, y * 2 + 1, e2);
            big.set(x * 2 + 1, y * 2 + 1, e3);
        }
    }
    big
}

/// A soft shadow on the ground: an ellipse of flat dark pixels, thinning at the rim.
fn ground_shadow(picture: &mut Pixels, cx: i32, cy: i32, rx: f32, ry: f32, strength: f32) {
    for y in (cy as f32 - ry) as i32..=(cy as f32 + ry) as i32 {
        for x in (cx as f32 - rx) as i32..=(cx as f32 + rx) as i32 {
            let d = ((x - cx) as f32 / rx).powi(2) + ((y - cy) as f32 / ry).powi(2);
            if d <= 1.0 && (1.0 - d) * 1.6 + 0.25 > dither(x, y) {
                let under = picture.get(x, y);
                picture.set(x, y, shade(under, 1.0 - strength));
            }
        }
    }
}

/// A thing standing at `foot` mirrored into the water below it, blended in where there is
/// water, wavering a pixel with the beats of the water.
fn reflect(picture: &mut Pixels, cells: &Cells, thing: &Pixels, foot: (i32, i32), beat: usize) {
    let (w, h) = (thing.w as i32, thing.h as i32);
    let left = foot.0 - w / 2;
    for y in 0..h {
        // Every fourth row is broken, as on moving water.
        if y % 4 == 3 {
            continue;
        }
        for x in 0..w {
            let c = thing.get(x, h - 1 - y);
            if c[3] == 0 {
                continue;
            }
            let wobble = if (y / 3 + beat as i32) % 4 == 0 { PX as i32 / 2 } else { 0 };
            let (tx, ty) = (left + x + wobble, foot.1 + y);
            if ty >= MAP_H as i32 * CELL || cells.at(tx.div_euclid(CELL), ty.div_euclid(CELL)) != Ground::Water {
                continue;
            }
            let strength = 0.42 * (1.0 - y as f32 / h as f32 * 0.6);
            let under = picture.get(tx, ty);
            picture.set(tx, ty, mix(under, mix(c, [60, 90, 150, 255], 0.35), strength));
        }
    }
}

/// The roofs of the buildings in a place: every walled area that cannot be walked into from
/// the edge of the map without going through a door. A place that is mostly one building has
/// no roof: the party is already inside it.
fn roofs(cells: &Cells, p: &Paint, exits: &[(usize, usize)]) -> Vec<Roof> {
    let (w, h) = (MAP_W as i32, MAP_H as i32);
    let blocks = |x: i32, y: i32| cells.at(x, y).wall() || cells.at(x, y) == Ground::Door;
    // Everything reached from the edge, around walls and doors, is outside.
    let mut outside = vec![false; (w * h) as usize];
    let mut queue: Vec<(i32, i32)> = (0..w).flat_map(|x| [(x, 0), (x, h - 1)]).chain((0..h).flat_map(|y| [(0, y), (w - 1, y)])).filter(|(x, y)| !blocks(*x, *y)).collect();
    for (x, y) in &queue {
        outside[(y * w + x) as usize] = true;
    }
    while let Some((x, y)) = queue.pop() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= w || ny >= h || outside[(ny * w + nx) as usize] || blocks(nx, ny) {
                continue;
            }
            outside[(ny * w + nx) as usize] = true;
            queue.push((nx, ny));
        }
    }
    // The rest falls into buildings: each open area inside, with its doors and walls.
    let mut taken = vec![false; (w * h) as usize];
    let mut found = Vec::new();
    for sy in 0..h {
        for sx in 0..w {
            let i = (sy * w + sx) as usize;
            if outside[i] || taken[i] || blocks(sx, sy) {
                continue;
            }
            let mut inside = Vec::new();
            let mut stack = vec![(sx, sy)];
            taken[i] = true;
            while let Some((x, y)) = stack.pop() {
                inside.push((x, y));
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w || ny >= h {
                        continue;
                    }
                    let j = (ny * w + nx) as usize;
                    if !outside[j] && !taken[j] && !blocks(nx, ny) {
                        taken[j] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            // A roof over a fifth of the place, or over a way out, would hide what matters.
            let has_exit = inside.iter().any(|(x, y)| exits.contains(&(*x as usize, *y as usize)));
            if inside.len() as i32 * 5 > w * h || inside.len() < 2 || has_exit {
                continue;
            }
            // The roof covers the inside and the walls and doors around it.
            let mut covered: Vec<(i32, i32)> = inside.clone();
            for (x, y) in &inside {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (nx, ny) = (x + dx, y + dy);
                        if nx >= 0 && ny >= 0 && nx < w && ny < h && blocks(nx, ny) && !covered.contains(&(nx, ny)) {
                            covered.push((nx, ny));
                        }
                    }
                }
            }
            found.push(roof(&covered, inside.iter().map(|(x, y)| (*x as usize, *y as usize)).collect(), p));
        }
    }
    found
}

/// Paints a roof of shingles over some cells: two slopes meeting at a ridge along the longer
/// side, the slope facing the light paler, the eaves dark, a shadow cast below.
fn roof(covered: &[(i32, i32)], inside: Vec<(usize, usize)>, p: &Paint) -> Roof {
    let (x0, x1) = (covered.iter().map(|c| c.0).min().unwrap_or(0), covered.iter().map(|c| c.0).max().unwrap_or(0) + 1);
    let (y0, y1) = (covered.iter().map(|c| c.1).min().unwrap_or(0), covered.iter().map(|c| c.1).max().unwrap_or(0) + 1);
    let (w, h) = ((x1 - x0) * CELL, (y1 - y0) * CELL + 8);
    let body = h - 8;
    let mut picture = Pixels::new(w as u32, h as u32, CLEAR);
    let along_x = x1 - x0 >= y1 - y0;
    let tile = mix(p.wood_dark, p.accent, 0.35);
    let has = |px: i32, py: i32| px >= 0 && py >= 0 && py < body && covered.contains(&(x0 + px.div_euclid(CELL), y0 + py.div_euclid(CELL)));
    for y in 0..body {
        for x in 0..w {
            if !has(x, y) {
                continue;
            }
            // How far up the slope, from 0 at the eaves to 1 at the ridge, and which slope.
            let (along, across, size) = if along_x { (x, y, body) } else { (y, x, w) };
            let half = size as f32 / 2.0;
            let rise = 1.0 - ((across as f32 + 0.5 - half).abs() / half);
            let near_side = (across as f32) < half;
            let row = across.div_euclid(6);
            let shift = if row % 2 == 0 { 0 } else { 5 };
            let seam = (along + shift).rem_euclid(10) == 0 || across % 6 == 0;
            let mut c = if near_side { mix(tile, p.light, 0.18) } else { shade(tile, 0.78) };
            c = mix(c, if near_side { p.light } else { p.outline }, (rise * 0.15).min(0.15));
            if seam {
                c = shade(c, 0.7);
            }
            if hash(x / 2, y / 2, 401) % 37 == 0 {
                c = mix(c, p.plant_dark, 0.4);
            }
            // The ridge, and the eaves all round.
            if rise > 0.94 {
                c = shade(tile, 0.55);
            }
            if !has(x - 2, y) || !has(x + 2, y) || !has(x, y - 2) || !has(x, y + 2) {
                c = mix(c, p.outline, 0.55);
            }
            picture.set(x, y, c);
        }
    }
    // The shadow the roof throws on the ground below its southern eaves.
    for x in 0..w {
        if let Some(bottom) = (0..body).rev().find(|y| picture.get(x, *y)[3] != 0) {
            for d in 1..=8 {
                if picture.get(x, bottom + d)[3] == 0 && (x + d) % 2 == 0 {
                    picture.set(x, bottom + d, [0, 0, 0, 70]);
                }
            }
        }
    }
    Roof { picture, at: (x0 * CELL, y0 * CELL), inside }
}

/// The colour of what is left on the ground, by the master's colour or by its kind, and
/// whether it lies in a heap rather than a splash.
fn mark_look(mark: &world::Mark) -> (Rgba, bool) {
    let k = mark.kind.as_str();
    let has = |words: &[&str]| words.iter().any(|w| k.contains(w));
    let heap = has(&["dung", "poop", "feces", "shit", "manure", "говн", "дерьм", "навоз", "кал", "куч", "rubble", "щеб", "обломк", "coins", "монет", "bones", "кост", "heap", "pile"]);
    let by_kind = if has(&["blood", "кров"]) {
        [128, 18, 22, 255]
    } else if has(&["dung", "poop", "feces", "shit", "manure", "говн", "дерьм", "навоз", "кал"]) {
        [96, 62, 30, 255]
    } else if has(&["scorch", "burn", "ash", "soot", "гар", "пеп", "саж", "ожог"]) {
        [34, 28, 28, 255]
    } else if has(&["slime", "ichor", "слиз", "гно"]) {
        [96, 170, 60, 255]
    } else if has(&["vomit", "блев", "рвот"]) {
        [170, 160, 70, 255]
    } else if has(&["urine", "piss", "моч", "ссан"]) {
        [214, 196, 70, 255]
    } else if has(&["wine", "вин"]) {
        [110, 24, 60, 255]
    } else if has(&["water", "puddle", "луж", "вод"]) {
        [70, 110, 160, 255]
    } else if has(&["coins", "gold", "монет", "золот"]) {
        [222, 186, 82, 255]
    } else if has(&["bones", "кост"]) {
        [226, 220, 196, 255]
    } else if has(&["rubble", "щеб", "обломк", "stone", "камн"]) {
        [130, 128, 134, 255]
    } else {
        [60, 40, 40, 255]
    };
    let given = mark.color.trim().trim_start_matches('#');
    let color = if given.len() == 6 {
        u32::from_str_radix(given, 16).map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]).unwrap_or(by_kind)
    } else {
        by_kind
    };
    (color, heap)
}

/// Leaves a mark on the ground of its cell: a splash with drops around it, or a small heap
/// with its own light and shade.
fn leave_mark(picture: &mut Pixels, mark: &world::Mark) {
    let (color, heap) = mark_look(mark);
    let seed = (mark.x * 31 + mark.y * 17) as i32;
    let (cx, cy) = (mark.x as i32 * CELL + CELL / 2, mark.y as i32 * CELL + CELL / 2 - 6);
    if heap {
        // A heap: rounded, lit from the upper left, with a dark outline and a soft shadow.
        ground_shadow(picture, cx + 2, cy + 6, 10.0, 4.0, 0.4);
        for (dx, dy, r) in [(0, 0, 8.0f32), (-4, -3, 5.5), (4, -2, 5.0), (0, -7, 4.0)] {
            for y in -9..=9 {
                for x in -9..=9 {
                    let d = ((x as f32).powi(2) + (y as f32 * 1.3).powi(2)).sqrt();
                    if d <= r {
                        let lit = -(x as f32) * 0.6 - y as f32 * 0.8;
                        let c = if d > r - 1.0 && lit < 0.0 { shade(color, 0.55) } else if lit > r * 0.5 { mix(color, [255, 255, 255, 255], 0.25) } else if lit < -r * 0.4 { shade(color, 0.75) } else { color };
                        picture.set(cx + dx + x, cy + dy + y, c);
                    }
                }
            }
        }
    } else {
        // A splash: a ragged blot and a few drops around it, darker at the rim.
        for y in -13..=13 {
            for x in -14..=14 {
                let edge = 9.0 + cloud(cx + x + seed, cy + y, 4, 140) * 5.0;
                let d = ((x as f32).powi(2) + (y as f32 * 1.4).powi(2)).sqrt();
                let drop = hash(cx + x, cy + y, 141 + seed as u32) % 61 == 0 && d < 15.0;
                if d <= edge || drop {
                    let under = picture.get(cx + x, cy + y);
                    let c = if d > edge - 1.5 { shade(color, 0.8) } else { color };
                    picture.set(cx + x, cy + y, mix(under, c, 0.82));
                }
            }
        }
    }
}

/// Something that stands on the ground and is drawn in its own right, so that whoever walks
/// behind it is hidden by it.
pub struct Standing {
    pub picture: Pixels,
    /// The middle of its foot, in the world's own pixels.
    pub foot: (f32, f32),
}

fn blob(picture: &mut Pixels, cx: f32, cy: f32, rx: f32, ry: f32, color: impl Fn(f32, f32, f32) -> Option<Rgba>) {
    for y in (cy - ry).floor() as i32..=(cy + ry).ceil() as i32 {
        for x in (cx - rx).floor() as i32..=(cx + rx).ceil() as i32 {
            let (nx, ny) = ((x as f32 + 0.5 - cx) / rx, (y as f32 + 0.5 - cy) / ry);
            let d = nx * nx + ny * ny;
            if d <= 1.0
                && let Some(c) = color(nx, ny, d)
            {
                picture.set(x, y, c);
            }
        }
    }
}

/// Every opaque pixel that touches a clear one gets a dark edge around it.
pub fn outline(picture: &mut Pixels, color: Rgba) {
    let before = picture.clone();
    for y in 0..before.h as i32 {
        for x in 0..before.w as i32 {
            if before.get(x, y)[3] != 0 {
                continue;
            }
            let touches = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| before.get(x + dx, y + dy)[3] != 0);
            if touches {
                picture.set(x, y, color);
            }
        }
    }
}

fn tree(p: &Paint, seed: (i32, i32)) -> Pixels {
    const W: i32 = 40;
    const H: i32 = 60;
    let mut t = Pixels::new(W as u32, H as u32, CLEAR);
    let lean = (hash(seed.0, seed.1, 201) % 5) as i32 - 2;
    // The trunk, widening into roots.
    for y in 34..H - 1 {
        let half = if y > H - 6 { 5 } else { 3 };
        for x in W / 2 - half..W / 2 + half {
            let lit = x < W / 2 - 1;
            let bark = hash(x, y / 3, 202 + seed.0 as u32) % 4 == 0;
            t.set(x, y, if bark { mix(p.wood_dark, p.outline, 0.3) } else if lit { p.wood } else { p.wood_dark });
        }
    }
    // The crown: a few clumps of leaves, lit from the upper left.
    let clumps = [(0.0, 22.0, 17.0, 15.0), (-8.0, 27.0, 11.0, 10.0), (8.0, 28.0, 11.0, 10.0), (lean as f32, 12.0, 12.0, 11.0)];
    for (n, (dx, cy, rx, ry)) in clumps.into_iter().enumerate() {
        let cx = W as f32 / 2.0 + dx + if n == 0 { lean as f32 * 0.5 } else { 0.0 };
        blob(&mut t, cx, cy, rx, ry, |nx, ny, d| {
            let lit = -nx * 0.6 - ny * 0.8;
            let (px, py) = ((cx + nx * rx) as i32, (cy + ny * ry) as i32);
            let leaf = unit(px / 2, py / 2, 203 + seed.1 as u32);
            Some(if lit + leaf * 0.5 > 0.62 {
                mix(p.plant, p.light, 0.35)
            } else if lit + leaf * 0.45 > 0.12 {
                p.plant
            } else if d > 0.8 && lit < -0.3 {
                mix(p.plant_dark, p.outline, 0.45)
            } else {
                p.plant_dark
            })
        });
    }
    outline(&mut t, p.outline);
    t
}

fn bush(p: &Paint, seed: (i32, i32)) -> Pixels {
    let mut b = Pixels::new(32, 26, CLEAR);
    for (n, (cx, cy, rx, ry)) in [(16.0, 15.0, 13.0, 9.0), (9.0, 16.0, 7.0, 7.0), (23.0, 17.0, 7.0, 6.0), (15.0, 9.0, 8.0, 6.0)].into_iter().enumerate() {
        blob(&mut b, cx, cy, rx, ry, |nx, ny, _| {
            let lit = -nx * 0.6 - ny * 0.8;
            let (px, py) = ((cx + nx * rx) as i32, (cy + ny * ry) as i32);
            let leaf = unit(px / 2, py / 2, 211 + n as u32 + seed.0 as u32);
            // Berries in the world's own colour.
            if hash(px / 2, py / 2, 212 + seed.1 as u32) % 23 == 0 {
                return Some(p.accent);
            }
            Some(if lit + leaf * 0.5 > 0.6 { mix(p.plant, p.light, 0.3) } else if lit + leaf * 0.4 > 0.1 { p.plant } else { p.plant_dark })
        });
    }
    outline(&mut b, p.outline);
    b
}

fn rock(p: &Paint, seed: (i32, i32)) -> Pixels {
    let mut r = Pixels::new(32, 26, CLEAR);
    let squat = 1.0 + (hash(seed.0, seed.1, 221) % 3) as f32 * 0.08;
    blob(&mut r, 16.0, 15.0, 13.0, 9.5 / squat, |nx, ny, _| {
        // Three flat planes: a lit top, a mid face, a dark foot.
        let plane = ny * 1.3 + nx * 0.5;
        let (px, py) = ((16.0 + nx * 13.0) as i32, (15.0 + ny * 9.5) as i32);
        let mut c = if plane < -0.45 { mix(p.stone, p.light, 0.3) } else if plane < 0.35 { p.stone } else { p.stone_dark };
        if hash(px, py, 222 + seed.0 as u32) % 19 == 0 {
            c = mix(c, p.stone_dark, 0.5);
        }
        // Moss in the lee.
        if plane > 0.2 && hash(px / 2, py / 2, 223 + seed.1 as u32) % 6 == 0 {
            c = mix(p.plant_dark, c, 0.35);
        }
        Some(c)
    });
    outline(&mut r, p.outline);
    r
}

/// The hour of a place: how dark it is, and the colour of the dark.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Hour {
    /// How much of the light is gone, from none to nearly all.
    pub dark: f32,
    pub tint: [u8; 3],
}

impl Hour {
    pub fn of(location: &Location) -> Self {
        match location.light.trim().to_lowercase().as_str() {
            "dusk" | "dawn" | "dim" => Self { dark: 0.48, tint: [52, 22, 70] },
            "night" => Self { dark: 0.7, tint: [10, 16, 52] },
            "dark" | "cave" | "underground" => Self { dark: 0.84, tint: [8, 6, 16] },
            _ => Self { dark: 0.0, tint: [0, 0, 0] },
        }
    }
}

/// Something that gives light: where it is, in the world's own pixels, and how far it reaches,
/// in cells.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Lamp {
    pub x: f32,
    pub y: f32,
    pub reach: f32,
}

const LIGHT_WORDS: [&str; 22] =
    ["fire", "torch", "lamp", "lantern", "candle", "brazier", "hearth", "glow", "crystal", "light", "flame", "forge", "костёр", "костер", "факел", "ламп", "фонар", "свеч", "огон", "очаг", "светил", "пламя"];

/// Whether a thing sounds like it burns or glows.
pub fn is_lamp(prop: &world::Prop) -> bool {
    let named = format!("{} {} {}", prop.id, prop.sprite, prop.name).to_lowercase();
    LIGHT_WORDS.iter().any(|word| named.contains(word))
}

/// The lamps of a place: whatever among its things sounds like it burns or glows.
pub fn lamps(location: &Location) -> Vec<Lamp> {
    let tile = world::TILE as f32;
    location
        .props
        .iter()
        .filter(|prop| is_lamp(prop))
        .map(|prop| Lamp { x: (prop.x as f32 + 0.5) * tile, y: (prop.y as f32 + 0.5) * tile, reach: 4.2 })
        .collect()
}

/// The roof over a building: drawn over everything inside it, and seen through when the
/// hero comes close.
pub struct Roof {
    pub picture: Pixels,
    /// Its top left corner, in canvas pixels.
    pub at: (i32, i32),
    /// The cells under it that one can stand in.
    pub inside: Vec<(usize, usize)>,
}

impl Roof {
    /// Whether a hero standing in `cell` is close enough to see under the roof: inside, or
    /// within two cells of the building.
    pub fn near(inside: &[(usize, usize)], cell: (usize, usize)) -> bool {
        inside.iter().any(|(x, y)| x.abs_diff(cell.0) <= 3 && y.abs_diff(cell.1) <= 3)
    }
}

pub struct Scene {
    /// The ground, once for each beat of the water. A place without water has one.
    pub ground: Vec<Pixels>,
    pub roofs: Vec<Roof>,
    /// The cells of water, for rain to ring.
    pub water: Vec<(usize, usize)>,
    pub standing: Vec<Standing>,
    pub hour: Hour,
    pub lamps: Vec<Lamp>,
}

/// A sprite the master drew, enlarged and graded like the rest of the picture.
pub fn sprite(sprite: &world::Sprite, palette: &world::Palette) -> Pixels {
    let mut small = Pixels::new(sprite.width(), sprite.height(), CLEAR);
    for y in 0..sprite.height() {
        for x in 0..sprite.width() {
            small.set(x as i32, y as i32, grade(palette.rgba(sprite.index_at(x, y))));
        }
    }
    enlarge(&small)
}

/// Paints a place.
pub fn paint(location: &Location, world: &World) -> Scene {
    let cells = Cells::read(location, world);
    let mut drawn = std::collections::HashMap::new();
    for id in cells.drawn.iter().flatten() {
        if !drawn.contains_key(id)
            && let Some(tile) = world.tiles.get(id)
        {
            let mut small = Pixels::new(tile.sprite.width(), tile.sprite.height(), CLEAR);
            for y in 0..tile.sprite.height() {
                for x in 0..tile.sprite.width() {
                    small.set(x as i32, y as i32, world.palette.rgba(tile.sprite.index_at(x, y)));
                }
            }
            drawn.insert(id.clone(), enlarge(&small));
        }
    }
    let painter = Painter { paint: Paint::of(world), cells, world, drawn };
    let _ = painter.world;
    let (w, h) = (MAP_W as i32 * CELL, MAP_H as i32 * CELL);
    let has_water = painter.cells.ground.contains(&Ground::Water);
    let mut first = Pixels::new(w as u32, h as u32, CLEAR);
    for y in 0..h {
        for x in 0..w {
            first.set(x, y, grade(painter.pixel(x, y, 0)));
        }
    }
    // What stands on the ground, and the shadow each thing throws.
    let tile = world::TILE as f32;
    let mut standing = Vec::new();
    let mut shadows: Vec<(i32, i32, f32, f32, f32)> = Vec::new();
    for cy in 0..MAP_H as i32 {
        for cx in 0..MAP_W as i32 {
            let Some(growth) = painter.cells.growth[cy as usize * MAP_W + cx as usize] else { continue };
            let (mut picture, rx, ry) = match growth {
                Growth::Tree => (tree(&painter.paint, (cx, cy)), 13.0, 6.0),
                Growth::Bush => (bush(&painter.paint, (cx, cy)), 13.0, 5.0),
                Growth::Rock => (rock(&painter.paint, (cx, cy)), 13.0, 5.0),
            };
            for y in 0..picture.h as i32 {
                for x in 0..picture.w as i32 {
                    let c = picture.get(x, y);
                    picture.set(x, y, grade(c));
                }
            }
            // A little off the grid, so a wood is not a row of posts.
            let (jx, jy) = ((hash(cx, cy, 231) % 5) as f32 - 2.0, (hash(cx, cy, 232) % 4) as f32 - 3.0);
            let foot = ((cx as f32 + 0.5) * tile + jx * 0.5, (cy as f32 + 1.0) * tile + jy * 0.5);
            shadows.push(((foot.0 * PX as f32) as i32 + 3, (foot.1 * PX as f32) as i32 - 3, rx, ry, 0.4));
            standing.push(Standing { picture, foot });
        }
    }
    for prop in &location.props {
        shadows.push((prop.x as i32 * CELL + CELL / 2 + 2, prop.y as i32 * CELL + CELL - 3, 11.0, 4.0, 0.35));
    }
    let mut ground = vec![first];
    if has_water {
        // Only the water moves: the rest of the picture is copied, the water painted again.
        for frame in 1..WATER_FRAMES {
            let mut next = ground[0].clone();
            for cy in 0..MAP_H as i32 {
                for cx in 0..MAP_W as i32 {
                    if painter.cells.at(cx, cy) != Ground::Water {
                        continue;
                    }
                    for y in cy * CELL..(cy + 1) * CELL {
                        for x in cx * CELL..(cx + 1) * CELL {
                            next.set(x, y, grade(painter.pixel(x, y, frame)));
                        }
                    }
                }
            }
            ground.push(next);
        }
    }
    for picture in &mut ground {
        for (x, y, rx, ry, strength) in &shadows {
            ground_shadow(picture, *x, *y, *rx, *ry, *strength);
        }
        for mark in &location.marks {
            leave_mark(picture, mark);
        }
    }
    // What stands by the water is seen again in it, upside down and a little blue.
    let mut seen: Vec<(Pixels, (i32, i32))> = standing.iter().map(|s| (s.picture.clone(), ((s.foot.0 * PX as f32) as i32, (s.foot.1 * PX as f32) as i32))).collect();
    for prop in &location.props {
        if let Some(picture) = world.sprites.get(&prop.sprite) {
            seen.push((sprite(picture, &world.palette), ((prop.x as i32 * CELL) + CELL / 2, (prop.y as i32 + 1) * CELL)));
        }
    }
    for (n, picture) in ground.iter_mut().enumerate() {
        for (thing, foot) in &seen {
            reflect(picture, &painter.cells, thing, *foot, n);
        }
    }
    let water = (0..MAP_H).flat_map(|y| (0..MAP_W).map(move |x| (x, y))).filter(|(x, y)| painter.cells.at(*x as i32, *y as i32) == Ground::Water).collect();
    let exits: Vec<(usize, usize)> = location.exits.iter().map(|e| (e.x, e.y)).collect();
    let roofs = roofs(&painter.cells, &painter.paint, &exits);
    Scene { ground, roofs, water, standing, hour: Hour::of(location), lamps: lamps(location) }
}

/// One cell of the dark is this many picture pixels a side: the dark falls in small squares.
pub const DARK_BLOCK: i32 = 4;

/// The dark over a place, and the holes the lamps make in it. `lights` are in the world's
/// own pixels with a reach in cells; `beat` makes flames waver.
pub fn darkness(hour: Hour, lights: &[Lamp], beat: u32) -> Pixels {
    darkness_seen(hour, lights, beat, None, None)
}

/// The dark, with what the party never saw of the place left wholly black; and fog, when
/// `fog` gives the seconds it has drifted: thick in the distance, thin where there is light.
pub fn darkness_seen(hour: Hour, lights: &[Lamp], beat: u32, seen: Option<&[u32]>, fog: Option<f32>) -> Pixels {
    let (w, h) = (MAP_W as i32 * CELL / DARK_BLOCK, MAP_H as i32 * CELL / DARK_BLOCK);
    let mut dark = Pixels::new(w as u32, h as u32, CLEAR);
    let per_block = DARK_BLOCK as f32 / PX as f32;
    for y in 0..h {
        for x in 0..w {
            let (wx, wy) = ((x as f32 + 0.5) * per_block, (y as f32 + 0.5) * per_block);
            let mut lit: f32 = 0.0;
            for (n, lamp) in lights.iter().enumerate() {
                let waver = 1.0 + ((hash(n as i32, beat as i32, 301) % 100) as f32 / 100.0 - 0.5) * 0.08;
                let reach = lamp.reach * world::TILE as f32 * waver;
                let d = ((wx - lamp.x).powi(2) + (wy - lamp.y).powi(2)).sqrt() / reach;
                lit = lit.max((1.0 - d).clamp(0.0, 1.0).powf(0.8));
            }
            let cell = ((wx / world::TILE as f32) as usize, (wy / world::TILE as f32) as usize);
            if seen.is_some_and(|rows| rows.get(cell.1).is_none_or(|row| row & (1 << cell.0) == 0)) {
                // Black in the dark; a thick grey where the hour is light and fog hides it.
                let unseen = if hour.dark >= 0.5 { [hour.tint[0] / 3, hour.tint[1] / 3, hour.tint[2] / 3, 255] } else { [hour.tint[0], hour.tint[1], hour.tint[2], 235] };
                dark.set(x, y, unseen);
                continue;
            }
            if let Some(t) = fog {
                // Drifting thicker and thinner, cleared around every light, its grey darkened
                // by the hour; where the night is darker than the fog, the night shows.
                let drift = cloud_noise(x + (t * 2.0) as i32, y, w, 14) * 0.6 + cloud_noise(x - (t * 1.3) as i32, y + 7, w, 6) * 0.4;
                let thick = ((0.42 + drift * 0.45) * (1.0 - lit * 1.2)).clamp(0.0, 0.85);
                let night = hour.dark * (1.0 - lit);
                let shade = 1.0 - hour.dark * 0.8;
                let colour = if night > thick { hour.tint } else { [(176.0 * shade) as u8, (178.0 * shade) as u8, (188.0 * shade) as u8] };
                let steps = ((thick.max(night) * 12.0) + dither(x, y) - 0.5).round().clamp(0.0, 12.0) / 12.0;
                if steps > 0.0 {
                    dark.set(x, y, [colour[0], colour[1], colour[2], (steps * 255.0) as u8]);
                }
                continue;
            }
            // Flat steps of dark, broken up into a pattern rather than banded.
            let amount = hour.dark * (1.0 - lit);
            let steps = ((amount * 12.0) + dither(x, y) - 0.5).round().clamp(0.0, 12.0) / 12.0;
            if steps > 0.0 {
                dark.set(x, y, [hour.tint[0], hour.tint[1], hour.tint[2], (steps * 255.0) as u8]);
            }
        }
    }
    dark
}

/// The warm pool around a lamp: rings of thinning flat pixels.
pub fn warm_pool(size: i32) -> Pixels {
    let mut pool = Pixels::new(size as u32, size as u32, CLEAR);
    let r = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 + 0.5 - r).powi(2) + (y as f32 + 0.5 - r).powi(2)).sqrt() / r;
            let strength = (1.0 - d).max(0.0).powf(1.4);
            if strength > dither(x / 2, y / 2) {
                pool.set(x, y, [255, 196, 96, 34]);
            }
        }
    }
    pool
}

/// The corners of the picture fall into shadow.
pub fn vignette(w: i32, h: i32) -> Pixels {
    let mut edge = Pixels::new(w as u32, h as u32, CLEAR);
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = ((x as f32 / w as f32 - 0.5) * 2.0, (y as f32 / h as f32 - 0.5) * 2.0);
            let far = ((dx * dx + dy * dy).sqrt() - 0.72).max(0.0) * 2.4;
            let steps = (far * 4.0 + dither(x / 2, y / 2) - 0.5).round().clamp(0.0, 4.0) / 4.0;
            if steps > 0.0 {
                edge.set(x, y, [10, 6, 14, (steps * 150.0) as u8]);
            }
        }
    }
    edge
}

/// Where one may walk, drawn over the place on demand: open cells outlined, cells that block
/// the way hatched, ways out marked.
pub fn walk_grid(location: &Location, world: &World) -> Pixels {
    let (w, h) = (MAP_W as i32 * CELL, MAP_H as i32 * CELL);
    let mut grid = Pixels::new(w as u32, h as u32, CLEAR);
    for cy in 0..MAP_H {
        for cx in 0..MAP_W {
            let solid = location.tile_id(cx, cy).and_then(|id| world.tiles.get(id)).is_none_or(|t| !t.walkable) || location.props.iter().any(|p| p.blocking && p.x == cx && p.y == cy);
            let exit = location.exit_at(cx, cy).is_some();
            let (x0, y0) = (cx as i32 * CELL, cy as i32 * CELL);
            for iy in 0..CELL {
                for ix in 0..CELL {
                    let edge = ix == 0 || iy == 0 || ix == CELL - 1 || iy == CELL - 1;
                    let c = if exit {
                        if edge || ix == 1 || iy == 1 || ix == CELL - 2 || iy == CELL - 2 { [120, 255, 140, 220] } else { [120, 255, 140, 60] }
                    } else if solid {
                        // Red stripes across a cell nobody can enter.
                        if (ix + iy).rem_euclid(8) < 2 { [255, 70, 60, 120] } else if edge { [255, 70, 60, 90] } else { CLEAR }
                    } else if edge && (ix + iy) % 2 == 0 {
                        [255, 255, 255, 70]
                    } else {
                        CLEAR
                    };
                    if c[3] != 0 {
                        grid.set(x0 + ix, y0 + iy, c);
                    }
                }
            }
        }
    }
    grid
}

/// The arrow over a way out, pointing off the map the way it leads: a pale chevron with a
/// dark outline, 24 canvas pixels a side. Down for a way out in the middle of a place.
pub fn exit_arrow(dx: i32, dy: i32) -> Pixels {
    const S: i32 = 24;
    let mut a = Pixels::new(S as u32, S as u32, CLEAR);
    for y in 0..S {
        for x in 0..S {
            // Draw a chevron pointing down, then turn it.
            // The chevron as drawn points up; it is turned to point where the way leads.
            let (u, v) = match (dx, dy) {
                (1, _) => (y, S - 1 - x),
                (-1, _) => (S - 1 - y, x),
                (_, -1) => (x, y),
                _ => (x, S - 1 - y),
            };
            let centre = (u as f32 + 0.5 - S as f32 / 2.0).abs();
            let band = v as f32 - 4.0 - centre;
            if (0.0..7.0).contains(&band) && centre < 10.0 {
                let lit = band < 2.5;
                a.set(x, y, if lit { [255, 246, 200, 255] } else { [240, 196, 90, 255] });
            }
        }
    }
    outline(&mut a, [40, 24, 12, 255]);
    a
}

/// A soft shadow to lay under anything that walks.
pub fn foot_shadow() -> Pixels {
    let mut shadow = Pixels::new(24, 10, CLEAR);
    for y in 0..10 {
        for x in 0..24 {
            let d = ((x as f32 + 0.5 - 12.0) / 12.0).powi(2) + ((y as f32 + 0.5 - 5.0) / 5.0).powi(2);
            if d <= 1.0 && (1.0 - d) * 1.5 + 0.2 > dither(x, y) {
                shadow.set(x, y, [0, 0, 0, 90]);
            }
        }
    }
    shadow
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_world() -> World {
        let draft: world::WorldDraft = serde_json::from_str(&world::fake::world("scene")).unwrap();
        world::build_world(&draft, "scene", "English").unwrap().0
    }

    #[test]
    fn the_ground_covers_the_place_at_twice_the_masters_measure_with_no_holes() {
        let w = test_world();
        let scene = paint(w.here(), &w);
        let ground = &scene.ground[0];
        assert_eq!((ground.w, ground.h), (960, 512));
        assert!((0..512).all(|y| (0..960).all(|x| ground.get(x, y)[3] == 255)));
    }

    #[test]
    fn a_place_with_water_has_a_picture_for_each_beat_and_only_the_water_differs() {
        let w = test_world();
        let scene = paint(w.here(), &w);
        assert_eq!(scene.ground.len(), WATER_FRAMES, "the camp has a pond");
        let cells = Cells::read(w.here(), &w);
        let mut moved = false;
        for y in (0..512).step_by(3) {
            for x in (0..960).step_by(3) {
                let differs = scene.ground[0].get(x, y) != scene.ground[1].get(x, y);
                moved |= differs;
                if differs {
                    assert_eq!(cells.at(x / CELL, y / CELL), Ground::Water, "only water moves");
                }
            }
        }
        assert!(moved);
    }

    #[test]
    fn trees_bushes_and_rocks_stand_in_their_own_right() {
        let w = test_world();
        let scene = paint(w.here(), &w);
        let cells = Cells::read(w.here(), &w);
        assert_eq!(scene.standing.len(), cells.growth.iter().flatten().count());
        assert!(scene.standing.len() > 50, "the camp is ringed with trees");
        assert!(scene.standing.iter().all(|s| s.picture.get(0, 0)[3] == 0), "each has a clear surround");
    }

    /// The test world with one cell of its camp changed.
    fn with_cell(w: &mut World, x: usize, y: usize, tile: &str) {
        let current = w.current.clone();
        let place = w.locations.get_mut(&current).unwrap();
        let c = *place.legend.iter().find(|(_, id)| id.as_str() == tile).expect("the tile is in the legend").0;
        let mut row: Vec<char> = place.map[y].chars().collect();
        row[x] = c;
        place.map[y] = row.into_iter().collect();
    }

    #[test]
    fn a_tree_by_the_water_is_seen_in_it() {
        let mut w = test_world();
        with_cell(&mut w, 5, 10, "water");
        with_cell(&mut w, 5, 11, "water");
        with_cell(&mut w, 5, 9, "grass");
        let bare = paint(w.here(), &w);
        with_cell(&mut w, 5, 9, "tree");
        let treed = paint(w.here(), &w);
        let differs = |cx: i32, cy: i32| (0..CELL).flat_map(|y| (0..CELL).map(move |x| (x, y))).filter(|(x, y)| bare.ground[0].get(cx * CELL + x, cy * CELL + y) != treed.ground[0].get(cx * CELL + x, cy * CELL + y)).count();
        assert!(differs(5, 10) > 40, "the water below the tree shows it");
        assert!(differs(5, 13) == 0, "far below, nothing");
    }

    #[test]
    fn a_walled_hut_with_a_door_gets_a_roof_and_the_open_camp_does_not() {
        let w = test_world();
        let scene = paint(w.here(), &w);
        assert_eq!(scene.roofs.len(), 1);
        let roof = &scene.roofs[0];
        assert_eq!(roof.inside.len(), 4 * 3);
        assert!(roof.inside.contains(&(24, 3)));
        assert_eq!(roof.at, (22 * CELL, CELL));
    }

    #[test]
    fn the_same_place_is_painted_the_same_every_time() {
        let w = test_world();
        let (a, b) = (paint(w.here(), &w), paint(w.here(), &w));
        assert!((0..512).step_by(7).all(|y| (0..960).step_by(5).all(|x| a.ground[0].get(x, y) == b.ground[0].get(x, y))));
    }

    #[test]
    fn enlarging_keeps_flat_areas_flat_and_smooths_a_diagonal() {
        let small = Pixels::from_rows(&["ab", "ba"], &[('a', [255, 0, 0, 255]), ('b', [0, 0, 255, 255])]);
        let big = enlarge(&small);
        assert_eq!((big.w, big.h), (4, 4));
        let flat = Pixels::new(3, 3, [9, 9, 9, 255]);
        let grown = enlarge(&flat);
        assert!((0..6).all(|y| (0..6).all(|x| grown.get(x, y) == [9, 9, 9, 255])));
    }

    #[test]
    fn a_fire_is_a_lamp_and_a_cat_is_not() {
        let w = test_world();
        let found = lamps(w.here());
        assert_eq!(found.len(), 1, "the campfire");
        let dark = darkness(Hour { dark: 0.6, tint: [0, 0, 60] }, &found, 0);
        let at = |wx: f32, wy: f32| dark.get((wx * PX as f32) as i32 / DARK_BLOCK, (wy * PX as f32) as i32 / DARK_BLOCK)[3];
        assert!(at(found[0].x, found[0].y) < at(8.0, 8.0), "it is lighter by the fire than in the far corner");
        assert!(darkness(Hour { dark: 0.0, tint: [0, 0, 0] }, &found, 0).get(3, 3)[3] == 0, "by day there is no dark");
    }

    #[test]
    fn tiles_the_master_made_are_painted_as_what_their_names_say_and_never_against_the_rules() {
        let tile = |name: &str, walkable: bool| world::TileDef { name: name.into(), walkable, sprite: world::Sprite { rows: vec!["1".repeat(16); 16] } };
        assert_eq!(classify("wet_cobble", &tile("Мокрая брусчатка", true)), Some((Ground::StoneFloor, None)));
        assert_eq!(classify("pier_wood", &tile("Причал", true)), Some((Ground::WoodFloor, None)));
        assert_eq!(classify("foggy_water", &tile("Туманная вода", false)), Some((Ground::Water, None)));
        assert_eq!(classify("nerve_tree", &tile("Нервное древо", false)), None);
        assert_eq!(classify("log_wall", &tile("бревенчатая стена", false)), Some((Ground::WoodWall, None)));
        assert_eq!(classify("idol", &tile("a strange idol", false)), None);
        // Shallow water one can wade through is not painted as the deep water that stops one.
        assert_eq!(classify("shallows", &tile("shallow water", true)), None);
    }

    #[test]
    fn the_hour_is_read_from_what_the_master_wrote() {
        let mut w = test_world();
        w.here_mut().light = "Night".into();
        assert!(Hour::of(w.here()).dark > 0.5);
        w.here_mut().light = String::new();
        assert_eq!(Hour::of(w.here()).dark, 0.0);
    }
}
