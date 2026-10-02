//! Heroes are drawn in code from what they are and what they carry: body, skin, hair and face
//! from the player's choices, armour, helmet, shield and weapon from the character's gear.
//! Each hero has frames for walking in four directions. A sword in the sheet is a sword in
//! the hand; plate on the sheet is plate on the body.

use crate::art::Pixels;
use crate::scene::{mix, outline, shade};
use crate::theme::{CLEAR, Rgba};
use rules::{Character, Look};

pub const W: i32 = 32;
pub const H: i32 = 48;
/// Frames of the walk; the first is also the stance.
pub const STEPS: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Facing {
    #[default]
    Down,
    Up,
    Right,
    Left,
}

impl Facing {
    /// The way someone moving by `(dx, dy)` faces; mostly sideways wins over mostly up or down.
    pub fn of(dx: f32, dy: f32) -> Option<Self> {
        if dx == 0.0 && dy == 0.0 {
            return None;
        }
        Some(if dx.abs() > dy.abs() * 0.9 {
            if dx > 0.0 { Facing::Right } else { Facing::Left }
        } else if dy > 0.0 {
            Facing::Down
        } else {
            Facing::Up
        })
    }

    fn index(self) -> usize {
        match self {
            Facing::Down => 0,
            Facing::Up => 1,
            Facing::Right | Facing::Left => 2,
        }
    }
}

// ---- the choices a player has, in order ----

pub const SKINS: [(u8, u8, u8); 9] = [(255, 222, 196), (241, 200, 164), (224, 176, 136), (198, 146, 104), (164, 112, 76), (124, 82, 56), (92, 62, 46), (150, 172, 118), (178, 120, 168)];
pub const HAIR_COLORS: [(u8, u8, u8); 11] =
    [(28, 22, 22), (66, 40, 28), (110, 70, 40), (140, 58, 36), (196, 98, 40), (222, 186, 102), (238, 230, 200), (140, 140, 146), (236, 236, 240), (60, 110, 160), (170, 40, 60)];
pub const EYE_COLORS: [(u8, u8, u8); 6] = [(92, 56, 32), (52, 110, 190), (60, 140, 80), (120, 130, 140), (210, 150, 40), (150, 70, 190)];
pub const CLOTHS: [(u8, u8, u8); 12] = [
    (168, 40, 44),
    (110, 30, 52),
    (176, 92, 44),
    (196, 156, 60),
    (104, 120, 52),
    (44, 104, 64),
    (40, 120, 124),
    (36, 52, 104),
    (60, 92, 176),
    (104, 56, 140),
    (58, 56, 64),
    (220, 206, 170),
];
pub const BUILDS: usize = 3;
pub const HAIRS: usize = 9;
pub const BEARDS: usize = 5;

fn rgb((r, g, b): (u8, u8, u8)) -> Rgba {
    [r, g, b, 255]
}

fn pick<T: Copy>(list: &[T], index: u8) -> T {
    list[index as usize % list.len()]
}

/// What the hero wears over the body.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Coat {
    Clothes,
    Robe,
    Leather,
    Mail,
    Plate,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Arm {
    None,
    Sword,
    BigSword,
    Axe,
    Mace,
    Dagger,
    Spear,
    Staff,
    Bow,
    Crossbow,
}

fn coat_of(hero: &Character) -> Coat {
    match hero.armor.as_deref().unwrap_or_default() {
        "padded" | "leather" | "studded-leather" | "hide" | "leather-armor" | "studded-leather-armor" | "hide-armor" | "padded-armor" => Coat::Leather,
        a if a.contains("leather") || a.contains("hide") || a.contains("padded") => Coat::Leather,
        a if a.contains("plate") || a.contains("splint") || a.contains("breastplate") => Coat::Plate,
        a if a.contains("mail") || a.contains("scale") || a.contains("ring") || a.contains("chain") => Coat::Mail,
        _ if matches!(hero.class.as_str(), "wizard" | "sorcerer" | "warlock" | "cleric" | "druid") => Coat::Robe,
        _ => Coat::Clothes,
    }
}

fn arm_of(hero: &Character) -> Arm {
    let main = hero.weapons.first().map(String::as_str).unwrap_or_default();
    match main {
        "greatsword" | "maul" | "greataxe" => if main == "greataxe" { Arm::Axe } else if main == "maul" { Arm::Mace } else { Arm::BigSword },
        w if w.contains("sword") || w.contains("rapier") || w.contains("scimitar") => Arm::Sword,
        w if w.contains("axe") => Arm::Axe,
        w if w.contains("mace") || w.contains("hammer") || w.contains("club") || w.contains("morningstar") || w.contains("flail") || w.contains("pick") => Arm::Mace,
        w if w.contains("dagger") || w.contains("sickle") => Arm::Dagger,
        w if w.contains("spear") || w.contains("javelin") || w.contains("trident") || w.contains("halberd") || w.contains("glaive") || w.contains("pike") || w.contains("lance") => Arm::Spear,
        w if w.contains("staff") => Arm::Staff,
        w if w.contains("crossbow") => Arm::Crossbow,
        w if w.contains("bow") => Arm::Bow,
        _ => Arm::None,
    }
}

/// The colours a hero is drawn in.
struct Tones {
    skin: Rgba,
    hair: Rgba,
    eyes: Rgba,
    cloth: Rgba,
    trim: Rgba,
    leather: Rgba,
    steel: Rgba,
    wood: Rgba,
    boots: Rgba,
    ink: Rgba,
}

impl Tones {
    fn of(look: &Look) -> Self {
        Self {
            skin: rgb(pick(&SKINS, look.skin)),
            hair: rgb(pick(&HAIR_COLORS, look.hair_color)),
            eyes: rgb(pick(&EYE_COLORS, look.eyes)),
            cloth: rgb(pick(&CLOTHS, look.cloth)),
            trim: rgb(pick(&CLOTHS, look.trim)),
            leather: [124, 78, 44, 255],
            steel: [176, 184, 196, 255],
            wood: [120, 80, 46, 255],
            boots: [72, 48, 36, 255],
            ink: [24, 18, 20, 255],
        }
    }
}

/// Where the parts of a body are, for one people's height and one build.
#[derive(Clone, Copy)]
struct Frame {
    /// The top of the head.
    head: i32,
    /// Shoulders, waist and the top of the legs.
    shoulder: i32,
    waist: i32,
    hip: i32,
    /// The soles of the feet.
    sole: i32,
    /// Half the width of the torso.
    half: i32,
}

fn frame_of(hero: &Character, look: &Look) -> Frame {
    let short = matches!(hero.race.as_str(), "dwarf" | "halfling" | "gnome");
    let drop = if short { 5 } else { 0 };
    let half = match look.build % BUILDS as u8 {
        0 => 5,
        1 => 6,
        _ => 7,
    } + if hero.race == "dwarf" { 1 } else { 0 };
    let leg = if short { 6 } else { 10 };
    let sole = H - 2;
    let hip = sole - leg;
    let waist = hip - 2;
    let shoulder = hip - 12 + if short { 1 } else { 0 };
    Frame { head: shoulder - 14 + drop.min(1), shoulder, waist, hip, sole, half }
}

fn fill(p: &mut Pixels, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgba) {
    for y in y0..y1 {
        for x in x0..x1 {
            p.set(x, y, c);
        }
    }
}

/// A block lit from the upper left: a lighter edge on the left and top, a darker one on the
/// right and bottom.
fn block(p: &mut Pixels, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgba) {
    fill(p, x0, y0, x1, y1, c);
    fill(p, x0, y0, x0 + 1, y1, shade(c, 1.12));
    fill(p, x1 - 1, y0, x1, y1, shade(c, 0.78));
    fill(p, x0, y1 - 1, x1, y1, shade(c, 0.82));
}

/// One frame of a hero.
fn draw(hero: &Character, facing: Facing, step: usize) -> Pixels {
    let look = hero.look;
    let t = Tones::of(&look);
    let f = frame_of(hero, &look);
    let coat = coat_of(hero);
    let arm = arm_of(hero);
    let mut p = Pixels::new(W as u32, H as u32, CLEAR);
    let cx = W / 2;
    // The walk in four frames: passing (which is also the stance), a stride, passing again, the
    // other stride. The body sinks a pixel on a stride, when both feet are on the ground.
    let swing: i32 = match step % STEPS {
        1 => 1,
        3 => -1,
        _ => 0,
    };
    let bob = swing.abs();
    let side = matches!(facing, Facing::Right | Facing::Left);
    let back = facing == Facing::Up;
    let helmet = look.helmet && matches!(coat, Coat::Mail | Coat::Plate);
    let torso_color = match coat {
        Coat::Clothes | Coat::Robe => t.cloth,
        Coat::Leather => t.leather,
        Coat::Mail => mix(t.steel, t.ink, 0.25),
        Coat::Plate => t.steel,
    };
    let legs_color = match coat {
        Coat::Plate => shade(t.steel, 0.9),
        _ => mix(t.cloth, t.ink, 0.45),
    };
    let half = if side { f.half - 2 } else { f.half };

    // Long hair falls behind the body.
    let long_hair = matches!(look.hair % HAIRS as u8, 3 | 4 | 8) && !helmet;
    if long_hair && !side {
        let reach = if look.hair % HAIRS as u8 == 3 { f.waist - 2 } else { f.shoulder + 4 };
        fill(&mut p, cx - 7, f.head + 6 + bob, cx + 7, reach + bob, shade(t.hair, 0.82));
    }

    // Legs and boots.
    let leg_w = 4;
    let boot = if coat == Coat::Plate { shade(t.steel, 0.8) } else { t.boots };
    if side {
        // Seen from the side the legs scissor: each runs from the hip to its foot on a slant,
        // the far one darker and drawn first. On the second passing frame the far foot is up.
        let hip_x = cx - leg_w / 2;
        let legs = [(swing * 4, if step % STEPS == 2 { 2 } else { 0 }, true), (-swing * 5, 0, false)];
        for (reach, raise, far) in legs {
            let color = if far { shade(legs_color, 0.72) } else { legs_color };
            let (top, bottom) = (f.hip + bob, f.sole - 3 - raise);
            for y in top..bottom {
                let x = hip_x + (reach * (y - top) + (bottom - top) / 2) / (bottom - top).max(1);
                fill(&mut p, x, y, x + leg_w, y + 1, color);
                p.set(x, y, shade(color, 1.1));
                p.set(x + leg_w - 1, y, shade(color, 0.8));
            }
            let foot = hip_x + reach;
            block(&mut p, foot, bottom, foot + leg_w + 1, bottom + 3, if far { shade(boot, 0.75) } else { boot });
        }
    } else {
        // From the front or back: the leading foot comes a pixel nearer, the trailing heel lifts.
        for dir in [-1, 1] {
            let x0 = if dir < 0 { cx - 1 - leg_w } else { cx + 1 };
            let lead = swing * dir < 0;
            let trail = swing * dir > 0;
            let passing_lift = if step % STEPS == 2 && dir < 0 { 1 } else { 0 };
            let y_end = f.sole + if lead { 1 } else { 0 } - if trail { 2 } else { 0 } - passing_lift;
            let color = if trail { shade(legs_color, 0.85) } else { legs_color };
            block(&mut p, x0, f.hip + bob, x0 + leg_w, y_end - 3, color);
            block(&mut p, x0, y_end - 3, x0 + leg_w, y_end, boot);
        }
    }

    // A robe hides the legs down to the boots.
    if coat == Coat::Robe {
        let flare = if side { 1 } else { 2 };
        for y in f.waist + bob..f.sole - 3 {
            let grow = (y - f.waist) / 4;
            fill(&mut p, cx - half - grow.min(flare), y, cx + half + grow.min(flare), y + 1, shade(t.cloth, if y > f.sole - 6 { 0.85 } else { 1.0 }));
        }
        fill(&mut p, cx - half - flare, f.sole - 4, cx + half + flare, f.sole - 3, t.trim);
    }

    // The far arm, behind the body when seen from the side.
    let arm_top = f.shoulder + 1 + bob;
    let arm_len = f.waist - f.shoulder + 1;
    let sleeve = match coat {
        Coat::Plate => t.steel,
        Coat::Mail => mix(t.steel, t.ink, 0.3),
        _ => shade(t.cloth, 0.92),
    };
    let hand = if coat == Coat::Plate { shade(t.steel, 0.85) } else { t.skin };
    if side {
        let ax = cx - 1 - swing * 2;
        block(&mut p, ax, arm_top, ax + 3, arm_top + arm_len - 2, shade(sleeve, 0.75));
        fill(&mut p, ax, arm_top + arm_len - 2, ax + 3, arm_top + arm_len, shade(hand, 0.8));
        if hero.shield {
            shield(&mut p, &t, ax - 4 - if facing == Facing::Left { 0 } else { 0 }, arm_top + 2, true);
        }
    }

    // The torso, rounded at the shoulders, with its far side and its lower part in shadow.
    block(&mut p, cx - half, f.shoulder + bob, cx + half, f.hip + bob, torso_color);
    fill(&mut p, cx + half - 3, f.shoulder + 1 + bob, cx + half - 1, f.hip + bob, shade(torso_color, 0.88));
    fill(&mut p, cx - half + 1, f.shoulder + bob, cx - half + 3, f.shoulder + 3 + bob, shade(torso_color, 1.1));
    p.set(cx - half, f.shoulder + bob, CLEAR);
    p.set(cx + half - 1, f.shoulder + bob, CLEAR);
    match coat {
        Coat::Mail => {
            for y in f.shoulder + 1 + bob..f.hip + bob {
                for x in cx - half + 1..cx + half - 1 {
                    if (x + y) % 2 == 0 {
                        p.set(x, y, mix(t.steel, t.ink, 0.45));
                    }
                }
            }
            // A tabard in the hero's colour over the mail.
            if !back {
                fill(&mut p, cx - 2, f.shoulder + 2 + bob, cx + 2, f.hip + 1 + bob, t.cloth);
                fill(&mut p, cx - 2, f.shoulder + 2 + bob, cx - 1, f.hip + 1 + bob, shade(t.cloth, 1.15));
            }
        }
        Coat::Plate => {
            // Plates with a shining ridge, and pauldrons on the shoulders.
            fill(&mut p, cx - 1, f.shoulder + 1 + bob, cx, f.waist + bob, mix(t.steel, [255, 255, 255, 255], 0.5));
            fill(&mut p, cx - half, f.waist - 4 + bob, cx + half, f.waist - 3 + bob, shade(t.steel, 0.7));
            if !side {
                block(&mut p, cx - half - 2, f.shoulder - 1 + bob, cx - half + 3, f.shoulder + 3 + bob, shade(t.steel, 1.05));
                block(&mut p, cx + half - 3, f.shoulder - 1 + bob, cx + half + 2, f.shoulder + 3 + bob, shade(t.steel, 0.95));
            } else {
                block(&mut p, cx - 3, f.shoulder - 1 + bob, cx + 3, f.shoulder + 3 + bob, shade(t.steel, 1.05));
            }
        }
        Coat::Leather => {
            // Straps across the chest.
            if !side {
                for i in 0..(f.hip - f.shoulder - 2) {
                    p.set(cx - half + 1 + i, f.shoulder + 1 + i + bob, shade(t.leather, 0.65));
                }
            }
            fill(&mut p, cx - half, f.shoulder + bob, cx + half, f.shoulder + 1 + bob, shade(t.leather, 1.2));
        }
        Coat::Clothes | Coat::Robe => {
            // A collar of the trim colour.
            if !back {
                fill(&mut p, cx - 2, f.shoulder + bob, cx + 2, f.shoulder + 2 + bob, t.trim);
            }
        }
    }
    // The belt, with a buckle.
    fill(&mut p, cx - half, f.waist + bob, cx + half, f.waist + 2 + bob, shade(t.leather, 0.6));
    if !back && !side {
        fill(&mut p, cx - 1, f.waist + bob, cx + 1, f.waist + 2 + bob, [222, 190, 92, 255]);
    }

    // The arms in front: both from the front or back, the near one from the side.
    let arms: Vec<(i32, i32)> = if side { vec![(cx + 1 + swing * 2, 0)] } else { vec![(cx - half - 3, swing), (cx + half, -swing)] };
    for (ax, lift) in &arms {
        let top = arm_top + lift;
        block(&mut p, *ax, top, ax + 3, top + arm_len - 2, sleeve);
        fill(&mut p, *ax, top + arm_len - 2, ax + 3, top + arm_len, hand);
    }

    // The head.
    let head_top = f.head + bob;
    let head_w = if side { 12 } else { 14 };
    let hx = cx - head_w / 2 + if side { 1 } else { 0 };
    // The neck, in the shadow of the head.
    fill(&mut p, cx - 2, head_top + 13, cx + 2, f.shoulder + 1 + bob, shade(t.skin, 0.72));
    // A rounded head, lit from the upper left: a light brow and cheek, a shaded far side and jaw.
    for y in 0..12 {
        for x in 0..head_w {
            let corner = |a: i32, b: i32| a.min(b);
            let (dx, dy) = (corner(x, head_w - 1 - x), corner(y, 11 - y));
            // Cut the corners round: three pixels off at the top, two at the jaw.
            let cut = if y < 6 { [3, 1, 0][dy.min(2) as usize] } else { [2, 1, 0][dy.min(2) as usize] };
            if dx < cut {
                continue;
            }
            let mut c = t.skin;
            if x >= head_w - 3 && !back || y >= 10 {
                c = shade(c, 0.84);
            } else if x <= 2 && y <= 5 {
                c = shade(c, 1.07);
            }
            p.set(hx + x, head_top + 2 + y, c);
        }
    }
    if !back && !side {
        // Cheeks with a little colour.
        let blush = mix(t.skin, [220, 90, 90, 255], 0.18);
        p.set(hx + 2, head_top + 10, blush);
        p.set(hx + head_w - 3, head_top + 10, shade(blush, 0.9));
    }
    // Pointed ears for the elves, tusks for the half-orcs, horns for the tieflings.
    let elf = matches!(hero.race.as_str(), "elf" | "half-elf");
    if elf && !back {
        let ear = shade(t.skin, 0.92);
        if side {
            fill(&mut p, hx + 2, head_top + 6, hx + 4, head_top + 9, ear);
            p.set(hx + 1, head_top + 5, ear);
        } else {
            fill(&mut p, hx - 2, head_top + 7, hx, head_top + 9, ear);
            fill(&mut p, hx + head_w, head_top + 7, hx + head_w + 2, head_top + 9, ear);
            p.set(hx - 3, head_top + 6, ear);
            p.set(hx + head_w + 2, head_top + 6, ear);
        }
    }

    // The face.
    if !back {
        let eye_y = head_top + 8;
        let eyes: Vec<i32> = if side { vec![hx + head_w - 4] } else { vec![hx + 3, hx + head_w - 5] };
        for ex in eyes {
            p.set(ex, eye_y, [250, 248, 240, 255]);
            p.set(ex + 1, eye_y, t.eyes);
            p.set(ex, eye_y + 1, shade(t.eyes, 0.6));
            p.set(ex + 1, eye_y + 1, t.ink);
        }
        let mouth = if side { (hx + head_w - 3, hx + head_w - 1) } else { (cx - 1, cx + 1) };
        fill(&mut p, mouth.0, head_top + 11, mouth.1, head_top + 12, shade(t.skin, 0.68));
        if hero.race == "half-orc" {
            p.set(mouth.0 - 1, head_top + 11, [240, 236, 220, 255]);
            if !side {
                p.set(mouth.1, head_top + 11, [240, 236, 220, 255]);
            }
        }
        // The beard.
        match look.beard % BEARDS as u8 {
            1 => {
                for x in hx + 1..hx + head_w - 1 {
                    if x % 2 == 0 {
                        p.set(x, head_top + 12, shade(t.hair, 0.9));
                    }
                }
            }
            2 => fill(&mut p, hx + 1, head_top + 10, hx + head_w - 1, head_top + 15, t.hair),
            3 => {
                fill(&mut p, hx + 1, head_top + 10, hx + head_w - 1, head_top + 16, t.hair);
                fill(&mut p, cx - 3 + if side { 3 } else { 0 }, head_top + 16, cx + 3 + if side { 2 } else { 0 }, head_top + 21, shade(t.hair, 0.9));
            }
            4 => fill(&mut p, mouth.0 - 2, head_top + 10, mouth.1 + 2, head_top + 11, t.hair),
            _ => {}
        }
        if matches!(look.beard % BEARDS as u8, 2 | 3) {
            fill(&mut p, mouth.0, head_top + 11, mouth.1, head_top + 12, shade(t.skin, 0.6));
        }
    }

    // The hair, or the helmet over it.
    if helmet {
        match coat {
            Coat::Plate => {
                block(&mut p, hx - 1, head_top, hx + head_w + 1, head_top + 13, t.steel);
                if !back {
                    let (v0, v1) = if side { (hx + head_w - 6, hx + head_w + 1) } else { (hx + 2, hx + head_w - 2) };
                    fill(&mut p, v0, head_top + 7, v1, head_top + 9, t.ink);
                    if !side {
                        fill(&mut p, cx - 1, head_top + 9, cx, head_top + 12, t.ink);
                    }
                }
                // A plume in the trim colour.
                fill(&mut p, cx - 1, head_top - 3, cx + 1, head_top + 1, t.trim);
                p.set(cx - 2 + if side { -1 } else { 0 }, head_top - 3, t.trim);
            }
            _ => {
                // A mail coif framing the face.
                block(&mut p, hx - 1, head_top, hx + head_w + 1, head_top + 5, mix(t.steel, t.ink, 0.3));
                fill(&mut p, hx - 1, head_top + 5, hx + 1, head_top + 15, mix(t.steel, t.ink, 0.3));
                fill(&mut p, hx + head_w - 1, head_top + 5, hx + head_w + 1, head_top + 15, mix(t.steel, t.ink, 0.35));
                if back {
                    fill(&mut p, hx - 1, head_top, hx + head_w + 1, head_top + 15, mix(t.steel, t.ink, 0.3));
                }
            }
        }
    } else {
        hair(&mut p, &t, look.hair % HAIRS as u8, hx, head_top, head_w, facing);
    }
    if hero.race == "tiefling" {
        let horn = [70, 40, 50, 255];
        for (x, y) in [(hx + 1, head_top), (hx, head_top - 1), (hx, head_top - 2), (hx + head_w - 2, head_top), (hx + head_w - 1, head_top - 1), (hx + head_w - 1, head_top - 2)] {
            p.set(x, y, horn);
        }
    }

    // The shield on the left arm, the weapon in the right hand.
    if hero.shield && !side {
        let (sx, flat) = if back { (cx - half - 6, true) } else { (cx + half + 1, false) };
        shield(&mut p, &t, sx, arm_top + 2 - swing, flat);
    }
    let hand_at = match facing {
        Facing::Down => (cx - half - 2, arm_top + arm_len - 1 + swing),
        Facing::Up => (cx + half + 1, arm_top + arm_len - 1 - swing),
        _ => (cx + 2 + swing * 2, arm_top + arm_len - 1),
    };
    weapon(&mut p, &t, arm, hand_at, side, back);

    outline(&mut p, t.ink);
    if facing == Facing::Left { p.flipped() } else { p }
}

fn hair(p: &mut Pixels, t: &Tones, style: u8, hx: i32, top: i32, w: i32, facing: Facing) {
    let back = facing == Facing::Up;
    let side = matches!(facing, Facing::Right | Facing::Left);
    let c = t.hair;
    let lit = shade(c, 1.2);
    match style {
        // Shaved.
        1 => {}
        // Short: a cap over the crown.
        0 => {
            cap(p, hx, top + 1, w, 5, c);
            fill(p, hx, top + 4, hx + 1, top + 8, c);
            fill(p, hx + w - 1, top + 4, hx + w, top + 8, c);
            if back {
                cap(p, hx, top + 1, w, 10, c);
            }
        }
        // Mohawk: a crest down the middle.
        5 => fill(p, hx + w / 2 - 2, top - 2, hx + w / 2 + 1, top + if back { 12 } else { 5 }, c),
        // A bun on top.
        7 => {
            fill(p, hx, top + 1, hx + w, top + 5, c);
            fill(p, hx + w / 2 - 3, top - 3, hx + w / 2 + 3, top + 1, c);
            if back {
                fill(p, hx, top + 1, hx + w, top + 11, c);
            }
        }
        // Curly: a round mass with a bumpy edge.
        6 => {
            fill(p, hx - 1, top - 1, hx + w + 1, top + 6, c);
            for x in (hx - 1..hx + w + 1).step_by(2) {
                p.set(x, top - 2, c);
            }
            fill(p, hx - 1, top + 6, hx + 1, top + 11, c);
            fill(p, hx + w - 1, top + 6, hx + w + 1, top + 11, c);
            if back {
                fill(p, hx - 1, top - 1, hx + w + 1, top + 13, c);
            }
        }
        // Medium, long, ponytail, braids: hair to the ears or past them, with a fringe.
        _ => {
            cap(p, hx - 1, top, w + 2, 6, c);
            let down = match style {
                2 => 10,
                4 => 7,
                _ => 13,
            };
            if side {
                fill(p, hx - 1, top + 3, hx + 4, top + down, c);
            } else {
                fill(p, hx - 1, top + 3, hx + 1, top + down, c);
                fill(p, hx + w - 1, top + 3, hx + w + 1, top + down, c);
            }
            if back {
                fill(p, hx - 1, top, hx + w + 1, top + down + 1, c);
            }
            // A ponytail sticks out behind; braids hang in front.
            if style == 4 {
                let tail_x = if side { hx - 3 } else { hx + w / 2 - 1 };
                if side || back {
                    fill(p, tail_x, top + 5, tail_x + 3, top + 14, shade(c, 0.9));
                }
            }
            if style == 8 && !back && !side {
                fill(p, hx - 1, top + 13, hx + 1, top + 20, shade(c, 0.9));
                fill(p, hx + w - 1, top + 13, hx + w + 1, top + 20, shade(c, 0.9));
            }
        }
    }
    // A shine on the hair, from the upper left.
    if style != 1 && !back {
        fill(p, hx + 1, top + 1, hx + 4, top + 2, lit);
    }
}

/// Hair over the crown, rounded like the head under it.
fn cap(p: &mut Pixels, x: i32, top: i32, w: i32, rows: i32, c: Rgba) {
    for y in 0..rows {
        let cut = [3, 1, 1, 0][y.min(3) as usize];
        fill(p, x + cut, top + y, x + w - cut, top + y + 1, if y == rows - 1 { shade(c, 0.85) } else { c });
    }
}

fn shield(p: &mut Pixels, t: &Tones, x: i32, y: i32, flat: bool) {
    let (w, h) = (if flat { 5 } else { 9 }, 12);
    for yy in 0..h {
        // Narrowing at the bottom into a point.
        let inset = if yy > h - 4 { yy - (h - 4) } else { 0 };
        for xx in inset..w - inset {
            let rim = xx == inset || xx == w - inset - 1 || yy == 0;
            p.set(x + xx, y + yy, if rim { shade(t.steel, 0.85) } else { mix(t.wood, t.trim, 0.55) });
        }
    }
    if !flat {
        // The boss, and a stripe of the trim colour.
        fill(p, x + w / 2 - 1, y + 4, x + w / 2 + 1, y + 6, [222, 190, 92, 255]);
        fill(p, x + 1, y + 2, x + w - 1, y + 3, shade(t.trim, 1.15));
    }
}

fn weapon(p: &mut Pixels, t: &Tones, arm: Arm, (hx, hy): (i32, i32), side: bool, back: bool) {
    let blade = mix(t.steel, [255, 255, 255, 255], 0.35);
    let edge = shade(t.steel, 0.7);
    let grip = shade(t.wood, 0.8);
    let gold = [222, 190, 92, 255];
    // Seen from the back the weapon is partly behind the hand: drawn the same, lower.
    let hy = if back { hy + 1 } else { hy };
    match arm {
        Arm::None => {}
        Arm::Sword | Arm::BigSword => {
            let length = if arm == Arm::BigSword { 18 } else { 13 };
            let lean = if side { 1 } else { 0 };
            for i in 0..length {
                let x = hx + 1 + if side { i / 4 } else { 0 } * lean;
                p.set(x, hy - 2 - i, blade);
                p.set(x + 1, hy - 2 - i, edge);
            }
            fill(p, hx - 1, hy - 2, hx + 4, hy - 1, gold);
            fill(p, hx + 1, hy - 1, hx + 3, hy + 2, grip);
        }
        Arm::Dagger => {
            for i in 0..6 {
                p.set(hx + 1, hy - 1 - i, blade);
            }
            fill(p, hx, hy - 1, hx + 3, hy, gold);
        }
        Arm::Axe => {
            fill(p, hx + 1, hy - 12, hx + 2, hy + 3, t.wood);
            block(p, hx - 3, hy - 12, hx + 1, hy - 6, t.steel);
            fill(p, hx - 4, hy - 11, hx - 3, hy - 7, blade);
        }
        Arm::Mace => {
            fill(p, hx + 1, hy - 10, hx + 2, hy + 3, t.wood);
            block(p, hx - 1, hy - 14, hx + 4, hy - 9, t.steel);
            p.set(hx, hy - 13, blade);
        }
        Arm::Spear => {
            fill(p, hx + 1, hy - 22, hx + 2, hy + 6, t.wood);
            fill(p, hx, hy - 25, hx + 3, hy - 22, t.steel);
            p.set(hx + 1, hy - 26, blade);
        }
        Arm::Staff => {
            fill(p, hx + 1, hy - 22, hx + 2, hy + 7, t.wood);
            fill(p, hx, hy - 25, hx + 3, hy - 22, t.trim);
            p.set(hx + 1, hy - 24, [250, 248, 220, 255]);
        }
        Arm::Bow => {
            // An arc taller than the hero's arm, and its string.
            for i in -9..=9i32 {
                let bend = 3 - (i.abs() * 3) / 9;
                p.set(hx - bend, hy + i - 4, t.wood);
                p.set(hx + 2, hy + i - 4, [230, 226, 210, 255]);
            }
        }
        Arm::Crossbow => {
            fill(p, hx - 3, hy - 2, hx + 5, hy, t.wood);
            fill(p, hx - 4, hy - 4, hx - 3, hy + 2, t.steel);
            fill(p, hx + 5, hy - 4, hx + 6, hy + 2, t.steel);
        }
    }
    let _ = grip;
}

/// Every frame of a hero: three directions (the fourth is the third mirrored) of four steps.
pub fn frames(hero: &Character) -> Vec<Pixels> {
    let mut all = Vec::new();
    for facing in [Facing::Down, Facing::Up, Facing::Right, Facing::Left] {
        for step in 0..STEPS {
            all.push(draw(hero, facing, step));
        }
    }
    all
}

/// Which of the frames from `frames` to show.
pub fn frame_index(facing: Facing, step: usize) -> usize {
    let direction = match facing {
        Facing::Left => 3,
        other => other.index(),
    };
    direction * STEPS + step % STEPS
}

/// A key that changes when anything that shows on the hero changes.
pub fn key(hero: &Character) -> String {
    format!("{:?}|{}|{}|{:?}|{}|{:?}", hero.look, hero.race, hero.class, hero.armor, hero.shield, hero.weapons.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hero(class: &str, race: &str) -> Character {
        let data = rules::srd();
        let c = data.class(class).unwrap();
        let skills: Vec<String> = c.skill_options.iter().take(c.skill_picks).cloned().collect();
        Character::create("Test", "", race, class, [15, 14, 13, 12, 10, 8], &skills, &[]).unwrap()
    }

    #[test]
    fn every_frame_is_the_same_size_and_stands_on_the_bottom_rows() {
        let all = frames(&hero("fighter", "human"));
        assert_eq!(all.len(), 4 * STEPS);
        for frame in &all {
            assert_eq!((frame.w as i32, frame.h as i32), (W, H));
            assert!((0..W).any(|x| frame.get(x, H - 2)[3] != 0 || frame.get(x, H - 1)[3] != 0), "feet on the ground");
            assert_eq!(frame.get(0, 0)[3], 0);
        }
    }

    #[test]
    fn walking_moves_the_legs_and_left_is_right_mirrored() {
        let all = frames(&hero("fighter", "human"));
        let differs = |a: &Pixels, b: &Pixels| (0..H).any(|y| (0..W).any(|x| a.get(x, y) != b.get(x, y)));
        assert!(differs(&all[frame_index(Facing::Down, 0)], &all[frame_index(Facing::Down, 2)]));
        let right = &all[frame_index(Facing::Right, 1)];
        let left = &all[frame_index(Facing::Left, 1)];
        assert!((0..H).all(|y| (0..W).all(|x| right.get(x, y) == left.get(W - 1 - x, y))));
    }

    #[test]
    fn what_is_carried_shows_and_what_is_chosen_shows() {
        let fighter = hero("fighter", "human");
        let wizard = hero("wizard", "human");
        let differs = |a: &Pixels, b: &Pixels| (0..H).filter(|y| (0..W).any(|x| a.get(x, *y) != b.get(x, *y))).count();
        let (f, w) = (draw(&fighter, Facing::Down, 1), draw(&wizard, Facing::Down, 1));
        assert!(differs(&f, &w) > 10, "chain and a sword are not a robe and a staff");
        let mut red = fighter.clone();
        red.look.hair_color = 10;
        red.look.hair = 2;
        assert!(differs(&f, &draw(&red, Facing::Down, 1)) > 2);
        let mut helmed = fighter.clone();
        helmed.look.helmet = true;
        assert!(differs(&f, &draw(&helmed, Facing::Down, 1)) > 2);
        assert_ne!(key(&fighter), key(&helmed));
    }

    #[test]
    fn short_peoples_are_shorter() {
        let tall = draw(&hero("fighter", "human"), Facing::Down, 1);
        let short = draw(&hero("fighter", "dwarf"), Facing::Down, 1);
        let top = |p: &Pixels| (0..H).find(|y| (0..W).any(|x| p.get(x, *y)[3] != 0)).unwrap();
        assert!(top(&short) > top(&tall));
    }

    #[test]
    fn facing_follows_the_movement() {
        assert_eq!(Facing::of(1.0, 0.2), Some(Facing::Right));
        assert_eq!(Facing::of(-1.0, 0.0), Some(Facing::Left));
        assert_eq!(Facing::of(0.1, 1.0), Some(Facing::Down));
        assert_eq!(Facing::of(0.0, -1.0), Some(Facing::Up));
        assert_eq!(Facing::of(0.0, 0.0), None);
    }
}

/// Every frame of a few heroes on one sheet, for a person to look at:
/// `DOLL_SHEET=out.png cargo test -p pixelgm doll_sheet -- --ignored`.
#[cfg(test)]
#[test]
#[ignore]
fn doll_sheet() {
    let data = rules::srd();
    let make = |class: &str, race: &str, look: rules::Look| {
        let c = data.class(class).unwrap();
        let skills: Vec<String> = c.skill_options.iter().take(c.skill_picks).cloned().collect();
        let mut hero = Character::create("Test", "", race, class, [15, 14, 13, 12, 10, 8], &skills, &[]).unwrap();
        hero.look = look;
        hero
    };
    let heroes = [
        make("fighter", "human", rules::Look::default()),
        make("wizard", "elf", rules::Look { build: 0, skin: 1, hair: 4, hair_color: 6, beard: 0, eyes: 1, cloth: 5, trim: 2, helmet: false }),
        make("cleric", "dwarf", rules::Look { build: 2, skin: 3, hair: 2, hair_color: 3, beard: 3, eyes: 0, cloth: 8, trim: 1, helmet: true }),
        make("rogue", "halfling", rules::Look { build: 1, skin: 5, hair: 7, hair_color: 1, beard: 0, eyes: 2, cloth: 3, trim: 4, helmet: false }),
    ];
    let scale = 4;
    let mut sheet = Pixels::new((W * 16 * scale) as u32, (H * heroes.len() as i32 * scale) as u32, [70, 90, 60, 255]);
    for (row, hero) in heroes.iter().enumerate() {
        for (n, frame) in frames(hero).iter().enumerate() {
            for y in 0..H * scale {
                for x in 0..W * scale {
                    let c = frame.get(x / scale, y / scale);
                    if c[3] != 0 {
                        sheet.set(n as i32 * W * scale + x, row as i32 * H * scale + y, c);
                    }
                }
            }
        }
    }
    let out = std::env::var("DOLL_SHEET").unwrap_or_else(|_| "doll_sheet.png".into());
    sheet.into_image().try_into_dynamic().unwrap().save(out).unwrap();
}
