//! One exchange with the game master during play: the party says what it does, the master
//! tells what happens, changes the world, and offers what might be done next.

use crate::build::{SpriteDraft, slug, sprite_from};
use crate::model::World;
use crate::story::{Action, ActionDraft, Speaker, read_actions, read_effects};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct TurnDraft {
    pub narration: String,
    pub effects: Vec<Value>,
    pub sprites: Vec<SpriteDraft>,
    pub options: Vec<ActionDraft>,
    /// What only one character learns.
    pub whispers: Vec<Whisper>,
}

/// Words for one player alone. `to` names the character, or the player.
#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct Whisper {
    pub to: String,
    pub text: String,
}

/// A creature the game master keeps to use again: what it is, how it looks, how it fights.
/// A human master collects these in a library of their own and brings them into any world.
#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct CreatureCard {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Rows of palette characters, as every sprite is drawn.
    pub rows: Vec<String>,
    pub monster: Option<String>,
    pub stats: Option<crate::party::Stats>,
}

impl CreatureCard {
    /// The changes that put this creature into the place the party is in, at a cell.
    pub fn summon(&self, x: usize, y: usize, hostile: bool) -> TurnDraft {
        let id = slug(&self.id);
        let id = if id.is_empty() { slug(&self.name) } else { id };
        TurnDraft {
            sprites: vec![SpriteDraft { id: id.clone(), rows: self.rows.clone() }],
            effects: vec![serde_json::json!({
                "op": "spawn", "kind": "creature", "id": id, "name": self.name, "description": self.description,
                "x": x, "y": y, "sprite": id, "attitude": if hostile { "hostile" } else { "neutral" },
                "monster": self.monster, "stats": self.stats,
            })],
            ..TurnDraft::default()
        }
    }
}

/// What the players see after a turn: the telling, and choices whose outcomes are already
/// written, so picking one costs no request.
#[derive(serde::Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Turn {
    pub narration: String,
    pub options: Vec<Action>,
}

/// What the master wrote when asked to put the older story into a chapter.
#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct ChapterDraft {
    pub chapter: String,
    pub facts: Vec<String>,
}

impl World {
    /// Takes the master's answer to what the party did. `said` is the party's own words; an
    /// empty `said` means the master spoke unprompted.
    pub fn take_turn(&mut self, said: &str, draft: &TurnDraft) -> (Turn, Vec<String>) {
        let mut notes = Vec::new();
        for sprite in &draft.sprites {
            let id = slug(&sprite.id);
            match sprite_from(&sprite.rows, self.palette.len()) {
                Some(picture) if !id.is_empty() => {
                    self.sprites.insert(id, picture);
                }
                _ => notes.push(format!("sprite '{}' had no usable picture and was skipped", sprite.id)),
            }
        }
        let effects = read_effects(&draft.effects, &mut notes);
        self.story.say(Speaker::Player, said);
        self.story.say(Speaker::Master, &draft.narration);
        notes.extend(self.apply(&effects, None));
        (Turn { narration: draft.narration.trim().to_string(), options: read_actions(&draft.options, &mut notes) }, notes)
    }

    /// Carries out one of the choices the master offered with a turn.
    pub fn choose(&mut self, option: &Action) -> String {
        self.story.say(Speaker::Player, &option.label);
        self.story.say(Speaker::Master, &option.outcome);
        self.apply(&option.effects, None);
        option.outcome.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{WorldDraft, build_world};
    use crate::fake;
    use crate::story::Effect;

    fn world() -> World {
        let draft: WorldDraft = serde_json::from_str(&fake::world("turn")).unwrap();
        build_world(&draft, "turn", "English").unwrap().0
    }

    #[test]
    fn a_turn_is_recorded_applied_and_offers_choices() {
        let mut w = world();
        let draft: TurnDraft = serde_json::from_str(&fake::turn("I shout into the trees")).unwrap();
        let (turn, notes) = w.take_turn("I shout into the trees", &draft);
        assert!(notes.is_empty(), "{notes:?}");
        assert!(turn.narration.contains("I shout into the trees"));
        assert_eq!(w.story.log.len(), 2);
        assert_eq!(w.story.facts, ["The party made itself heard."]);
        assert_eq!(turn.options.len(), 2);

        let outcome = w.choose(&turn.options[0]);
        assert_eq!(outcome, turn.options[0].outcome);
        assert_eq!(w.story.log.len(), 4);
        assert_eq!(w.story.inventory.len(), 1);
    }

    #[test]
    fn the_master_speaking_unprompted_leaves_no_player_line() {
        let mut w = world();
        let draft = TurnDraft { narration: "A branch snaps somewhere close.".into(), ..TurnDraft::default() };
        w.take_turn("", &draft);
        assert_eq!(w.story.log.len(), 1);
        assert_eq!(w.story.log[0].speaker, Speaker::Master);
    }

    #[test]
    fn a_sprite_drawn_in_a_turn_can_be_used_by_a_spawn_in_the_same_turn() {
        let mut w = world();
        let draft: TurnDraft = serde_json::from_value(serde_json::json!({
            "narration": "A crow lands.",
            "sprites": [{ "id": "crow", "rows": ["0110", "1111", "0110"] }],
            "effects": [{ "op": "spawn", "kind": "creature", "id": "crow", "name": "Crow", "x": 4, "y": 4, "sprite": "crow" }]
        }))
        .unwrap();
        let (_, notes) = w.take_turn("I wait", &draft);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(w.here().creatures.iter().find(|c| c.id == "crow").unwrap().sprite, "crow");
    }

    #[test]
    fn a_broken_effect_in_a_turn_is_noted_and_the_telling_still_stands() {
        let mut w = world();
        let draft: TurnDraft = serde_json::from_value(serde_json::json!({ "narration": "Nothing much.", "effects": [{ "op": "teleport" }], "options": [{ "label": "", "outcome": "x" }] })).unwrap();
        let (turn, notes) = w.take_turn("I look around", &draft);
        assert_eq!(turn, Turn { narration: "Nothing much.".into(), options: vec![] });
        assert_eq!(notes.len(), 1);
        let _ = Effect::Fact { text: String::new() };
    }

    #[test]
    fn a_card_from_the_library_arrives_with_its_picture_and_its_numbers() {
        let mut w = world();
        let card = CreatureCard {
            id: "Bog Troll".into(),
            name: "Bog troll".into(),
            description: "Wet and patient.".into(),
            rows: vec!["0110".into(), "1111".into(), "1001".into()],
            monster: None,
            stats: Some(crate::party::Stats { ac: 13, hp: 30, speed: 30, attacks: Vec::new(), xp: 200 }),
        };
        let (_, notes) = w.take_turn("", &card.summon(6, 6, true));
        assert!(notes.is_empty(), "{notes:?}");
        let troll = w.here().creatures.iter().find(|c| c.id == "bog_troll").expect("the troll stands in the place");
        assert_eq!((troll.sprite.as_str(), troll.attitude, troll.stats.as_ref().map(|s| s.hp)), ("bog_troll", crate::Attitude::Hostile, Some(30)));
        assert!(w.sprites.contains_key("bog_troll"));
        assert!(w.story.log.is_empty(), "bringing a creature in says nothing by itself");
        // The same card twice gives two creatures.
        w.take_turn("", &card.summon(6, 6, false));
        assert!(w.here().creatures.iter().any(|c| c.id == "bog_troll_2"));
    }

    #[test]
    fn a_draft_survives_the_wire_with_its_whispers() {
        let draft: TurnDraft = serde_json::from_value(serde_json::json!({ "narration": "Rain.", "whispers": [{ "to": "Brak", "text": "You smell smoke." }] })).unwrap();
        let back: TurnDraft = serde_json::from_str(&serde_json::to_string(&draft).unwrap()).unwrap();
        assert_eq!(back, draft);
        assert_eq!(back.whispers[0], Whisper { to: "Brak".into(), text: "You smell smoke.".into() });
    }

}
