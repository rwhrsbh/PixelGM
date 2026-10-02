//! The rules of the game: D&D 5e as written in the System Reference Document 5.1. Dice,
//! characters, checks and combat are decided here, by code — the game master tells the
//! story but never holds the dice.

pub mod character;
pub mod combat;
pub mod data;
pub mod dice;

pub use character::{Ability, AbilityMethod, Character, DeathSaves};
pub use combat::{Combatant, Encounter, Event, Outcome, Side};
pub use data::{Srd, srd, tr};
pub use dice::{Dice, Rng, Roll};
