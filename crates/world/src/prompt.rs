//! The words sent to the game master when it is asked to draw: the rules of the drawing
//! format, and the two requests built on them — a new world, and one more place in a world.

use crate::base::SLOTS;
use crate::model::{MAP_H, MAP_W, World, char_of};
use std::fmt::Write;

/// The rules that never change. The answer format is plain JSON so that every model in the
/// fallback chain can produce it.
pub fn system_prompt(language: &str) -> String {
    let mut slots = String::new();
    for (i, (job, _)) in SLOTS.iter().enumerate() {
        let _ = writeln!(slots, "  {} — {job}", char_of(i as u8 + 1));
    }
    format!(
        r##"You are the game master of a top-down pixel-art tabletop role-playing game for adults. You invent the world and you draw it. Dark themes are welcome.

You answer with ONE JSON object and nothing else: no prose, no code fence.

LANGUAGE. Every name, label and description is written in {language}. Every "id" is short English snake_case.

PICTURES. A picture is a list of rows; a row is a string; each character is one pixel holding a palette index: 0 is transparent, then 1-9, then a-v (a=10 … v=31).
The first fifteen colours have fixed jobs, so that everything in a world matches:
{slots}You choose the actual colours for these jobs (purple plants, red sand — whatever the world needs) and may add colours g-v for anything else.
- A tile is 16 rows of 16 characters and covers its whole cell: no 0.
- A sprite is 16x16 (one cell) or 32x32 (a large creature or object). Draw it like a classic 16-bit RPG: a one-pixel outline in colour 1, background 0, the feet on the bottom row, light from the top left. Make it recognisable at a glance.

MAPS. A map is {h} rows of exactly {w} characters. "legend" says which tile each character stands for. x counts columns 0-{mx} from the left, y counts rows 0-{my} from the top.
Built-in tiles you can use without drawing them:
  walkable: grass, dirt, sand, stone_floor, wood_floor, door
  solid: water, stone_wall, wood_wall, tree, bush, rock
Draw "new_tiles" when a place needs something these do not cover. A place should look like somewhere particular: buildings with walls and doors, paths, water, clearings — not an empty field.

EXITS. An exit is a walkable cell, usually on the edge of the map or in a doorway, that leads to another place named by "to". Every place an exit leads to is listed in "neighbors" unless it already exists. Give a place two to four exits. Everything must be reachable on foot from "spawn".

PROPS are things worth a look (a well, a wreck, a shrine); CREATURES are beings (people, animals, monsters). Each names a sprite by id; draw the sprite in "sprites" unless one with that id already exists. "attitude" is friendly, neutral or hostile."##,
        w = MAP_W,
        h = MAP_H,
        mx = MAP_W - 1,
        my = MAP_H - 1,
    )
}

/// The shape of a location in an answer, shown by example.
const LOCATION_SHAPE: &str = r##"{
  "id": "snake_case", "name": "…", "description": "two or three sentences the players read on arrival",
  "legend": {".": "grass", "#": "stone_wall", "+": "door"},
  "map": ["30 characters", "… 16 rows in all"],
  "spawn": [x, y],
  "new_tiles": [{"id": "…", "name": "…", "walkable": true, "rows": ["16 characters", "… 16 rows"]}],
  "sprites": [{"id": "…", "rows": ["16 or 32 characters", "… as many rows"]}],
  "props": [{"id": "…", "name": "…", "description": "…", "x": 0, "y": 0, "blocking": true, "sprite": "…"}],
  "creatures": [{"id": "…", "name": "…", "description": "…", "x": 0, "y": 0, "sprite": "…", "attitude": "neutral"}],
  "exits": [{"x": 0, "y": 0, "to": "neighbor_id", "label": "what the way out looks like"}],
  "neighbors": [{"id": "neighbor_id", "name": "…", "description": "one sentence: what is there"}]
}"##;

/// The first request of a game. `request` is what the host typed; it may be empty.
pub fn new_world(request: &str) -> String {
    let wish = if request.trim().is_empty() {
        "The host left the choice to you. Pick a setting nobody would expect and make it vivid.".to_string()
    } else {
        format!("The host asked for this world: «{}»", request.trim())
    };
    format!(
        r##"Create a new world and draw the place where the adventure begins.

{wish}

Answer with this JSON object:
{{
  "title": "the name of the world or campaign",
  "summary": "three or four sentences: the setting, what is going on, what makes it different",
  "tone": "a few words",
  "palette": {{"1": "#rrggbb", "…": "a colour for each of 1-f, plus any of g-v you add"}},
  "tiles": [tiles of this world that every place may use, same shape as new_tiles],
  "location": {LOCATION_SHAPE}
}}"##
    )
}

/// A request to draw a place that so far is only a name. `from` is where the party walks in
/// from.
pub fn new_location(world: &World, id: &str, from: &str) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "WORLD: {}. {}\nTone: {}.", world.title, world.summary, world.tone);
    let _ = write!(text, "\nPALETTE:");
    for (i, [r, g, b]) in world.palette.colors.iter().enumerate() {
        let _ = write!(text, " {}=#{r:02x}{g:02x}{b:02x}", char_of(i as u8 + 1));
    }
    let _ = write!(text, "\n\nTILES THAT EXIST (walkable marked +):");
    for (tile_id, tile) in &world.tiles {
        let _ = write!(text, " {tile_id}{}", if tile.walkable { "+" } else { "" });
    }
    let _ = write!(text, "\nSPRITES THAT EXIST:");
    for sprite_id in world.sprites.keys() {
        let _ = write!(text, " {sprite_id}");
    }
    let _ = writeln!(text, "\n\nPLACES SO FAR:");
    for (place_id, place) in &world.locations {
        let _ = writeln!(text, "- {place_id}: {} — {}", place.name, place.description);
    }
    for (place_id, stub) in &world.stubs {
        let _ = writeln!(text, "- {place_id} (not drawn yet): {} — {}", stub.name, stub.description);
    }
    let (name, description) = world.stubs.get(id).map(|s| (s.name.as_str(), s.description.as_str())).unwrap_or((id, ""));
    let _ = write!(
        text,
        "\nDraw the place \"{id}\": {name}. {description}\nThe party walks in from \"{from}\" ({}): one exit must lead back to \"{from}\", on the side that makes sense. Other exits may lead to places that exist or to new ones.\n\nAnswer with this JSON object (\"id\" must be \"{id}\"):\n{LOCATION_SHAPE}",
        world.place_name(from)
    );
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{WorldDraft, build_world};
    use crate::fake;

    #[test]
    fn system_prompt_states_the_language_the_sizes_and_every_colour_job() {
        let p = system_prompt("Russian");
        assert!(p.contains("written in Russian"));
        assert!(p.contains("16 rows of exactly 30 characters"));
        assert!(p.contains("x counts columns 0-29") && p.contains("y counts rows 0-15"));
        for (job, _) in SLOTS {
            assert!(p.contains(job), "{job}");
        }
    }

    #[test]
    fn world_request_carries_the_hosts_wish_or_says_the_choice_is_free() {
        assert!(new_world("динозавры и лазеры").contains("«динозавры и лазеры»"));
        assert!(new_world("  ").contains("left the choice to you"));
    }

    #[test]
    fn location_request_tells_the_master_what_exists_and_where_the_party_comes_from() {
        let draft: WorldDraft = serde_json::from_str(&fake::world("test")).unwrap();
        let (world, _) = build_world(&draft, "test", "English").unwrap();
        let stub = world.stubs.keys().next().unwrap().clone();
        let text = new_location(&world, &stub, &world.current);
        assert!(text.contains(&format!("Draw the place \"{stub}\"")));
        assert!(text.contains(&format!("lead back to \"{}\"", world.current)));
        assert!(text.contains("grass+") && text.contains(" water"));
        assert!(text.contains(" hero") && text.contains("1=#"));
        assert!(text.contains(&world.title));
    }
}
