//! Dice. Every random number in the game comes from an `Rng` that can be seeded, so a fight
//! can be replayed in a test and, later, checked by every player's machine.

use serde::{Deserialize, Serialize};

/// A small, fast generator (xorshift64*). Good enough for dice; not for secrets.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Rng(u64);

impl Rng {
    pub fn seeded(seed: u64) -> Self {
        // A zero state would stay zero forever.
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn from_clock() -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        Self::seeded(nanos)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// One die of `sides` sides: 1 to `sides`.
    pub fn die(&mut self, sides: u32) -> u32 {
        if sides == 0 {
            return 0;
        }
        (self.next() >> 33) as u32 % sides + 1
    }
}

/// How a d20 is rolled.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Edge {
    #[default]
    Plain,
    Advantage,
    Disadvantage,
}

impl Edge {
    /// Advantage and disadvantage cancel, however many of each there are.
    pub fn of(advantage: bool, disadvantage: bool) -> Self {
        match (advantage, disadvantage) {
            (true, false) => Edge::Advantage,
            (false, true) => Edge::Disadvantage,
            _ => Edge::Plain,
        }
    }
}

/// A d20 roll with its bonus, kept apart so the interface can show the die itself.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Roll {
    /// The die that counted.
    pub die: u32,
    /// The die that was discarded under advantage or disadvantage.
    pub other: Option<u32>,
    pub bonus: i32,
}

impl Roll {
    pub fn d20(rng: &mut Rng, bonus: i32, edge: Edge) -> Self {
        let (a, b) = (rng.die(20), rng.die(20));
        match edge {
            Edge::Plain => Roll { die: a, other: None, bonus },
            Edge::Advantage => Roll { die: a.max(b), other: Some(a.min(b)), bonus },
            Edge::Disadvantage => Roll { die: a.min(b), other: Some(a.max(b)), bonus },
        }
    }

    pub fn total(&self) -> i32 {
        self.die as i32 + self.bonus
    }

    pub fn natural_20(&self) -> bool {
        self.die == 20
    }

    pub fn natural_1(&self) -> bool {
        self.die == 1
    }
}

/// A dice expression such as `2d6+3`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Dice {
    pub count: u32,
    pub sides: u32,
    pub bonus: i32,
}

impl Dice {
    pub const fn new(count: u32, sides: u32, bonus: i32) -> Self {
        Self { count, sides, bonus }
    }

    /// Reads `2d6`, `1d8+3`, `1d10 - 1`, a bare number, or the SRD's `1d8 + MOD`, where
    /// `modifier` stands in for MOD. Anything else is `None`.
    pub fn parse(text: &str, modifier: i32) -> Option<Self> {
        let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
        let (dice_part, bonus) = match compact.find(['+', '-']) {
            Some(at) if at > 0 => {
                let (dice, rest) = compact.split_at(at);
                let sign = if rest.starts_with('-') { -1 } else { 1 };
                let amount = if &rest[1..] == "mod" { modifier } else { rest[1..].parse::<i32>().ok()? };
                (dice.to_string(), sign * amount)
            }
            _ => (compact.clone(), 0),
        };
        match dice_part.split_once('d') {
            Some((count, sides)) => Some(Self { count: if count.is_empty() { 1 } else { count.parse().ok()? }, sides: sides.parse().ok()?, bonus }),
            None => Some(Self { count: 0, sides: 0, bonus: bonus + dice_part.parse::<i32>().ok()? }),
        }
    }

    pub fn roll(&self, rng: &mut Rng) -> i32 {
        (0..self.count).map(|_| rng.die(self.sides) as i32).sum::<i32>() + self.bonus
    }

    /// A critical hit rolls the dice twice; the flat bonus is added once.
    pub fn roll_critical(&self, rng: &mut Rng) -> i32 {
        Dice { count: self.count * 2, ..*self }.roll(rng)
    }

    pub fn average(&self) -> f32 {
        self.count as f32 * (self.sides as f32 + 1.0) / 2.0 + self.bonus as f32
    }
}

impl std::fmt::Display for Dice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.count, self.bonus) {
            (0, bonus) => write!(f, "{bonus}"),
            (count, 0) => write!(f, "{count}d{}", self.sides),
            (count, bonus) if bonus > 0 => write!(f, "{count}d{}+{bonus}", self.sides),
            (count, bonus) => write!(f, "{count}d{}{bonus}", self.sides),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_dice() {
        let rolls = |seed| {
            let mut rng = Rng::seeded(seed);
            (0..20).map(|_| rng.die(20)).collect::<Vec<_>>()
        };
        assert_eq!(rolls(7), rolls(7));
        assert_ne!(rolls(7), rolls(8));
    }

    #[test]
    fn a_die_stays_within_its_faces_and_shows_every_one() {
        let mut rng = Rng::seeded(1);
        let mut seen = [false; 6];
        for _ in 0..600 {
            let face = rng.die(6);
            assert!((1..=6).contains(&face));
            seen[face as usize - 1] = true;
        }
        assert!(seen.iter().all(|s| *s));
        assert_eq!(rng.die(0), 0);
    }

    #[test]
    fn dice_expressions_are_read_in_the_forms_the_srd_uses() {
        assert_eq!(Dice::parse("2d6", 0), Some(Dice::new(2, 6, 0)));
        assert_eq!(Dice::parse("1d8+3", 0), Some(Dice::new(1, 8, 3)));
        assert_eq!(Dice::parse("1d10 - 1", 0), Some(Dice::new(1, 10, -1)));
        assert_eq!(Dice::parse("1d8 + MOD", 4), Some(Dice::new(1, 8, 4)));
        assert_eq!(Dice::parse("d4", 0), Some(Dice::new(1, 4, 0)));
        assert_eq!(Dice::parse("5", 0), Some(Dice::new(0, 0, 5)));
        assert_eq!(Dice::parse("fire", 0), None);
        assert_eq!(Dice::parse("2d", 0), None);
    }

    #[test]
    fn dice_print_the_way_they_are_written() {
        for text in ["2d6", "1d8+3", "1d10-1", "5"] {
            assert_eq!(Dice::parse(text, 0).unwrap().to_string(), text);
        }
    }

    #[test]
    fn a_roll_stays_between_its_least_and_most() {
        let mut rng = Rng::seeded(3);
        let dice = Dice::new(2, 6, 3);
        for _ in 0..200 {
            assert!((5..=15).contains(&dice.roll(&mut rng)));
            assert!((7..=27).contains(&dice.roll_critical(&mut rng)));
        }
        assert_eq!(dice.average(), 10.0);
    }

    #[test]
    fn advantage_keeps_the_higher_die_and_disadvantage_the_lower() {
        let mut rng = Rng::seeded(11);
        for _ in 0..100 {
            let high = Roll::d20(&mut rng, 2, Edge::Advantage);
            assert!(high.die >= high.other.unwrap());
            assert_eq!(high.total(), high.die as i32 + 2);
            let low = Roll::d20(&mut rng, 0, Edge::Disadvantage);
            assert!(low.die <= low.other.unwrap());
        }
        assert_eq!(Roll::d20(&mut rng, 0, Edge::Plain).other, None);
    }

    #[test]
    fn advantage_and_disadvantage_cancel() {
        assert_eq!(Edge::of(true, true), Edge::Plain);
        assert_eq!(Edge::of(true, false), Edge::Advantage);
        assert_eq!(Edge::of(false, true), Edge::Disadvantage);
        assert_eq!(Edge::of(false, false), Edge::Plain);
    }
}
