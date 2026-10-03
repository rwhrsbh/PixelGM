//! Pictures of the things in the bag, drawn in code at the interface's measure: sixteen
//! pixels to a cell of the bag. Gear is drawn from what the rules say it is; anything else
//! from the words of its name — a potion, a coin, a key, a scroll — or as a pouch.

use crate::art::Pixels;
use crate::scene::{mix, outline, shade};
use crate::theme::*;
use world::Item;
use world::bag::{Gear, gear_kind};

pub const CELL: i32 = 16;

const STEEL: Rgba = [176, 184, 196, 255];
const STEEL_DARK: Rgba = [104, 112, 128, 255];
const SHINE: Rgba = [236, 240, 248, 255];
const GOLD: Rgba = [222, 186, 82, 255];
const GOLD_DARK: Rgba = [150, 108, 40, 255];
const TIMBER: Rgba = [126, 82, 46, 255];
const LEATHER: Rgba = [140, 90, 52, 255];
const LEATHER_DARK: Rgba = [92, 56, 34, 255];
const CLOTH: Rgba = [214, 200, 160, 255];

fn hash(text: &str) -> u32 {
    text.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619))
}

/// A colour of the thing's own, the same every time for the same name.
fn tint(text: &str) -> Rgba {
    const TINTS: [Rgba; 8] = [[200, 50, 50, 255], [60, 120, 210, 255], [70, 170, 80, 255], [170, 80, 200, 255], [230, 150, 40, 255], [60, 180, 180, 255], [220, 220, 230, 255], [120, 60, 40, 255]];
    TINTS[hash(text) as usize % TINTS.len()]
}

fn fill(p: &mut Pixels, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgba) {
    for y in y0..y1 {
        for x in x0..x1 {
            p.set(x, y, c);
        }
    }
}

fn line(p: &mut Pixels, (x0, y0): (i32, i32), (x1, y1): (i32, i32), width: i32, c: Rgba) {
    let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
    for i in 0..=steps {
        let x = x0 + (x1 - x0) * i / steps;
        let y = y0 + (y1 - y0) * i / steps;
        fill(p, x, y, x + width, y + 1, c);
    }
}

fn disc(p: &mut Pixels, cx: f32, cy: f32, r: f32, c: impl Fn(f32, f32) -> Rgba) {
    for y in (cy - r).floor() as i32..=(cy + r).ceil() as i32 {
        for x in (cx - r).floor() as i32..=(cx + r).ceil() as i32 {
            let (dx, dy) = ((x as f32 + 0.5 - cx) / r, (y as f32 + 0.5 - cy) / r);
            if dx * dx + dy * dy <= 1.0 {
                p.set(x, y, c(dx, dy));
            }
        }
    }
}

/// The picture of a thing, as large as the cells it takes.
pub fn icon(item: &Item) -> Pixels {
    let (w, h) = item.size();
    let mut p = Pixels::new(w as u32 * CELL as u32, h as u32 * CELL as u32, CLEAR);
    let (pw, ph) = (p.w as i32, p.h as i32);
    match item.gear.as_deref() {
        Some(index) => match gear_kind(index) {
            Some(Gear::Shield) => shield(&mut p, pw, ph),
            Some(Gear::Armor) => armor(&mut p, index, pw, ph),
            _ => weapon(&mut p, index, pw, ph),
        },
        None => thing(&mut p, &item.name, pw, ph),
    }
    outline(&mut p, INK);
    p
}

fn blade(p: &mut Pixels, cx: i32, top: i32, guard: i32, bottom: i32) {
    // The blade, lit on its left edge, to a point.
    fill(p, cx - 2, top + 2, cx + 2, guard, STEEL);
    fill(p, cx - 2, top + 2, cx - 1, guard, SHINE);
    fill(p, cx + 1, top + 2, cx + 2, guard, STEEL_DARK);
    fill(p, cx - 1, top, cx + 1, top + 2, STEEL);
    fill(p, cx, guard - 6, cx + 1, guard, STEEL_DARK);
    // The guard, the grip, the pommel.
    fill(p, cx - 6, guard, cx + 6, guard + 2, GOLD);
    fill(p, cx - 6, guard + 1, cx + 6, guard + 2, GOLD_DARK);
    fill(p, cx - 1, guard + 2, cx + 1, bottom - 2, LEATHER_DARK);
    for y in (guard + 3..bottom - 2).step_by(2) {
        fill(p, cx - 1, y, cx + 1, y + 1, LEATHER);
    }
    fill(p, cx - 2, bottom - 2, cx + 2, bottom, GOLD);
}

fn weapon(p: &mut Pixels, index: &str, w: i32, h: i32) {
    let cx = w / 2;
    let i = index;
    if i.contains("crossbow") {
        fill(p, cx - 1, 4, cx + 2, h - 3, TIMBER);
        fill(p, cx - 1, 4, cx, h - 3, mix(TIMBER, SHINE, 0.2));
        line(p, (3, 12), (cx, 8), 2, STEEL);
        line(p, (cx, 8), (w - 4, 12), 2, STEEL);
        line(p, (4, 13), (w - 5, 13), 1, CLOTH);
    } else if i.contains("bow") {
        for y in 3..h - 3 {
            let t = (y - 3) as f32 / (h - 7) as f32 - 0.5;
            let x = 5 + ((1.0 - 4.0 * t * t) * (w as f32 - 14.0)) as i32;
            fill(p, x, y, x + 3, y + 1, TIMBER);
            p.set(x, y, mix(TIMBER, SHINE, 0.3));
        }
        line(p, (6, 3), (6, h - 4), 1, CLOTH);
        fill(p, w / 2 + 2, h / 2 - 3, w / 2 + 6, h / 2 + 3, LEATHER_DARK);
    } else if i.contains("spear") || i.contains("pike") || i.contains("lance") || i == "trident" || i == "javelin" {
        fill(p, cx - 1, 12, cx + 1, h - 2, TIMBER);
        fill(p, cx - 1, 12, cx, h - 2, mix(TIMBER, SHINE, 0.25));
        for y in 2..12 {
            let half = ((y - 2) / 3).min(3);
            fill(p, cx - half, y, cx + half + 1, y + 1, if y < 5 { SHINE } else { STEEL });
        }
        fill(p, cx - 3, 12, cx + 3, 14, STEEL_DARK);
    } else if i.contains("staff") {
        fill(p, cx - 1, 6, cx + 2, h - 2, TIMBER);
        fill(p, cx - 1, 6, cx, h - 2, mix(TIMBER, SHINE, 0.25));
        disc(p, cx as f32 + 0.5, 6.0, 4.0, |dx, dy| if dx + dy < -0.6 { SHINE } else { [90, 170, 230, 255] });
    } else if i.contains("axe") {
        fill(p, cx, 3, cx + 2, h - 2, TIMBER);
        fill(p, cx, 3, cx + 1, h - 2, mix(TIMBER, SHINE, 0.25));
        // A crescent of steel to the left of the haft.
        disc(p, cx as f32 - 2.0, 11.0, 9.0, |dx, dy| if dx > 0.25 { CLEAR } else if dx < -0.75 || dx + dy < -0.9 { SHINE } else { STEEL });
        for y in 0..p.h as i32 {
            for x in cx..cx + 2 {
                if p.get(x, y) == CLEAR && y > 2 && y < h - 2 {
                    p.set(x, y, TIMBER);
                }
            }
        }
        if i.contains("great") || i.contains("battle") {
            disc(p, cx as f32 + 4.0, 11.0, 6.0, |dx, _| if dx < -0.2 { CLEAR } else { STEEL_DARK });
        }
    } else if i.contains("mace") || i.contains("hammer") || i == "maul" || i == "club" || i == "morningstar" || i == "flail" || i.contains("pick") {
        let head = if i == "club" { TIMBER } else { STEEL };
        fill(p, cx - 1, 10, cx + 1, h - 2, TIMBER);
        if i.contains("hammer") || i == "maul" {
            fill(p, cx - 6, 3, cx + 6, 10, head);
            fill(p, cx - 6, 3, cx + 6, 4, SHINE);
        } else {
            disc(p, cx as f32, 7.0, 5.0, |dx, dy| if dx + dy < -0.7 { SHINE } else if dx + dy > 0.8 { STEEL_DARK } else { head });
            if i == "morningstar" {
                for (x, y) in [(cx, 1), (cx - 6, 7), (cx + 5, 7)] {
                    p.set(x, y, STEEL_DARK);
                }
            }
        }
    } else if i == "dagger" || i == "sickle" || i == "dart" {
        blade(p, cx, 3, h - 11, h - 2);
    } else if i == "sling" || i == "net" || i == "whip" {
        line(p, (3, 3), (w - 4, h - 6), 1, LEATHER);
        disc(p, w as f32 - 6.0, h as f32 - 6.0, 3.5, |_, _| LEATHER_DARK);
    } else {
        // A sword, long or short by its cells.
        blade(p, cx, 2, h - 12, h - 1);
        if i.contains("great") || i.contains("two") {
            fill(p, cx - 3, 6, cx + 3, h - 12, STEEL);
            fill(p, cx - 3, 6, cx - 2, h - 12, SHINE);
        }
    }
}

fn armor(p: &mut Pixels, index: &str, w: i32, h: i32) {
    let (body, dark) = if index.contains("leather") || index.contains("hide") || index.contains("padded") {
        (LEATHER, LEATHER_DARK)
    } else {
        (STEEL, STEEL_DARK)
    };
    // A coat with sleeves, laid flat.
    let (l, r) = (6, w - 6);
    fill(p, l, 6, r, h - 4, body);
    fill(p, l - 4, 8, l, 26, body);
    fill(p, r, 8, r + 4, 26, body);
    fill(p, l - 4, 24, l, 26, dark);
    fill(p, r, 24, r + 4, 26, dark);
    fill(p, w / 2 - 4, 4, w / 2 + 4, 9, CLEAR);
    fill(p, w / 2 - 4, 9, w / 2 + 4, 10, dark);
    fill(p, r - 3, 10, r, h - 4, shade(body, 0.85));
    fill(p, l, 10, l + 2, h - 4, shade(body, 1.1));
    if index.contains("mail") || index.contains("scale") || index.contains("ring") {
        for y in 11..h - 5 {
            for x in l + 1..r - 1 {
                if (x + y) % 2 == 0 {
                    p.set(x, y, dark);
                }
            }
        }
    } else if index.contains("plate") || index.contains("splint") || index.contains("breast") {
        fill(p, w / 2 - 1, 10, w / 2 + 1, h - 6, SHINE);
        for y in [18, 26, 34] {
            fill(p, l, y, r, y + 1, dark);
        }
    } else {
        for y in (14..h - 6).step_by(6) {
            fill(p, l + 2, y, r - 2, y + 1, dark);
        }
    }
    fill(p, l, h - 9, r, h - 7, LEATHER_DARK);
    fill(p, w / 2 - 2, h - 9, w / 2 + 2, h - 7, GOLD);
}

fn shield(p: &mut Pixels, w: i32, h: i32) {
    for y in 2..h - 2 {
        let narrow = if y > h / 2 { (y - h / 2) * (w / 2 - 3) / (h / 2 - 2) } else { 0 };
        for x in 3 + narrow..w - 3 - narrow {
            let rim = x <= 4 + narrow || x >= w - 5 - narrow || y <= 3;
            p.set(x, y, if rim { STEEL } else if x < w / 2 { WAX } else { WAX_DARK });
        }
    }
    fill(p, w / 2 - 1, 5, w / 2 + 1, h - 6, GOLD);
    fill(p, 6, h / 2 - 5, w - 6, h / 2 - 3, GOLD);
    disc(p, w as f32 / 2.0, h as f32 / 2.0 - 4.0, 2.5, |_, _| GOLD_DARK);
}

/// Anything that is not gear, by the words of its name.
fn thing(p: &mut Pixels, name: &str, w: i32, h: i32) {
    let n = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| n.contains(word));
    let own = tint(name);
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    if has(&["potion", "vial", "flask", "зель", "флакон", "эликсир", "elixir"]) {
        // A round bottle with a cork and a shine.
        disc(p, cx, cy + 2.0, 5.0, |dx, dy| if dy < -0.35 { mix(own, SHINE, 0.7) } else if dx + dy < -0.6 { SHINE } else if dx > 0.5 { shade(own, 0.7) } else { own });
        fill(p, cx as i32 - 1, 2, cx as i32 + 2, 6, mix(own, SHINE, 0.6));
        fill(p, cx as i32 - 1, 1, cx as i32 + 2, 3, TIMBER);
    } else if has(&["coin", "gold", "монет", "золот"]) {
        disc(p, cx, cy, 5.5, |dx, dy| if dx * dx + dy * dy > 0.6 { GOLD_DARK } else if dx + dy < -0.3 { [252, 230, 140, 255] } else { GOLD });
    } else if has(&["ring", "кольц", "перстен"]) {
        disc(p, cx, cy + 1.0, 5.0, |dx, dy| if dx * dx + dy * dy < 0.36 { CLEAR } else if dx + dy < -0.5 { [252, 230, 140, 255] } else { GOLD });
        disc(p, cx, cy - 4.0, 2.0, |_, _| own);
    } else if has(&["key", "ключ"]) {
        disc(p, 5.0, cy, 3.5, |dx, dy| if dx * dx + dy * dy < 0.25 { CLEAR } else { GOLD });
        fill(p, 8, cy as i32 - 1, w - 2, cy as i32 + 1, GOLD);
        fill(p, w - 5, cy as i32 + 1, w - 4, cy as i32 + 4, GOLD);
        fill(p, w - 3, cy as i32 + 1, w - 2, cy as i32 + 3, GOLD);
    } else if has(&["gem", "jewel", "crystal", "самоцвет", "кристал", "алмаз", "рубин", "изумруд"]) {
        for y in 0..10 {
            let half = if y < 4 { 2 + y } else { 5 - (y - 4) * 5 / 6 };
            for x in -half..=half {
                let c = if x < 0 && y < 5 { mix(own, SHINE, 0.45) } else if x > 1 { shade(own, 0.7) } else { own };
                p.set(cx as i32 + x, 3 + y, c);
            }
        }
    } else if has(&["scroll", "letter", "map", "свит", "письм", "карт", "записк", "note"]) {
        fill(p, 3, 4, w - 3, h - 4, CLOTH);
        fill(p, 3, 4, 5, h - 4, mix(CLOTH, SHINE, 0.4));
        fill(p, 2, 2, w - 2, 5, PARCHMENT_DARK);
        fill(p, 2, h - 5, w - 2, h - 2, PARCHMENT_DARK);
        for y in (8..h - 7).step_by(3) {
            fill(p, 6, y, w - 6, y + 1, FADED);
        }
        if has(&["map", "карт"]) {
            p.set(w / 2, h / 2, WAX);
            p.set(w / 2 + 1, h / 2 + 1, WAX);
        }
    } else if has(&["book", "tome", "книг", "том", "дневник", "journal"]) {
        fill(p, 4, 3, w - 4, h - 3, own);
        fill(p, 4, 3, 7, h - 3, shade(own, 0.65));
        fill(p, w - 6, 5, w - 4, h - 5, CLOTH);
        fill(p, 11, h / 2 - 3, w - 9, h / 2 + 3, GOLD);
        fill(p, 12, h / 2 - 2, w - 10, h / 2 + 2, own);
    } else if has(&["hair", "fur", "feather", "шерст", "перо", "мех", "клок"]) {
        for k in 0..7 {
            let x = 3 + k * (w - 6) / 7;
            line(p, (x, h - 3), (x + 3 - (k % 3), 3 + k % 4), 1, mix([150, 150, 156, 255], SHINE, k as f32 / 10.0));
        }
    } else if has(&["torch", "факел", "candle", "свеч"]) {
        fill(p, w / 2 - 1, 10, w / 2 + 2, h - 2, TIMBER);
        disc(p, cx + 0.5, 7.0, 4.0, |dx, dy| if dx * dx + dy * dy < 0.3 { [255, 244, 194, 255] } else if dy < 0.0 { [255, 208, 90, 255] } else { [240, 138, 44, 255] });
    } else if has(&["rope", "верёв", "веревк"]) {
        for r in [9.0f32, 6.0, 3.0] {
            disc(p, cx, cy, r, |dx, dy| if dx * dx + dy * dy > 0.55 { if (dx * 7.0) as i32 % 2 == 0 { TIMBER } else { mix(TIMBER, CLOTH, 0.5) } } else { CLEAR });
        }
    } else if has(&["bone", "skull", "tooth", "fang", "кост", "череп", "зуб", "клык"]) {
        fill(p, 4, h / 2 - 1, w - 4, h / 2 + 2, TALLOW);
        disc(p, 4.0, cy - 2.0, 2.5, |_, _| TALLOW);
        disc(p, 4.0, cy + 3.0, 2.5, |_, _| TALLOW);
        disc(p, w as f32 - 4.0, cy - 2.0, 2.5, |_, _| TALLOW_SHADE);
        disc(p, w as f32 - 4.0, cy + 3.0, 2.5, |_, _| TALLOW_SHADE);
    } else {
        // A pouch tied at the neck, in a colour of its own.
        let body = mix(own, LEATHER, 0.55);
        disc(p, cx, h as f32 - cx - 2.0, cx - 2.5, |dx, dy| if dx + dy < -0.6 { shade(body, 1.2) } else if dx > 0.45 { shade(body, 0.75) } else { body });
        fill(p, cx as i32 - 3, 4, cx as i32 + 3, (h as f32 - 2.0 * cx + 3.0) as i32, body);
        fill(p, cx as i32 - 4, (h as f32 - 2.0 * cx + 2.0) as i32, cx as i32 + 4, (h as f32 - 2.0 * cx + 4.0) as i32, GOLD);
        fill(p, cx as i32 - 4, 2, cx as i32 + 4, 5, shade(body, 1.1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str) -> Item {
        Item { name: name.into(), description: String::new(), count: 1, gear: world::bag::gear_named(name), at: None, ..Default::default() }
    }

    #[test]
    fn each_picture_fills_its_cells_and_is_not_empty() {
        for name in ["Longsword", "Greataxe", "Longbow", "Light crossbow", "Spear", "Quarterstaff", "Mace", "Dagger", "Chain mail", "Leather armor", "Shield", "Potion of healing", "Old coin", "Ash ring", "Iron key", "Ruby", "Old map", "Book of names", "Cat hair", "Torch", "Something odd"] {
            let thing = item(name);
            let picture = icon(&thing);
            let (w, h) = thing.size();
            assert_eq!((picture.w, picture.h), (w as u32 * 16, h as u32 * 16), "{name}");
            let filled = (0..picture.h as i32).flat_map(|y| (0..picture.w as i32).map(move |x| (x, y))).filter(|(x, y)| picture.get(*x, *y)[3] != 0).count();
            assert!(filled > 20, "{name} is nearly empty");
        }
    }
}
