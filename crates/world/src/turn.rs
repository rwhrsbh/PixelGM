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
    /// Colours added for what this turn draws, by letters not in use yet.
    pub palette: std::collections::BTreeMap<String, String>,
    /// Kinds of ground the master draws for this turn: a bridge, ice, rubble.
    pub tiles: Vec<crate::build::TileDraft>,
    pub options: Vec<ActionDraft>,
    /// What only one character learns.
    pub whispers: Vec<Whisper>,
    /// The master's own count of what its narration says the heroes got and lost, by name:
    /// the game holds the effects against it.
    pub gained: Vec<String>,
    pub lost: Vec<String>,
    /// Its own count of who the narration says dies or is struck down, by id.
    pub slain: Vec<String>,
    /// Whether the narration has blows beginning between the party and someone who can
    /// fight back.
    pub fight: bool,
}

/// Every effect in an answer, however deep: the turn's own, those of its choices, and those
/// behind a check.
fn every_effect<'a>(value: &'a Value, found: &mut Vec<&'a Value>) {
    match value {
        Value::Array(list) => list.iter().for_each(|v| every_effect(v, found)),
        Value::Object(map) => {
            if map.contains_key("op") {
                found.push(value);
            }
            map.values().for_each(|v| every_effect(v, found));
        }
        _ => {}
    }
}

impl World {
    /// What is wrong with the master's answer that it could mend if asked again: a thing
    /// told of but not handed over, a thing with no picture, someone or some ground that is
    /// not there. Empty when the answer can be played as it is.
    pub fn faults(&self, draft: &TurnDraft) -> Vec<String> {
        let mut faults = Vec::new();
        if draft.narration.trim().is_empty() {
            faults.push("\"narration\" is empty".to_string());
        }
        let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string();
        let same = |a: &str, b: &str| {
            let (a, b) = (a.to_lowercase(), b.to_lowercase());
            !a.is_empty() && !b.is_empty() && (a.contains(&b) || b.contains(&a))
        };
        // What the telling itself hands over and takes away.
        let mut deep = Vec::new();
        draft.effects.iter().for_each(|e| every_effect(e, &mut deep));
        let given: Vec<String> = deep.iter().filter(|e| text(e, "op") == "give").map(|e| text(e, "name")).collect();
        let taken: Vec<String> = deep.iter().filter(|e| text(e, "op") == "take").map(|e| text(e, "name")).collect();
        for name in draft.gained.iter().filter(|n| !n.trim().is_empty()) {
            if !given.iter().any(|g| same(g, name)) {
                faults.push(format!("the narration says a hero gets \"{name}\" but no \"give\" effect hands it over"));
            }
        }
        for name in draft.lost.iter().filter(|n| !n.trim().is_empty()) {
            if !taken.iter().any(|t| same(t, name)) {
                faults.push(format!("the narration says a hero loses \"{name}\" but no \"take\" effect removes it"));
            }
        }
        // Who the telling strikes down must really fall; a fight told of must really begin.
        let fallen: Vec<String> = deep.iter().filter(|e| matches!(text(e, "op").as_str(), "kill" | "remove")).map(|e| slug(&text(e, "id"))).collect();
        for id in draft.slain.iter().map(|id| slug(id)).filter(|id| !id.is_empty()) {
            let hero = self.party.iter().any(|h| slug(&h.name) == id);
            if !hero && !fallen.contains(&id) {
                faults.push(format!("the narration says \"{id}\" dies or is struck down but no \"kill\" effect makes it so: it would go on standing there"));
            }
        }
        if draft.fight && !deep.iter().any(|e| text(e, "op") == "fight") {
            faults.push("the narration has a fight beginning but there is no \"fight\" effect: add \"attitude\" hostile for the enemy and \"fight\", and tell only how it begins — the game plays the blows".to_string());
        }
        // Everything named must be there, or be made in this same answer.
        let mut all = Vec::new();
        draft.effects.iter().for_each(|e| every_effect(e, &mut all));
        draft.options.iter().flat_map(|o| o.effects.iter()).for_each(|e| every_effect(e, &mut all));
        let here = self.here();
        let spawned: Vec<String> = all.iter().filter(|e| text(e, "op") == "spawn").map(|e| slug(&text(e, "id"))).collect();
        let heroes: Vec<String> = self.party.iter().map(|h| slug(&h.name)).collect();
        let known = |id: &str| {
            matches!(id, "" | "self" | "party" | "hero" | "heroes" | "player" | "players" | "everyone" | "all") || heroes.iter().any(|h| h == id) || spawned.iter().any(|s| s == id) || here.props.iter().any(|p| p.id == id) || here.creatures.iter().any(|c| c.id == id)
        };
        let drawn_tiles: Vec<String> = draft.tiles.iter().map(|t| slug(&t.id)).collect();
        let drawn_sprites: Vec<String> = draft.sprites.iter().map(|s| slug(&s.id)).collect();
        for effect in &all {
            let op = text(effect, "op");
            let id = slug(&text(effect, "id"));
            match op.as_str() {
                "remove" | "move" | "animate" | "attitude" | "join" | "kill" if !known(&id) => faults.push(format!("\"{op}\" names \"{id}\", which is not in this place: use an id from PLACE or WHAT IS WHERE, or \"spawn\" it first")),
                "tile" => {
                    let tile = slug(&text(effect, "tile"));
                    if !self.tiles.contains_key(&tile) && !drawn_tiles.contains(&tile) {
                        faults.push(format!("\"tile\" uses \"{tile}\", which is neither in TILES THAT EXIST nor drawn in this answer's \"tiles\""));
                    }
                }
                "spawn" => {
                    let sprite = slug(&text(effect, "sprite"));
                    if !sprite.is_empty() && !self.sprites.contains_key(&sprite) && !drawn_sprites.contains(&sprite) {
                        faults.push(format!("\"spawn\" of \"{id}\" uses the sprite \"{sprite}\", which is neither in SPRITES THAT EXIST nor drawn in this answer's \"sprites\""));
                    }
                }
                "give" if effect.get("rows").and_then(Value::as_array).is_none_or(|rows| rows.is_empty()) => faults.push(format!("\"give\" of \"{}\" has no \"rows\": every thing given needs its picture", text(effect, "name"))),
                _ => {}
            }
        }
        faults.sort();
        faults.dedup();
        faults.truncate(8);
        faults
    }
}

/// Things said aloud in a place, to be shown as speech bubbles.
#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct ChatterDraft {
    pub lines: Vec<Said>,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct Said {
    pub who: String,
    pub text: String,
}

/// Words for one player alone. `to` names the character, or the player.
#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct Whisper {
    pub to: String,
    pub text: String,
}

/// What the master wrote when asked for abilities of a character's own.
#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct FeatsDraft {
    pub feats: Vec<Value>,
}

impl FeatsDraft {
    /// The feats that could be read; one that cannot is dropped, not fatal.
    pub fn read(&self) -> Vec<rules::Feat> {
        self.feats.iter().filter_map(|value| serde_json::from_value::<rules::Feat>(lenient(value.clone())).ok()).filter(|feat| !feat.name.trim().is_empty()).collect()
    }
}

/// Models write numbers as strings and leave nulls where nothing applies.
fn lenient(mut value: Value) -> Value {
    if let Some(fields) = value.as_object_mut() {
        fields.retain(|_, v| !v.is_null());
        for key in ["range", "area", "uses", "ac", "hp", "speed"] {
            if let Some(number) = fields.get(key).and_then(|v| v.as_str()).and_then(|s| s.trim().parse::<i64>().ok()) {
                fields.insert(key.to_string(), Value::from(number));
            }
        }
        if let Some(save) = fields.get("save").and_then(|v| v.as_str()).map(|s| s.trim().to_lowercase()) {
            match rules::Ability::from_index(&save) {
                Some(ability) => fields.insert("save".to_string(), serde_json::to_value(ability).unwrap_or(Value::Null)),
                None => fields.remove("save"),
            };
        }
        if let Some(dice) = fields.get("dice").filter(|v| v.is_number()).map(|v| v.to_string()) {
            fields.insert("dice".to_string(), Value::from(dice));
        }
    }
    value
}

/// Translations, in the order the texts were sent.
#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct TranslationDraft {
    pub texts: Vec<String>,
}

/// A hero's picture, as the master drew it.
#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct PortraitDraft {
    pub rows: Vec<String>,
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

/// The id of the sprite a hero is drawn with, once the master has drawn one.
pub fn portrait_id(name: &str) -> String {
    format!("portrait_{}", slug(name))
}

impl World {
    /// Keeps the picture the master drew of a hero. `false` when it cannot be used.
    pub fn set_portrait(&mut self, hero: usize, rows: &[String]) -> bool {
        let Some(name) = self.party.get(hero).map(|h| h.name.clone()) else { return false };
        match sprite_from(rows, self.palette.len()) {
            Some(picture) => {
                self.sprites.insert(portrait_id(&name), picture);
                true
            }
            None => false,
        }
    }

    /// The sprite to draw a hero with: its own picture, or the stock figure.
    pub fn hero_sprite(&self, hero: Option<usize>) -> String {
        hero.and_then(|i| self.party.get(i)).map(|h| portrait_id(&h.name)).filter(|id| self.sprites.contains_key(id)).unwrap_or_else(|| crate::base::HERO_SPRITE.to_string())
    }

    /// Takes the master's answer to what the party did. `said` is the party's own words; an
    /// empty `said` means the master spoke unprompted.
    pub fn take_turn(&mut self, said: &str, draft: &TurnDraft) -> (Turn, Vec<String>) {
        let mut notes = Vec::new();
        self.add_colors(&draft.palette);
        for sprite in &draft.sprites {
            let id = slug(&sprite.id);
            match sprite_from(&sprite.rows, self.palette.len()) {
                Some(picture) if !id.is_empty() => {
                    self.sprites.insert(id, picture);
                }
                _ => notes.push(format!("sprite '{}' had no usable picture and was skipped", sprite.id)),
            }
        }
        for tile in &draft.tiles {
            let id = slug(&tile.id);
            match crate::build::tile_from(&tile.rows, self.palette.len()) {
                Some(sprite) if !id.is_empty() => {
                    let name = if tile.name.trim().is_empty() { id.clone() } else { tile.name.trim().to_string() };
                    self.tiles.insert(id, crate::model::TileDef { name, walkable: tile.walkable, sprite });
                }
                _ => notes.push(format!("tile '{}' had no usable picture and was skipped", tile.id)),
            }
        }
        let effects = read_effects(&draft.effects, &mut notes);
        self.turns += 1;
        // Threats whose time was up were put to the master with this telling.
        let now = self.clock;
        for threat in self.threats.iter_mut().filter(|t| now >= t.due) {
            threat.told = true;
        }
        self.story.say(Speaker::Player, said);
        self.story.say(Speaker::Master, &draft.narration);
        notes.extend(self.apply(&effects, None));
        let mut options = read_actions(&draft.options, &mut notes);
        // A choice that needs the party beside something is also kept on that thing, to be
        // found there when the party walks up; one naming nothing here needs nobody.
        let here = self.current.clone();
        if let Some(location) = self.locations.get_mut(&here) {
            for option in &mut options {
                let Some(id) = option.near.clone() else { continue };
                let actions = location.props.iter_mut().find(|p| p.id == id).map(|p| &mut p.actions).or_else(|| location.creatures.iter_mut().find(|c| c.id == id).map(|c| &mut c.actions));
                match actions {
                    Some(actions) => {
                        if !actions.iter().any(|a| a.label == option.label) {
                            actions.push(option.clone());
                            let extra = actions.len().saturating_sub(6);
                            actions.drain(..extra);
                        }
                    }
                    None => option.near = None,
                }
            }
        }
        (Turn { narration: draft.narration.trim().to_string(), options }, notes)
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
    fn an_answer_that_cannot_be_played_as_told_is_found_out() {
        let w = world();
        let bad: TurnDraft = serde_json::from_value(serde_json::json!({
            "narration": "You take a flask and an amulet.",
            "gained": ["Flask", "Amulet"],
            "effects": [
                { "op": "give", "name": "Flask of water", "rows": ["11", "11"] },
                { "op": "remove", "id": "pulse_core" },
                { "op": "tile", "x": 1, "y": 1, "tile": "ice" },
                { "op": "give", "name": "Rope" }
            ]
        }))
        .unwrap();
        let faults = w.faults(&bad).join(" | ");
        assert!(faults.contains("Amulet") && !faults.contains("\"Flask\""), "{faults}");
        assert!(faults.contains("pulse_core") && faults.contains("\"ice\"") && faults.contains("\"Rope\" has no"), "{faults}");
        let good: TurnDraft = serde_json::from_value(serde_json::json!({
            "narration": "The cat jumps on a slab of ice.",
            "tiles": [{ "id": "ice", "walkable": true, "rows": ["1"] }],
            "effects": [{ "op": "tile", "x": 1, "y": 1, "tile": "ice" }, { "op": "animate", "id": "cat", "motion": "jump" }, { "op": "animate", "id": "party", "motion": "nod" }]
        }))
        .unwrap();
        assert_eq!(w.faults(&good), Vec::<String>::new());
        // A death told of with nothing to make it so, and a fight told in words.
        let told: TurnDraft = serde_json::from_value(serde_json::json!({ "narration": "You cut the cat down and its mate leaps at you.", "slain": ["cat"], "fight": true, "effects": [] })).unwrap();
        let faults = w.faults(&told).join(" | ");
        assert!(faults.contains("\"cat\" dies") && faults.contains("no \"fight\" effect"), "{faults}");
        let done: TurnDraft = serde_json::from_value(serde_json::json!({ "narration": "You cut the cat down.", "slain": ["cat"], "effects": [{ "op": "kill", "id": "cat" }] })).unwrap();
        assert_eq!(w.faults(&done), Vec::<String>::new());
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
        assert_eq!(turn.options.len(), 4);

        let outcome = w.choose(&turn.options[0]);
        assert_eq!(outcome, turn.options[0].outcome);
        assert_eq!(w.story.log.len(), 4);
        assert_eq!(w.story.bags[0].inventory.len(), 1);
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


    #[test]
    fn feats_are_read_leniently_and_the_unreadable_are_dropped() {
        let draft: FeatsDraft = serde_json::from_value(serde_json::json!({ "feats": [
            { "name": "Ember hands", "description": "Fire.", "dice": "2d6", "range": "30", "save": "DEX", "uses": 2, "area": null },
            { "name": "", "dice": "1d4" },
            "nonsense",
            { "name": "Thick skin", "ac": 1, "dice": 0 }
        ] })).unwrap();
        let feats = draft.read();
        assert_eq!(feats.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Ember hands", "Thick skin"]);
        assert_eq!((feats[0].range, feats[0].save, feats[0].uses), (30, Some(rules::Ability::Dex), 2));
        assert_eq!(feats[1].ac, 1);
    }


    #[test]
    fn a_hero_is_drawn_with_its_own_picture_once_there_is_one() {
        let mut w = world();
        let skills = vec!["athletics".to_string(), "perception".to_string()];
        w.party.push(rules::Character::create("Brak the Bold", "", "human", "fighter", [15, 12, 14, 8, 10, 13], &skills, &[]).unwrap());
        assert_eq!(w.hero_sprite(Some(0)), crate::base::HERO_SPRITE);
        assert!(!w.set_portrait(0, &[]), "an empty picture is no picture");
        let portrait: PortraitDraft = serde_json::from_str(&fake::portrait()).unwrap();
        assert!(w.set_portrait(0, &portrait.rows));
        assert_eq!(w.hero_sprite(Some(0)), "portrait_brak_the_bold");
        assert_eq!(w.hero_sprite(Some(7)), crate::base::HERO_SPRITE);
        assert_eq!(w.hero_sprite(None), crate::base::HERO_SPRITE);
    }

}
