//! The game world as data: what a location is, how the game master's drawing of one is read
//! and made playable, and how the whole thing is saved. No engine code lives here, so every
//! rule can be tested without opening a window.

pub mod base;
pub mod build;
pub mod fake;
pub mod model;
pub mod party;
pub mod path;
pub mod prompt;
pub mod story;
pub mod turn;
pub mod walk;

pub use build::{LocationDraft, WorldDraft, build_location, build_world, extract_json};
pub use model::{Attitude, Creature, Exit, Location, MAP_H, MAP_W, Palette, Player, Prop, Sprite, Stub, TILE, TileDef, World};
pub use party::{Checked, FightSummary, PendingCheck, Stats};
pub use story::{Action, Effect, Item, Line, Speaker, Story};
pub use turn::{ChapterDraft, Turn, TurnDraft};
