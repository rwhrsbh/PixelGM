//! The words sent to the game master: the rules of the drawing format with the requests
//! built on them (a new world, one more place), and the rules of telling the story with
//! theirs (what the party does, an unprompted event, writing up a chapter).

use crate::base::SLOTS;
use crate::model::{Attitude, MAP_H, MAP_W, World, char_of};
use crate::story::{KEPT_LINES, Speaker};
use std::fmt::Write;

/// How the master says what a creature fights with.
const FIGHTERS: &str = r##"A creature that might fight needs numbers. Give "monster" the index of the D&D 5e SRD monster it is or most resembles (goblin, wolf, bandit, skeleton, giant-spider, ogre, guard, …), or give "stats" of its own: {"ac": 13, "hp": 11, "speed": 30, "attacks": [{"name": "…", "bonus": 4, "damage": "1d6+2", "range": 5}], "xp": 50}. Someone who sells things — a merchant, a smith, an innkeeper, a peddler — gets "shop": a list of 6 to 15 wares, each a thing as in "give" with its "rows" picture drawn at its "size" (cells across and down in the bag: a potion [1,1], a dagger [1,2], a sword [1,3], a bow or armour [2,3] — the shop shows every ware at that size on a grid, sorted onto shelves for weapons, armour, supplies and the rest), its "kind" and numbers ("heal", "damage", "gear", "bonus", "rarity") and a "price" in coins (a ration 2, a torch 2, a healing potion 50, a common weapon 10 to 50, armour 30 to 400, a rare thing hundreds); the game shows the shop, takes the coins and restocks it daily — never sell through narration or "give". A settled place has at least one shop. Every place has a "danger" from 0 (a town, a camp, a home: nothing ever comes) to 5 (the lair itself). Where it is above 0, the game rolls each hour for something to wander in — one of your "wanderers", or one of your "encounters" — so give both. Draw every wanderer now: a picture of its own in this answer's "sprites" (or one of SPRITES THAT EXIST that truly shows it), named in its "sprite". One you leave undrawn does not appear as a blank: when its hour comes you are asked again to draw it and tell of its coming. Where you set hostile creatures yourself, or give "danger": 0, it rolls nothing. A creature that belongs to a faction takes "faction": its id. Hostile creatures come in packs: put one creature down for the pack and the game brings as many of its kind as make a fair fight for the party there is; give "count" (1 to 8) to set the number yourself — "count": 1 for one that stands alone, a boss or a lone beast. Match the danger to a first-level party unless told otherwise: a first-level hero has about ten hit points, so against one or two heroes a single hit must not average more than five (challenge 1/4 or less: giant-rat, kobold, bandit, giant-centipede — not swarm-of-insects, whose bite takes ten), and nothing hostile stands right where the party walks in."##;

/// How the master changes the world. Shared by drawing (prepared actions) and by play.
const EFFECTS: &str = r##"EFFECTS change the world. Each is an object with "op":
  {"op": "remove", "id": "…"} — a prop or creature leaves the scene
  {"op": "give", "name": "…", "description": "…", "kind": "weapon|armor|light|potion|food|throwable|key|tool|junk|quest", "size": [1, 2], "heal": "2d4+2", "damage": "1d6", "light": 4, "rows": ["…"]} — the party gains an item. "rows" is REQUIRED on every "give" and on every thing in a container's "contents": always draw the thing yourself — 16 rows of 16 palette characters for each cell of its size, '0' transparent, the thing filling most of the picture, recognisable at a glance — the player sees it in the bag and on the ground, and a thing without a picture is a mistake. The rest besides name is optional: "size" is cells across and down in the bag (1 to 4), "heal" what drinking or eating it heals, "damage" what throwing it does, "light" how many cells it lights when held, "rows" your picture of it (required), 16 pixels to a cell; a weapon, armour or shield the hero can wield or wear ALWAYS takes "kind": "weapon" or "armor" and "gear": the D&D 5e SRD index it works as (longsword, dagger, greataxe, shortbow, leather-armor, chain-mail, breastplate, plate-armor, shield, …) — its own name and picture stay, the rules come from that index; "bonus": 1 to 3 makes it that much better than the book's (to hit and to damage, or to armour) — the game holds it to what the party's level allows; "rarity": "uncommon|rare|legendary" for a thing out of the ordinary; "price": what it is worth in coins
  {"op": "take", "name": "…"} — the party loses an item (used up, given away, broken)
  {"op": "hand", "name": "…", "to": "the other hero's name"} — the acting hero hands a thing from their own bag to another hero standing beside them; it only changes bags, nothing is built or used with it
  {"op": "describe", "id": "…", "description": "…"} — a thing looks different from now on
  {"op": "light", "light": "day|dusk|night|dark"} — "day", "dusk" or "night" puts the world's clock forward to that hour; "dark" makes this one place lightless for good (a cave, a cellar)
  {"op": "weather", "weather": "clear|sun|rain|storm|snow|fog"} — the sky over this place changes
  {"op": "mark", "x": 0, "y": 0, "kind": "blood|dung|puddle|scorch|ash|slime|vomit|wine|paint|rubble|coins|bones", "color": "#rrggbb"} — something is left on the ground of a cell; without x and y it is where the party stands; "id" instead puts it under that thing; "color" is optional
  {"op": "music", "mood": "calm|explore|tense|battle|boss|mystery|horror|sad|tavern|triumph|dungeon"} — the music of this place changes with what happens: dread creeping in, a celebration, grief
  {"op": "clean", "x": 0, "y": 0} — what was left on a cell is picked up or washed away (with "give" when the party takes it)
  {"op": "quest", "goal": "what the players must do next", "advance": true, "done": "won|lost"} — the main quest moves: "goal" changes what the players are told, "advance" goes to the next act, "done" ends it
  {"op": "quest_new", "title": "…", "goal": "…", "acts": ["…"], "ending": "…", "failure": "…"} — a new main quest, when there is none or the last one is over
  {"op": "task", "name": "…", "step": "what to do next", "done": false} — a side task someone gave the party: new, moved on, or done
  {"op": "faction", "id": "snake_case", "name": "…", "description": "…", "standing": 0} — a people, an order or a gang whose regard for the party is kept: new, or what is known of it changes
  {"op": "standing", "id": "faction id", "change": -1} — a faction thinks better (+) or worse (-) of the party for what it has just done; from -3, sworn enemies, to 3, allies
  {"op": "thread", "text": "what was done and who will come back about it"} — a note to yourself: a deed that must have consequences later
  {"op": "thread_done", "text": "a few words of the thread"} — that thread has come back and is done with
  {"op": "threat", "name": "…", "what": "what happens when the time is up", "rumour": "what the players hear of it, no hour named", "hours": 48} — something ripens while the party is busy; the same name again moves its hour
  {"op": "threat_end", "name": "…"} — a threat is averted, or has happened and is done with
  {"op": "join", "id": "creature id"} — someone here joins the party as a companion: they walk with the hero and fight under the player's hand
  {"op": "dismiss", "id": "companion id"} — a companion leaves and stays here
  {"op": "time", "hours": 2} — time passes: a long talk, a search, a wait, a march (the clock also runs by itself, a minute a second)
  {"op": "gold", "amount": 25, "to": "hero name"} — coins: a hero is paid, finds or is given them; a negative amount is what they pay or lose
  {"op": "shake", "seconds": 1.5, "who": ["hero name"], "x": 0, "y": 0, "radius": 4} — the screen shakes for those who feel it: an earthquake, an explosion, a roar, a collapse, a giant's step, a heavy blow; "who" names the heroes, or a cell and a "radius" in cells limits it to those that near; with neither, everyone in this place
  {"op": "travel", "to": "place_id", "name": "for a new place", "description": "what a new place looks like", "who": ["hero name"]} — heroes go to another place; "who" lists those who go, everyone here when left out
  {"op": "animate", "id": "hero name|creature id|party", "motion": "jump|crouch|sit|lie|fall|shake|spin|strike|dance|wave|nod|bow|cower|stagger"} — someone moves their body
  {"op": "edit", "id": "…", "name": "…", "size": 2, "ac": 15, "hp": 40, "speed": 30, "xp": 200, "monster": "srd index", "blocking": true, "sprite": "…"} — a creature or thing here changes: only what is given (a beast grows, a guard is badly hurt for good, a door stops blocking)
  {"op": "action", "id": "…", "label": "what the players choose", "outcome": "what happens", "effects": [], "needs": "a thing's name", "remove": false} — something new can be done with a creature or thing here (a lever appears to pull, a statue to question); "remove": true takes it away
  {"op": "stash", "id": "chest, crate, body or merchant id", "name": "…", …as in "give"} — a thing goes into a container here (found when it is opened) or onto a merchant's shelf
  {"op": "rest", "kind": "short|long"} — the party rests: a short rest spends hit dice, a long one is a night's sleep that heals everything
  {"op": "peace"} — the fight here stops without a winner: a parley, a surrender, a truce; those who fought are no longer hostile
  {"op": "drop", "x": 0, "y": 0, "name": "…", …as in "give"} — a thing is left lying on a cell, to be picked up
  {"op": "say", "id": "creature id", "text": "…"} — someone says a line aloud, in a bubble over them
  Every sight — "shake", "animate", "say" — takes "who": ["hero name", …] for only those heroes to see or feel it, or "x", "y" and "radius" (cells) for only those that near; with neither, everyone in this place. A whisper in a crowd, a tremor felt only in the cellar, a wink to one hero.
  {"op": "lines", "id": "creature id", "lines": ["…", "…"], "every": 20, "order": "in_turn|random"} — what someone keeps saying by themselves every so many seconds (a crier, a guard, a parrot); "lines": [] makes them quiet
  {"op": "wait", "seconds": 180} — everything after it in the list happens later: seconds of play (a clock minute passes each second), or "minutes"/"hours" of the clock; a fuse, a slow flood, a second blast
  {"op": "set", "var": "lever_down", "value": 1} / {"op": "add", "var": "blasts", "value": 1} — the world remembers a flag or a count
  {"op": "if", "var": "blasts", "at_least": 3, "then": [], "else": []} — one way or the other by a variable ("is" for an exact value)
  {"op": "random", "choices": [[…], […]]} — one of the lists, at random
  {"op": "repeat", "times": 3, "effects": [{"op": "shake", "seconds": 1}, {"op": "wait", "seconds": 10}]} — again and again; with a wait inside, a clock that ticks
  {"op": "define", "id": "collapse", "on": "enter|death|hour|var", "place": "place id", "of": "creature id", "at": 18, "var": "…", "effects": []} — a script the world keeps: run it with {"op": "script", "id": "collapse"} from an action or a plate, or let it go off by itself when the party walks into "place", when "of" dies, at the hour "at", or when "var" reaches "at"
  {"op": "script", "id": "collapse"} — a script runs now
  {"op": "attitude", "id": "…", "attitude": "friendly|neutral|hostile"}
  {"op": "move", "id": "…", "x": 0, "y": 0}
  {"op": "tile", "x": 0, "y": 0, "tile": "tile_id", "to": [0, 0]} — the ground changes, anywhere on the map: one cell, or with "to" every cell from (x, y) to that far corner — a line or a block (a door opens, a bridge is laid across water, a wall falls, a pit opens, a room floods). Whether the cell can then be walked on is the tile's own: a walkable tile over water makes a crossing, a solid one closes a way. Use a tile from TILES THAT EXIST or one you draw in this answer's "tiles"
  {"op": "spawn", "kind": "prop|creature", "id": "…", "name": "…", "description": "…", "x": 0, "y": 0, "sprite": "…", "blocking": false, "attitude": "neutral", "size": 1} — a thing spawned may carry "actions" as in a place you draw (a lever's pull, a lock that "needs" its key); a creature that might fight also takes "monster" or "stats"; a prop that holds things (a chest, a crate, a purse) takes "contents": [things as in "give"], and "lock": 15 with "key": "key name" when it is locked — the game handles opening, picking and forcing it
  {"op": "secret", "x": 0, "y": 0, "dc": 14, "kind": "hidden|trap|plate", "note": "a loose stone in the wall", "effects": []} — something hidden: when a hero with a sharp enough eye comes near it is found and its effects happen (a hidden door: a "tile" to a door; a cache: "give"); a trap goes off with its effects ("damage") when stepped on unnoticed
  {"op": "exit", "x": 0, "y": 0, "to": "place_id", "label": "…"} — a way out appears
  {"op": "fact", "text": "…"} — something that must be remembered for the rest of the campaign
  {"op": "check", "ability": "str|dex|con|int|wis|cha", "skill": "stealth", "dc": 15, "success": {"narration": "…", "effects": []}, "failure": {"narration": "…", "effects": []}} — the outcome is uncertain: the game rolls the die and takes one branch. Write both. Skills: acrobatics, animal-handling, arcana, athletics, deception, history, insight, intimidation, investigation, medicine, nature, perception, performance, persuasion, religion, sleight-of-hand, stealth, survival. Difficulty: 5 trivial, 10 easy, 15 moderate, 20 hard, 25 very hard.
  {"op": "damage", "amount": "2d6"} / {"op": "heal", "amount": "1d8", "id": "hero name or empty"} — to the party, outside a fight
  {"op": "damage", "id": "creature id", "amount": "1d8"} — a creature here is wounded outside a fight: it goes into the next fight that much weaker, and dies if it has no more to lose
  {"op": "kill", "id": "creature id"} — a creature here dies or is struck down without a fight; it lies there as a body that can be searched
  {"op": "xp", "amount": 50} — for something achieved; rarely, and modestly
  {"op": "fight", "with": ["creature id"]} — a fight begins with those named (they turn hostile) and everyone already hostile here; the game runs it by the rules, blow by blow
  {"op": "deed", "hero": "name"} — that character just did something out of the ordinary, clever or brave beyond what the moment asked; the game keeps count
  {"op": "revive", "hero": "name or a body's id"} — the story brings a dead character back; only when it has truly earned it
Never roll dice yourself and never decide an uncertain outcome: that is what "check" is for. Nothing may follow a check in the same list except what should happen whichever way it goes.
Use only ids that exist. Inside an action that belongs to a thing, the id "self" means that thing.
SPACE. The party is somewhere on the map and can only touch, take, open, strike or speak softly with what is beside it. To deal with something farther off that can be reached, first "move" the party to a cell beside it, in the same answer, then do the rest. What cannot be reached on foot cannot be touched: say what is in the way. Only what the party carries can be used — no torch, rope or key that is not in their hands. Each hero uses only what is in their OWN bag: a thing another player's hero carries stays theirs until that player drops it on the ground and it is picked up — never "take" it from them or let someone else use it. When another hero needs it, offer its holder a choice to hand it over: "for": the holder, "near": the hero who gets it (so the holder walks up first), effects [{"op": "hand", …}] and nothing more — what is then done with it is the other hero's own deed.
VIOLENCE. There are two cases, and the map must show each.
(1) A FIGHT — the party attacks someone who can fight back, or is attacked. You NEVER tell its blows or how it ends: tell only how it begins (two or three sentences), set "fight": true, make the enemy hostile with "attitude" and add the "fight" effect. The game then plays every blow by the rules and the dice decide. A fight told in words, with no "fight" effect, is a mistake.
(2) NO FIGHT — the victim cannot fight back: helpless, asleep, bound, taken wholly unawares, executed, crushed by a trap, or the story kills them. Then you tell it, and you MUST make it so: {"op": "kill", "id": …} for each who dies (list them in "slain"), {"op": "damage", "id": …, "amount": …} for a wound that does not kill. Whoever your narration says is dead, down or gone must not be left standing on the map: "kill" leaves a body, "remove" takes someone away alive.
Harm to the party outside a fight is "damage" with no id.
TRAVEL. When the party goes to another place — through a way out, down a hole, carried off — add {"op": "travel", "to": "place_id"}: an id from the exits, or a new id with "name" for a place that does not exist yet, which the game will ask you to draw. A choice that leads elsewhere carries its own "travel". The map on the players' screen IS where they are. When the story takes them out of what the map shows — out into space, into a vision or a dream, under the ground, onto a ship — or the place itself becomes another (it burns down, freezes over, collapses), "travel" to a NEW id with "name" and "description", so that the new scene is drawn; never tell of surroundings the map does not show.
MECHANISMS work by the game's own rules, with no question to you, so build them whole when you draw a place or spawn a thing. A lever, a winch, a valve: a prop with an action whose effects change the map ("tile" opens a gate or lowers a bridge, "remove" lifts a grate, "spawn" lets something out). A locked door or chest of another kind: a blocking prop with an action that has "needs": "the key's name" (and "spends": true when the key breaks or stays in the lock) and effects that remove it or turn its cell into a door; put the key somewhere it can be found. A pressure plate, a tripwire, a rune on the floor: {"op": "secret", "kind": "plate", …} at its cell, whose effects happen when someone steps there. A hidden door or cache: a "secret" of kind "hidden". A trap: a "secret" of kind "trap". Give a place with a riddle at least one way through that needs no dice.
A WORLD THAT ANSWERS. Nothing the party does is forgotten. When it helps or wrongs a faction or one of its people, change the "standing" in the same answer, and let those people treat the party accordingly from then on (enemies attack or refuse, allies help and sell cheap). When it does something that must come back — spares or kills someone who matters, steals, breaks an oath, leaves a job half done — note a "thread". When you are told a thread is due, bring it back in this very telling as something that happens to the party now, then "thread_done". Keep one or two "threat"s alive: enemies with plans of their own that move forward by the clock whether the party looks or not; let the players hear of them as rumours. When you are told a threat has come due, it HAS happened: tell what the party finds or hears, change the world with effects, and if it was the quest's own, end the quest "lost".
COMPANIONS. Anyone the party meets may be won over to travel with it — talked round (ask for a Charisma check when it is not sure), hired for gold (take it with "gold"), or given as the reward of a task. When they agree, add {"op": "join", "id": …} in the same answer; the game then walks them beside the hero and puts them in fights. They need "monster" or "stats" to fight with. When they leave or are sent away, "dismiss". A companion has a will of its own: it speaks up, and it can refuse.
THE MAIN QUEST. The acts are yours alone: never tell them to the players, lead the story through them. Every telling leaves a pull towards the goal — a clue, a rumour, someone who needs it done, an enemy who moves first — but the party is free to wander and you never refuse them. When what the act (NOW) calls for has happened, add {"op": "quest", "advance": true, "goal": "what they must do next"} in the same answer. When the last act is resolved, tell the ending and add {"op": "quest", "done": "won"} or "lost" — a quest can be lost, and a lost one is told as honestly as a won one. When someone asks the party for something on the side, add a "task"; when it is done, the same task with "done": true and its reward.
CHOICES. Offer up to five. At least one must rest on what this very hero is or carries — a skill they are trained in, their class, a thing in their bag — and say so in its label ("as a fighter: …", "with the rope: …"). At least one must be a risk: give it a "check" effect and put the ability and the difficulty in the label ("(Dexterity, DC 14)"). Where it fits, one costs something — a thing, gold, hit points — for a sure gain. Never offer only plain, safe things.
WHERE THE PARTY IS. "This wall", "the door", "in front of me", "here" in a player's words mean the cell IN FRONT of the hero, or one of those AROUND the party: change exactly those cells, by the coordinates given there, never some other spot nearby. Any cell can be changed with "tile", walls and the solid edge of the map included: a wall broken through becomes a walkable tile at that very cell. The party stands exactly on the cell marked @ and has not moved since, whatever was planned or told before: read the map before you write. It is on the far side of water, a wall or a chasm only if @ is there. To put it somewhere else in this place — across a bridge, up a ledge, through a gap — add {"op": "move", "id": "party", "x": …, "y": …} to a walkable cell there, in the same answer; with no "move" the party is still where @ is, and your narration must say so. A crossing must be laid on the very cells that are in the way (the solid cells between the party and the far side, by their coordinates from the map), not under the party's feet.
THE PICTURE MUST MATCH THE STORY. The players watch a map, not only words. Everything your narration says that one could see must come with the effect that shows it, in the same answer: someone crouches, jumps, strikes, falls or dances — "animate"; the ground trembles, something explodes, crashes down or roars — "shake"; a creature comes closer, backs off, circles — "move" it to its new cell (several cells if it runs); it leaves — "remove"; something appears — "spawn"; blood, dung, ash, a spill or a scorch is left — "mark"; a door opens or a wall falls — "tile"; creatures attack — "attitude" hostile and "fight". Never narrate a change on the map without its effect. Every "id" in an effect must be one listed in PLACE, WHAT IS WHERE or THE PARTY (or a thing you spawn in the same answer); an id that is not there does nothing — to bring someone new in, "spawn" them first. The same goes for the bag: every thing a narration or an outcome says a hero gets, finds, takes or is handed needs a "give" of its own in the same effects (two things named — two "give"), and every thing lost or spent a "take"; never name a thing that no effect hands over. A chest, crate or purse holds its things in "contents", not in an action of its own."##;

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
Draw "new_tiles" when a place needs something these do not cover. A place should look like somewhere particular: buildings with walls and doors, paths, water, clearings — not an empty field. Every tile stands where the thing it shows would really be: trees, bushes, growths and open water belong outdoors — inside a hut, a cellar or a cave use walls, floors, furniture and props, never a tree; and a tile drawn for one place is reused in another only when that thing would be found there too.

EXITS. An exit is a walkable cell, usually on the edge of the map or in a doorway, that leads to another place named by "to". Every place an exit leads to is listed in "neighbors" unless it already exists. Give a place two to four exits, and at least one of them must lead somewhere new — a neighbor that is not yet in the world — so that the world keeps opening up rather than closing into a loop. Everything must be reachable on foot from "spawn".

PROPS are things worth a look (a well, a wreck, a shrine); CREATURES are beings (people, animals, monsters). Each names a sprite by id; draw the sprite in "sprites" unless one with that id already exists. "attitude" is friendly, neutral or hostile.
{FIGHTERS}

ACTIONS. Give every prop and creature two or three "actions": things the players are likely to do with it. The players pick one from a list and read its outcome at once, without asking you, so write the outcome now: one to three sentences, told to the players as their game master ("you"), with the effects it has. Make them matter: a clue, an item, a changed attitude, a new way out. A creature that can talk has an action in which it says something worth hearing.

{EFFECTS}"##,
        w = MAP_W,
        h = MAP_H,
        mx = MAP_W - 1,
        my = MAP_H - 1,
    )
}

/// The shape of a location in an answer, shown by example.
const LOCATION_SHAPE: &str = r##"{
  "id": "snake_case", "name": "…", "description": "two or three sentences the players read on arrival",
  "palette": {"g": "#rrggbb"},
  "danger": 0,
  "wanderers": [{"monster": "srd index it fights as", "name": "what it is called in this world", "sprite": "id of its picture, drawn in sprites", "description": "one sentence told when it comes"}],
  "encounters": ["3 or 4 things that may happen here that are not fights, two sentences each: a peddler passing, tracks, a wounded stranger, a find"],
  "light": "day, dusk, night or dark (a cave, a cellar): the hour and the light of the place",
  "weather": "clear, sun, rain, storm, snow or fog; clear indoors and underground",
  "mood": "the music of the place: calm, explore, tense, mystery, horror, sad, tavern, dungeon",
  "legend": {".": "grass", "#": "stone_wall", "+": "door"},
  "map": ["30 characters", "… 16 rows in all"],
  "spawn": [x, y],
  "new_tiles": [{"id": "…", "name": "…", "walkable": true, "rows": ["16 characters", "… 16 rows"]}],
  "sprites": [{"id": "…", "rows": ["16 or 32 characters", "… as many rows"]}],
  "props": [{"id": "…", "name": "…", "description": "…", "x": 0, "y": 0, "blocking": true, "sprite": "…", "actions": [{"label": "what the players choose", "outcome": "what happens", "effects": []}]}],
  "creatures": [{"id": "…", "name": "…", "description": "…", "x": 0, "y": 0, "sprite": "…", "attitude": "neutral", "faction": "", "monster": "srd-index or omit", "count": 0, "size": 1, "lines": [], "shop": [], "actions": [{"label": "…", "outcome": "…", "effects": []}]}],
  "exits": [{"x": 0, "y": 0, "to": "neighbor_id", "label": "what the way out looks like"}],
  "neighbors": [{"id": "neighbor_id", "name": "…", "description": "one sentence: what is there"}]
}"##;

/// The first request of a game. `request` is what the host typed; it may be empty.
const QUEST_SHAPE: &str = r##"{"title": "the name of the main quest", "goal": "one sentence the players read: what they must do first", "acts": ["3 to 5 turns of the story in order, each one sentence: what must happen for the story to move on — hidden from the players"], "ending": "how it ends if they succeed", "failure": "how it ends if they fail, and what makes them fail"}"##;

pub fn new_world(request: &str, story: bool) -> String {
    let quest = if story { format!("\n  \"quest\": {QUEST_SHAPE},\n  \"factions\": [{{\"id\": \"snake_case\", \"name\": \"…\", \"description\": \"who they are and what they want\", \"standing\": 0}}, \"… three to five that matter in this world\"],") } else { String::new() };
    let wish = if request.trim().is_empty() {
        "The host left the choice to you. Pick a setting nobody would expect and make it vivid.".to_string()
    } else {
        format!("The host asked for this world: «{}»", request.trim())
    };
    format!(
        r##"Create a new world and draw the place where the adventure begins.

{wish}

Answer with this JSON object:
{{{quest}
  "title": "the name of the world or campaign",
  "summary": "three or four sentences: the setting, what is going on, what makes it different",
  "tone": "a few words",
  "palette": {{"1": "#rrggbb", "…": "a colour for each of 1-f, plus any of g-v you add"}},
  "tiles": [tiles of this world that every place may use, same shape as new_tiles],
  "location": {LOCATION_SHAPE}
}}"##
    )
}

/// The rules of telling the story, sent with every request made during play.
pub fn play_prompt(world: &World) -> String {
    let violence = if world.violence { "Violence may be described as graphically as the scene calls for." } else { "Keep violence off the page: say that it happened, not how it looked." };
    let sex = if world.sex { "Sexual content between adults may be explicit when the players lead there." } else { "Keep sex off the page: fade to black." };
    format!(
        r##"You are the game master of a tabletop role-playing game for adults, played on a top-down pixel map. You tell the players what happens and you change the world to match.

You answer with ONE JSON object and nothing else: no prose, no code fence.

LANGUAGE. Everything the players read is written in {language}. Every "id" is short English snake_case.{others}

THE TABLE. All players and all characters are adults. {violence} {sex}

TELLING. Speak to the players as "you". Two to five sentences: concrete, sensory, no summary of what they already know. Never decide what the players' characters do, say or feel. The world reacts: people remember, things break, time passes, actions have consequences. What the party attempts can fail or cost something. Do not hand out items, allies or secrets for nothing. Keep to what is established in the facts, the chapters and the story so far.

{EFFECTS}

A picture, when you need a new sprite, is a list of rows of palette index characters (0 transparent, 1-9, a-v), 16x16 or 32x32, outlined in colour 1.{book}"##,
        language = world.language,
        others = other_languages(world),
        book = book_note(world),
    )
}

/// What the master is told of the other languages read at the table: the telling is
/// written in each of them too, so nobody waits for a translation.
fn other_languages(world: &World) -> String {
    let others: Vec<&str> = world.table_languages.iter().map(String::as_str).filter(|l| !l.is_empty() && *l != world.language).collect();
    if others.is_empty() {
        return String::new();
    }
    let shape: Vec<String> = others.iter().map(|l| format!("\"{l}\": \"…\"")).collect();
    format!(
        " Some players at this table read {}: in every answer that has a \"narration\", add \"told_in\": {{{}}}, the same narration written in each of them, as well told. Everything else stays in {}.",
        others.join(" and "),
        shape.join(", "),
        world.language
    )
}

const WAYS: [(&str, i32, i32); 4] = [("north", 0, -1), ("east", 1, 0), ("south", 0, 1), ("west", -1, 0)];

/// A cell of the place being looked at, as the master is told it: where, what ground,
/// whether solid, what is on it.
fn cell_words(world: &World, cx: i32, cy: i32) -> String {
    let here = world.here();
    let (here_w, here_h) = here.size();
    if !(0..here_w as i32).contains(&cx) || !(0..here_h as i32).contains(&cy) {
        return "the edge of the map".to_string();
    }
    let (ux, uy) = (cx as usize, cy as usize);
    let tile = here.tile_id(ux, uy).unwrap_or("nothing");
    let solid = if world.tiles.get(tile).is_some_and(|t| t.walkable) { "" } else { ", solid" };
    let thing = here.props.iter().find(|p| (p.x, p.y) == (ux, uy)).map(|p| p.id.as_str()).or_else(|| here.creatures.iter().find(|c| (c.x, c.y) == (ux, uy)).map(|c| c.id.as_str()));
    let exit = here.exit_at(ux, uy).map(|e| format!(", the way out to {}", e.to)).unwrap_or_default();
    format!("({cx}, {cy}) {tile}{solid}{}{exit}", thing.map(|t| format!(", with {t} on it")).unwrap_or_default())
}

/// Where one of several who act at once stands and what is in front of them, for a line
/// of their own: "this wall" in their words is that cell.
pub fn stance(world: &World, cell: (i32, i32), facing: (i32, i32)) -> String {
    let (x, y) = cell;
    match WAYS.iter().find(|(_, dx, dy)| (*dx, *dy) == facing) {
        Some((name, dx, dy)) => format!("stands at ({x}, {y}), faces {name}; IN FRONT: {}", cell_words(world, x + dx, y + dy)),
        None => format!("stands at ({x}, {y})"),
    }
}

fn attitude_word(attitude: Attitude) -> &'static str {
    match attitude {
        Attitude::Friendly => "friendly",
        Attitude::Neutral => "neutral",
        Attitude::Hostile => "hostile",
    }
}

/// Everything the master has to know to tell the next part: the world, what must be
/// remembered, the place and what is in it, and the story so far. The newest
/// `recent` lines of the story are given word for word.
fn situation(world: &World, recent: std::ops::RangeFrom<usize>) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "WORLD: {}. {}\nTone: {}.", world.title, world.summary, world.tone);
    if !world.story.facts.is_empty() {
        let _ = writeln!(text, "\nFACTS:");
        for fact in &world.story.facts {
            let _ = writeln!(text, "- {fact}");
        }
    }
    for (n, chapter) in world.story.chapters.iter().enumerate() {
        let _ = writeln!(text, "\nCHAPTER {}: {chapter}", n + 1);
    }
    // What only grows comes first and what changes every turn last: a provider that keeps a
    // cache of the prompt takes everything up to the first difference from the request
    // before, so the longer the part that stays the same, the less each turn costs.
    // The colours behind the letters: whatever is drawn in a turn — a sprite, a tile, a
    // thing given — is drawn with them.
    let _ = write!(text, "\nPALETTE (0 is transparent):");
    for (i, [r, g, b]) in world.palette.colors.iter().enumerate() {
        let _ = write!(text, " {}=#{r:02x}{g:02x}{b:02x}", char_of(i as u8 + 1));
    }
    let free: String = (world.palette.len()..crate::model::PALETTE_MAX).map(|i| char_of(i as u8)).collect();
    if !free.is_empty() {
        let _ = write!(text, "\nFREE PALETTE LETTERS (give each a colour in \"palette\" to draw with it): {free}");
    }
    let _ = write!(text, "\nSPRITES THAT EXIST:");
    for sprite_id in world.sprites.keys() {
        let _ = write!(text, " {sprite_id}");
    }
    let _ = write!(text, "\nTILES THAT EXIST:");
    for tile_id in world.tiles.keys() {
        let _ = write!(text, " {tile_id}");
    }
    let _ = writeln!(text);
    let lines = world.story.log.get(recent).unwrap_or(&[]);
    if !lines.is_empty() {
        let _ = writeln!(text, "\n\nTHE STORY SO FAR:");
        for line in lines {
            let who = if line.speaker == Speaker::Master { "GM" } else { "PARTY" };
            let _ = writeln!(text, "{who}: {}", line.text);
        }
    }
    let _ = writeln!(text, "\nWHAT IS TRUE NOW:");
    let here = world.here();
    if !world.table.sandbox {
        let quest = &world.quest;
        if !quest.is_set() {
            let _ = writeln!(text, "\nMAIN QUEST: there is none yet. Give this world one now with a \"quest_new\" effect, grown out of what has happened so far.");
        } else if let Some(won) = quest.done {
            let _ = writeln!(text, "\nMAIN QUEST \"{}\" is over: the party {}. Tell what the world is like after it, and open the next chapter with a \"quest_new\" effect that grows out of how this one ended.", quest.title, if won { "won" } else { "lost" });
        } else {
            let _ = writeln!(text, "\nMAIN QUEST \"{}\". The players are told: {}", quest.title, quest.goal);
            for (n, act) in quest.acts.iter().enumerate() {
                let _ = writeln!(text, "  act {}{}: {act}", n + 1, if n == quest.act { " (NOW)" } else if n < quest.act { " (done)" } else { "" });
            }
            let _ = writeln!(text, "  if they succeed: {}\n  if they fail: {}", quest.ending, quest.failure);
        }
        let open: Vec<String> = world.quest.tasks.iter().filter(|t| !t.done).map(|t| format!("{} ({})", t.name, t.step)).collect();
        if !open.is_empty() {
            let _ = writeln!(text, "SIDE TASKS open: {}", open.join("; "));
        }
    }
    if !world.factions.is_empty() {
        let _ = writeln!(text, "\nFACTIONS (standing -3 enemies … 3 allies):");
        for f in &world.factions {
            let _ = writeln!(text, "- {} \"{}\": {}{} — {}", f.id, f.name, f.standing, if f.met { "" } else { ", not met yet" }, f.description);
        }
    } else {
        let _ = writeln!(text, "\nFACTIONS: none yet. Name the two or three that matter here with \"faction\" effects.");
    }
    if !world.threads.is_empty() {
        let _ = writeln!(text, "THREADS (yours alone, never told to the players as a list):");
        for t in &world.threads {
            let _ = writeln!(text, "- {}", t.text);
        }
    }
    if let Some(thread) = world.thread_due() {
        let _ = writeln!(text, "A THREAD IS DUE NOW — bring this back in this telling, as something that happens to the party, then \"thread_done\": {}", thread.text);
    }
    let (day, hour, minute) = world.time();
    for threat in &world.threats {
        let left = threat.due.saturating_sub(world.clock);
        if left == 0 {
            let _ = writeln!(text, "A THREAT HAS COME DUE — \"{}\": {}. It has happened. Tell what the party sees or hears of it now, change the world, then \"threat_end\".", threat.name, threat.what);
        } else {
            let _ = writeln!(text, "THREAT \"{}\" (hidden; the players have heard: {}): {} — in {} h {} min.", threat.name, threat.rumour, threat.what, left / 60, left % 60);
        }
    }
    if world.threats.is_empty() && !world.table.sandbox {
        let _ = writeln!(text, "THREATS: none is alive. Start one with a \"threat\" effect: who is moving against what the party wants, and when it will be too late.");
    }
    let _ = writeln!(text, "\nTIME: day {day}, {hour:02}:{minute:02} ({}).{}", world.daylight(), if world.is_night() { " It is night: honest folk sleep, shops are shut, and the dark is more dangerous." } else { "" });
    let _ = writeln!(text, "\nPLACE \"{}\": {}. {}", world.current, here.name, here.description);
    for prop in &here.props {
        let held = prop.chest.as_ref().map(|c| if c.open { " (opened)".to_string() } else if c.lock > 0 { format!(" (locked, lock {}{})", c.lock, if c.key.is_empty() { String::new() } else { format!(", key: {}", c.key) }) } else { " (shut)".to_string() }).unwrap_or_default();
        let _ = writeln!(text, "- prop {} at ({}, {}){held}: {} — {}", prop.id, prop.x, prop.y, prop.name, prop.description);
    }
    for creature in &here.creatures {
        let hurt = if creature.hurt > 0 { format!(" (wounded: {} hit points lost)", creature.hurt) } else { String::new() };
        let _ = writeln!(text, "- creature {} at ({}, {}), {}{hurt}: {} — {}", creature.id, creature.x, creature.y, attitude_word(creature.attitude), creature.name, creature.description);
    }
    for exit in &here.exits {
        let _ = writeln!(text, "- exit at ({}, {}) to {}: {}", exit.x, exit.y, exit.to, world.place_name(&exit.to));
    }
    let (x, y) = world.player.cell();
    let (here_w, here_h) = here.size();
    let _ = writeln!(text, "The party stands at ({x}, {y}). This place is {here_w} cells across and {here_h} down.");
    // What is around, by name: "this wall", "in front of me", "here" are these very cells.
    let cell_words = |cx: i32, cy: i32| cell_words(world, cx, cy);
    let ways = WAYS;
    if let Some((name, dx, dy)) = ways.iter().find(|(_, dx, dy)| (*dx, *dy) == world.facing) {
        let _ = writeln!(text, "The hero faces {name}. IN FRONT of the hero: {}; one further: {}.", cell_words(x + dx, y + dy), cell_words(x + dx * 2, y + dy * 2));
    }
    let around: Vec<String> = ways.iter().map(|(name, dx, dy)| format!("{name} {}", cell_words(x + dx, y + dy))).collect();
    let _ = writeln!(text, "AROUND the party: {}.", around.join("; "));
    // The map itself, so that walls between the party and anything are known.
    let _ = writeln!(text, "MAP of this place (x across, y down; the party is @). The two lines above it count x, the number before each row is y:");
    let _ = writeln!(text, "      {}", (0..here_w).map(|c| if c % 10 == 0 { char::from_digit((c / 10) as u32, 10).unwrap_or(' ') } else { ' ' }).collect::<String>());
    let _ = writeln!(text, "      {}", (0..here_w).map(|c| char::from_digit((c % 10) as u32, 10).unwrap_or(' ')).collect::<String>());
    for (row, line) in here.map.iter().enumerate() {
        let shown: String = line.chars().enumerate().map(|(col, c)| if (col as i32, row as i32) == (x, y) { '@' } else { c }).collect();
        let _ = writeln!(text, "  {row:>2}  {shown}");
    }
    let legend: Vec<String> = here.legend.iter().map(|(c, id)| format!("{c} {id}{}", if world.tiles.get(id).is_some_and(|t| t.walkable) { "" } else { " (solid)" })).collect();
    let _ = writeln!(text, "LEGEND: {}", legend.join(", "));
    // How far everything is on foot, around walls.
    let start = (x.clamp(0, here_w as i32 - 1) as usize, y.clamp(0, here_h as i32 - 1) as usize);
    let far = crate::path::steps(start, |(cx, cy)| here.blocked(cx as i32, cy as i32, &world.tiles));
    let walk = |(tx, ty): (usize, usize)| {
        let beside = [(0i32, -1i32), (1, 0), (0, 1), (-1, 0)].iter().filter_map(|(dx, dy)| {
            let (nx, ny) = (tx as i32 + dx, ty as i32 + dy);
            ((0..here_w as i32).contains(&nx) && (0..here_h as i32).contains(&ny)).then(|| far[ny as usize][nx as usize]).flatten()
        });
        match beside.min() {
            Some(0) => "beside the party".to_string(),
            Some(n) => format!("{n} steps away on foot"),
            None => "cannot be reached on foot from here".to_string(),
        }
    };
    for lying in &here.items {
        let _ = writeln!(text, "- {} lies on the ground at ({}, {})", lying.item.name, lying.x, lying.y);
    }
    let _ = writeln!(text, "WHAT IS WHERE FROM THE PARTY:");
    for prop in &here.props {
        let _ = writeln!(text, "- {} ({}): {}", prop.id, prop.name, walk((prop.x, prop.y)));
    }
    for creature in &here.creatures {
        let _ = writeln!(text, "- {} ({}): {}", creature.id, creature.name, walk((creature.x, creature.y)));
    }
    // The other heroes in this place, where they stand: a thing is handed only side by side.
    for (index, hero) in world.party.iter().enumerate().filter(|(i, h)| !h.dead && world.takes_part(*i)) {
        let (hx, hy) = world.hero_cell(index);
        if (hx, hy) == (x, y) || hx < 0 || hy < 0 || hx >= here_w as i32 || hy >= here_h as i32 {
            continue;
        }
        let _ = writeln!(text, "- the hero {} at ({hx}, {hy}): {}", hero.name, walk((hx as usize, hy as usize)));
    }
    for exit in &here.exits {
        let on_foot = match far[exit.y][exit.x] {
            Some(0) => "the party stands on it".to_string(),
            Some(n) => format!("{n} steps away on foot"),
            None => "cannot be reached on foot from here".to_string(),
        };
        let _ = writeln!(text, "- the way out to {} at ({}, {}): {on_foot}", world.place_name(&exit.to), exit.x, exit.y);
    }
    // Ground the party cannot get to from where it stands: what a bridge would be for.
    let cut_off = (0..here_h).flat_map(|cy| (0..here_w).map(move |cx| (cx, cy))).filter(|(cx, cy)| far[*cy][*cx].is_none() && !here.blocked(*cx as i32, *cy as i32, &world.tiles)).count();
    if cut_off > 0 {
        let _ = writeln!(text, "- {cut_off} walkable cells of this place cannot be reached on foot from where the party stands.");
    } else {
        let _ = writeln!(text, "- Every walkable cell of this place can be reached on foot from where the party stands: nothing has to be bridged or broken through to get anywhere here.");
    }
    if world.tired() {
        let _ = writeln!(text, "The party has not slept for more than a day: it is worn out (everything it tries is two harder). Say so, and let the weariness show.");
    }
    if !world.party.is_empty() && world.party.iter().all(|h| h.dead || h.hp <= 0) && !world.companions_here().is_empty() {
        let lead = &world.companions[world.companions_here()[0]].creature;
        let _ = writeln!(text, "THE HEROES ARE DOWN. The player now acts through the companion \"{}\" ({}), who carries on: tell the story from their side. A dead hero comes back only by a \"revive\" effect — a healer, a shrine, a bargain — which the companion may seek.", lead.name, lead.id);
    }
    let with: Vec<String> = world.companions_here().into_iter().map(|n| &world.companions[n]).map(|c| format!("{} \"{}\" ({} of {} hit points) — {}", c.creature.id, c.creature.name, c.hp, c.max_hp, c.creature.description)).collect();
    if !with.is_empty() {
        let _ = writeln!(text, "COMPANIONS travelling with the party (room for {} more): {}", world.room_for_companions(), with.join("; "));
    } else {
        let _ = writeln!(text, "No companions travel with the party; there is room for {}.", world.room_for_companions());
    }
    if !world.party.is_empty() {
        let _ = writeln!(text, "\n\nTHE PARTY:");
        // Those who are here: a party that has split up is told about scene by scene.
        for hero in world.party.iter().enumerate().filter(|(i, _)| !world.is_away(*i) && world.hero_place(*i) == world.current).map(|(_, h)| h) {
            let state = if hero.dead { " (dead)".to_string() } else { format!(", {} of {} hit points", hero.hp, hero.max_hp) };
            let index = world.party.iter().position(|h| std::ptr::eq(h, hero)).unwrap_or(0);
            let light = world.held_light(index).map(|l| format!("{} (lit, in hand)", l.name)).unwrap_or_default();
            let bag: Vec<String> = world.story.bags.get(index).map(|b| b.inventory.iter().map(|i| if i.count > 1 { format!("{} x{}", i.name, i.count) } else { i.name.clone() }).collect()).unwrap_or_default();
            let bag = if bag.is_empty() { "nothing".to_string() } else { bag.join(", ") };
            let gear = [hero.weapons.join(", "), hero.armor.clone().unwrap_or_default(), if hero.shield { "shield".to_string() } else { String::new() }, light].into_iter().filter(|g| !g.is_empty()).collect::<Vec<_>>().join("; ");
            let _ = writeln!(text, "- {}, {} {} of level {}{state}. Trained in: {}. Carries: {gear}. In the bag: {bag}. {}", hero.name, hero.race, hero.class, hero.level, hero.skills.join(", "), hero.description);
        }
    }
    // With no heroes made yet, what was found so far waits in the first bag.
    if world.party.is_empty()
        && let Some(bag) = world.story.bags.first().filter(|b| !b.inventory.is_empty())
    {
        let things: Vec<String> = bag.inventory.iter().map(|i| i.name.clone()).collect();
        let _ = write!(text, "
THE PARTY CARRIES: {}", things.join(", "));
    }
    text
}

const TURN_SHAPE: &str = r##"{
  "narration": "what happens, told to the players",
  "fight": false,
  "slain": ["the id of every creature the narration says dies or is struck down"],
  "gained": ["the name of every thing the narration says a hero got, found, took or was handed"],
  "lost": ["the name of every thing the narration says a hero lost, spent, gave away or broke"],
  "effects": [],
  "sprites": [{"id": "…", "rows": ["…"]}],
  "palette": {"g": "#rrggbb"},
  "tiles": [{"id": "…", "name": "…", "walkable": true, "rows": ["16 characters", "… 16 rows"]}],
  "options": [{"label": "something the party might do next", "outcome": "what happens if they do, written now", "effects": [], "near": "", "for": ""}],
  "parts": [{"to": ["a hero's name"], "narration": "…", "options": []}]
}
"narration" and "options" are for whoever acted. When another hero in this place must see or decide something of their own — a thing handed to them, a choice offered to them, what only they notice — give them a "part": "to" names them (or ["all"] for everyone here), "narration" is written for them alone — what the others just did and how it looks and sounds from where they stand, then what it means for them — so that they understand it without the rest, and "options" are their own choices. When several heroes acted together, give each of them a part. Each player is shown only their own part and their own choices; leave "parts" empty when nobody else is concerned.
An option that needs the party beside someone or something (talk to them, take it, open it) gives that thing's id — or another hero's name — as "near": the game lets the party choose it only once they have walked up to it, and keeps it on the thing to be found there. Every other option must be something the party can do from where it stands now."##;

/// With several players at the table one of them can be told something alone.
const WHISPERS: &str = r##"Several players sit at this table, each playing one character. Tell what a character does by that character's name, in the third person — "Bora reaches for it, slips and falls in the muck" — so that every player knows who did what; keep "you" for the whole party together. What one character does, that character does: when you walk someone up to a thing or send someone elsewhere ("move", "travel"), name that character in the effect — the others stay where their players left them, and "party" there means only those whose deeds you are answering; write "everyone" when the whole table is truly carried along. An option that only one character can take — the one who holds the key, who was spoken to, who is caught — gets "for": that character's name; leave "for" empty when anyone may take it. When one character alone would notice, know or be told something, add "whispers": [{"to": "that character's name", "text": "…"}] to the object: only that player reads it. Use it rarely; most turns need none."##;

fn whispers_note(world: &World) -> &'static str {
    let players = (0..world.party.len()).filter(|i| world.takes_part(*i)).count();
    if players > 1 { WHISPERS } else { "" }
}

/// A human game master sits at the table and tells the model what should happen; the model
/// writes it up as a turn, which the master reads before the players do.
pub fn directive(world: &World, said: &str, text: &str) -> String {
    let did = if said.trim().is_empty() { String::new() } else { format!("The party now does this: «{}»
", said.trim()) };
    format!(
        "{}
{did}A human game master runs this table and directs you: «{}»

Carry the direction out: tell what happens in your own words, with the effects it needs, and offer two to four \"options\" for what the party might do next, each with its outcome already written. Do not add events the direction does not call for. {}

Answer with this JSON object:
{TURN_SHAPE}",
        situation(world, 0..),
        text.trim(),
        whispers_note(world)
    )
}

/// What a table that plays by the book adds to every request.
fn book_note(world: &World) -> &'static str {
    if world.table.book_only {
        "\n\nBY THE BOOK. This table plays strictly by the D&D 5e SRD. Every creature that might fight must be an SRD monster: give \"monster\", never \"stats\". Do not invent abilities, spells or magic items the SRD does not have."
    } else {
        ""
    }
}

/// The master is asked for abilities of a hero's own: three to choose from at a level, or
/// one secret skill after an unusual deed.
pub fn feats(world: &World, hero: usize, secret: bool) -> String {
    let who = world.party.get(hero).map(|h| format!("{}, {} {} of level {}. {}", h.name, h.race, h.class, h.level, h.description)).unwrap_or_default();
    let ask = if secret {
        "This character has just done something out of the ordinary and earned a secret skill for it. Write ONE, born of that deed and of what the character has been through.".to_string()
    } else {
        "This character has reached a new level. Offer THREE abilities to choose from, each different in kind, each grown out of what this character has done and been through in the story — not generic ones.".to_string()
    };
    format!(
        "{}\nTHE CHARACTER: {who}\n\n{ask}\n\nAn ability is put together only from what the game can carry out:\n- harm or healing: \"dice\" (like 2d6), \"heals\" true for healing, \"range\" in feet, \"area\" in feet around the target (0 for one target), \"save\" (str, dex, con, int, wis, cha) when the targets resist instead of the character rolling to hit, \"uses\" between long rests (0 for at will);\n- or plain numbers that always count: \"ac\" (at most 1), \"hp\", \"speed\" in feet.\nKeep it modest for the level; the game cuts down what is too strong. Give each a name and one or two sentences of description that say what it looks like in play.\n\nAnswer with this JSON object:\n{{\"feats\": [{{\"name\": \"…\", \"description\": \"…\", \"dice\": \"2d6\", \"heals\": false, \"range\": 30, \"area\": 0, \"save\": null, \"uses\": 2, \"ac\": 0, \"hp\": 0, \"speed\": 0}}]}}",
        situation(world, 0..)
    )
}

/// A hero tries something in a fight that the rules have no button for.
pub fn stunt(world: &World, said: &str) -> String {
    let mut text = situation(world, 0..);
    if let Some(fight) = &world.fight {
        let _ = writeln!(text, "\nA FIGHT IS ON. It is {}'s turn. Those fighting:", fight.actor().name);
        for c in fight.combatants.iter().filter(|c| !c.dead) {
            let side = if c.side == rules::Side::Party { "party" } else { "enemy" };
            let _ = writeln!(text, "- {} \"{}\" ({side}) at ({}, {}), {} of {} hit points", c.id, c.name, c.x, c.y, c.hp.max(0), c.max_hp);
        }
    }
    format!(
        "{text}\nOn their turn the acting character tries this: «{}»\n\nThe rules have no button for it, so you judge it. Name the check it takes and how hard it is, and say what a success is worth: harm to one of those fighting (\"target\" is its id), or healing, in dice suited to the character's level — or nothing but an advantage in the story. Never roll yourself. Write what it looks like, then one sentence for success and one for failure. Whether anyone falls is decided by the dice and the hit points, after your words are read: never write that a creature dies, is destroyed or falls, nor that the character is struck down — describe the blow landing or missing, and the game says who drops.\n\nAnswer with this JSON object:\n{{\"narration\": \"the attempt\", \"ability\": \"str|dex|con|int|wis|cha\", \"skill\": \"athletics or null\", \"dc\": 13, \"target\": \"id or empty\", \"damage\": \"1d8 or empty\", \"heal\": \"\", \"success\": \"…\", \"failure\": \"…\"}}",
        said.trim()
    )
}

/// The master draws a hero as the player described it.
pub fn portrait(world: &World, hero: usize) -> String {
    let who = world.party.get(hero).map(|h| format!("{}, a {} {}. {}", h.name, h.race, h.class, h.description)).unwrap_or_default();
    format!(
        "Draw the player character {who}\nWorld: {}. {}\n\nOne sprite, 16 by 16, drawn the way you draw every creature of this world: rows of palette characters, '0' transparent, a one-pixel outline in colour 1, feet on the bottom row, facing the viewer. It must read as this very character at a glance: the build, the gear of the class, one telling detail from the description.\n\nAnswer with this JSON object:\n{{\"rows\": [\"16 characters\", \"… 16 rows\"]}}",
        world.title, world.summary
    )
}

/// Things that came into the world without a picture of the master's: each is drawn to
/// fill the cells it takes in the bag.
pub fn draw_things(world: &World, things: &[crate::bag::Undrawn]) -> String {
    let list: Vec<String> = things
        .iter()
        .map(|t| format!("- \"{}\"{}{}: {} by {} cells", t.name, if t.kind.is_empty() { String::new() } else { format!(" ({})", t.kind) }, if t.description.trim().is_empty() { String::new() } else { format!(" — {}", t.description.trim()) }, t.size.0, t.size.1))
        .collect();
    let palette: String = world.palette.colors.iter().enumerate().map(|(i, [r, g, b])| format!(" {}=#{r:02x}{g:02x}{b:02x}", char_of(i as u8 + 1))).collect();
    format!(
        "World: {}. {}\nPALETTE (0 is transparent):{palette}\n\nThese things are carried, lie about or are sold here, and have no picture yet. Draw each as an icon for the bag, recognisable at a glance, in this world's palette: 16 rows of 16 palette characters for every cell of its size ('0' transparent), so a thing 1 by 2 cells is 16 across and 32 down, filling most of its cells, outlined in colour 1. A long thing (a spear, a staff, a sword) stands upright. You may correct the size where it is plainly wrong (a potion 1 by 1, a dagger 1 by 2, a sword 1 by 3, a bow or armour 2 by 3, a spear 1 by 4).\n\n{}\n\nAnswer with this JSON object, one entry for every thing, with its name exactly as given:\n{{\"things\": [{{\"name\": \"…\", \"size\": [1, 2], \"rows\": [\"…\"]}}]}}",
        world.title,
        world.summary,
        list.join("\n")
    )
}

/// One picture the human master asks for: a tile of ground or a sprite of a creature or a
/// thing, drawn in the world's palette and its look.
pub fn draw_art(world: &World, tile: bool, id: &str, description: &str) -> String {
    let palette: String = world.palette.colors.iter().enumerate().map(|(i, [r, g, b])| format!(" {}=#{r:02x}{g:02x}{b:02x}", char_of(i as u8 + 1))).collect();
    let look = place_context(world, "", &world.current);
    let what = if tile {
        "a TILE of ground, 16 by 16, seen from above, filling every pixel (no transparency), seamless so it can lie beside itself"
    } else {
        "a SPRITE, 16 by 16, '0' transparent, outlined in colour 1, facing the viewer, feet on the bottom row"
    };
    format!(
        "World: {}. {}\nPALETTE (0 is transparent):{palette}\n{look}\nThe game master wants {what}: \"{id}\" — {}.\nDraw it alike the pictures above, recognisable at a glance.\n\nAnswer with this JSON object:\n{{\"rows\": [\"16 characters\", \"… 16 rows\"]}}",
        world.title,
        world.summary,
        description.trim()
    )
}

/// What the master is told it is when it only draws things: short, so a drawing does not
/// carry the rules of the whole game.
pub const DRAWER: &str = "You draw pixel-art icons for a tabletop role-playing game. A picture is a list of rows; a row is a string; each character is one pixel holding a palette index: 0 is transparent, then 1-9, then a-v. You answer with ONE JSON object and nothing else.";

/// A player who reads another language than the world is told in asks for a translation.
pub fn translate(texts: &[String], language: &str) -> String {
    let list = serde_json::to_string(texts).unwrap_or_default();
    format!(
        "Translate these pieces of a tabletop role-playing game into {language}: tellings by the game master, names of places, things and creatures, things the players may choose to do. Keep the tone and the person (\"you\" stays \"you\"), keep names recognisable, keep numbers, dice (2d6) and line breaks as they are. Translate every piece; a piece already in {language} comes back unchanged.\n\n{list}\n\nAnswer with this JSON object, the translations in the same order and of the same number:\n{{\"texts\": [\"…\"]}}"
    )
}

/// A human game master asks how the rules settle something that happened at the table.
/// `book` is the rules of this table as the master reads them; the master's own guide —
/// how the model would run the game — goes with it, so that the answer follows this game
/// and not some other.
pub fn ruling(world: &World, book: &str, question: &str, language: &str) -> String {
    format!(
        "A human game master runs this game and asks you, as an adviser who knows its rules by heart, how to settle something by the rules. Answer in {language}: first the ruling in one or two sentences (what to roll, against what, what happens on success and on failure), then why, naming the rule. When the rules leave it to the master, say so and suggest a fair way. Be short and concrete: the master is at the table and players are waiting.

THE RULES OF THIS TABLE:
{book}

HOW THE GAME MASTER RUNS THIS GAME (the game's own rules for the master):
{guide}

THE PARTY: {party}

WHAT HAPPENED, IN THE MASTER'S WORDS:
{question}

Answer with this JSON object: {{\"answer\": \"…\"}}",
        guide = play_prompt(world),
        party = world.party.iter().map(|h| format!("{} (level {} {} {}, {}/{} HP, AC {})", h.name, h.level, h.race, h.class, h.hp, h.max_hp, h.ac())).collect::<Vec<_>>().join("; "),
        question = question.trim(),
    )
}

/// A human game master describes in words what should happen, and the model writes it as
/// effects: a script, a trap, a mechanism, a chain of events with pauses.
pub fn script(world: &World, about: &str, wish: &str) -> String {
    format!(
        "A human game master is building this game and asks you to write what they describe as EFFECTS — the very list you would write in an answer — so that the game can do it by itself. Use only effects listed in your rules. Use \"wait\" for pauses, \"set\"/\"add\"/\"if\" to remember and to choose, \"random\" for chance, \"repeat\" for loops, \"check\" when a hero must roll. Ids must be ones of this place or spawned in the same list; \"self\" is the thing it belongs to.

WHAT IT BELONGS TO: {about}
THE PLACE: {place}
WHAT THE MASTER WANTS:
{wish}

Answer with this JSON object: {{\"effects\": [ … ]}}",
        about = about.trim(),
        place = situation(world, world.story.log.len().saturating_sub(4)..),
        wish = wish.trim(),
    )
}

/// A human game master asks for a creature to keep in their library.
pub fn invent(world: &World, request: &str) -> String {
    format!(
        "A human game master asks you for a creature to keep and use in their games: «{}»
The world it is first meant for: {}. Party level: {}.

Draw it as a sprite the same way you draw everything else (rows of palette characters, at most 16 by 16, '0' transparent). {FIGHTERS}

Answer with this JSON object:
{{\"id\": \"snake_case_id\", \"name\": \"…\", \"description\": \"one or two sentences\", \"rows\": [\"…\"], \"monster\": null, \"stats\": {{\"ac\": 13, \"hp\": 11, \"speed\": 30, \"attacks\": [{{\"name\": \"…\", \"bonus\": 4, \"damage\": \"1d6+2\", \"range\": 5}}], \"xp\": 50}}}}",
        request.trim(),
        world.title,
        world.party_level()
    )
}

/// The same request again, with what was wrong with the answer: the game checked it and it
/// could not be played as told.
pub fn mend(asked: &str, faults: &[String]) -> String {
    let mut text = format!("{asked}

YOUR ANSWER TO THIS WAS CHECKED BY THE GAME AND CANNOT BE PLAYED AS IT IS:
");
    for fault in faults {
        let _ = writeln!(text, "- {fault}");
    }
    text.push_str("Answer again with the whole JSON object, the same story, every fault mended: each thing in \"gained\" has its \"give\" with \"rows\", each in \"lost\" its \"take\", each in \"slain\" its \"kill\", a fight its \"fight\" effect, every id, tile and sprite is one that exists or is made in this answer.");
    text
}

/// The party does something the master has not prepared an outcome for. `near` is the thing
/// they stand next to, if any.
/// What a choice that was picked is sent to the master as: the outcome is in the story
/// already, so the master goes on from there.
pub const GO_ON: &str = "[this choice was taken and its outcome is told above; go on from there: what follows, and new options]";

pub fn turn(world: &World, said: &str, near: Option<&str>) -> String {
    let beside = near.map(|id| format!(" They are standing next to \"{id}\".")).unwrap_or_default();
    format!(
        "{}\nThe party now does this: «{}»{beside}\n\nTell what happens. Offer two to four \"options\" for what the party might do next, each with its outcome already written, so that picking one needs no further question to you. Leave \"sprites\" empty unless an effect needs a sprite that does not exist. {}{}\n\nAnswer with this JSON object:\n{TURN_SHAPE}",
        situation(world, 0..),
        said.trim(),
        whispers_note(world),
        undrawn_note(world)
    )
}

/// How many things without a picture one turn is asked to draw besides its telling.
pub const DRAWN_WITH_A_TURN: usize = 4;

/// Things that came into the world without a picture, for the master to draw along with the
/// turn instead of in a request of their own.
fn undrawn_note(world: &World) -> String {
    let things = world.undrawn_things(&[], DRAWN_WITH_A_TURN);
    if things.is_empty() {
        return String::new();
    }
    let list: Vec<String> = things
        .iter()
        .map(|t| format!("- \"{}\"{}{}: {} by {} cells", t.name, if t.kind.is_empty() { String::new() } else { format!(" ({})", t.kind) }, if t.description.trim().is_empty() { String::new() } else { format!(" — {}", t.description.trim()) }, t.size.0, t.size.1))
        .collect();
    format!(
        "\n\nTHINGS WITHOUT A PICTURE YET: draw each as an icon for the bag in \"pictures\", besides the telling: {{\"name\": exactly as given, \"size\": [across, down] in cells, \"rows\": 16 rows of 16 palette characters for every cell of its size, '0' transparent, outlined in colour 1, the thing filling most of it, a long thing standing upright}}. You may correct a size that is plainly wrong.\n{}",
        list.join("\n")
    )
}

/// The players have been idle. The master may make something happen, or not.
pub fn event(world: &World) -> String {
    format!(
        "{}\nThe party has done nothing for a while. If this place and this story call for it, make something happen now of its own accord: someone arrives, something breaks, a sound, a change in the weather that matters. If nothing should happen, answer with an empty \"narration\".\n\nAnswer with this JSON object:\n{TURN_SHAPE}",
        situation(world, 0..)
    )
}

/// Something hostile has come upon the party while it lingered, and the master never drew
/// it: it draws it now, sets it down and tells how it shows itself. Nothing appears until
/// this is answered.
pub fn ambush(world: &World, comer: &crate::model::Wanderer, cell: (usize, usize)) -> String {
    let who = if comer.name.is_empty() { comer.monster.clone() } else { format!("{} ({})", comer.name, comer.monster) };
    let about = if comer.description.is_empty() { String::new() } else { format!(" — {}", comer.description) };
    format!(
        "{}\nTHE PARTY HAS LINGERED IN A DANGEROUS PLACE, AND SOMETHING HAS FOUND IT: {who}{about}. This is decided; do not turn it into nothing.\n\
         1. DRAW it: one new picture in \"sprites\" (16 by 16, rows of palette characters, '0' transparent, outlined in colour 1, facing the viewer), drawn as this very creature of this world — not a sprite that exists.\n\
         2. SET IT DOWN with a \"spawn\" effect: \"kind\": \"creature\", a new \"id\", its \"name\" and \"description\", \"sprite\": the id you just drew, \"x\": {}, \"y\": {} (or a free cell beside that), \"attitude\": \"hostile\", \"monster\": \"{}\"; leave \"count\" out to let the game match the pack to the party, or give it.\n\
         3. TELL in \"narration\" how it shows itself — what the party hears, smells and sees first, from which side it comes, and one thing that makes this meeting its own (it is wounded, it drags prey, it is driven by something worse, it wears a faction's mark). Two to four sentences. Add \"music\" if the mood turns, \"shake\" or \"animate\" if it fits.\n\
         Leave \"options\" empty: the fight begins at once.\n\nAnswer with this JSON object:\n{TURN_SHAPE}",
        situation(world, 0..),
        cell.0,
        cell.1,
        comer.monster
    )
}

/// Things said aloud by those in a place while the party walks about: a handful at once,
/// to be shown one by one as speech bubbles.
pub fn chatter(world: &World) -> String {
    let here = world.here();
    let mut text = format!("WORLD: {}. {}\nTone: {}.\nPLACE: {}. {}\n", world.title, world.summary, world.tone, here.name, here.description);
    let (day, hour, minute) = world.time();
    let _ = writeln!(text, "TIME: day {day}, {hour:02}:{minute:02} ({}).", world.daylight());
    let _ = writeln!(text, "WHO IS HERE:");
    for c in here.creatures.iter().filter(|c| c.attitude != crate::model::Attitude::Hostile) {
        let _ = writeln!(text, "- {} \"{}\": {}", c.id, c.name, c.description);
    }
    for n in world.companions_here() {
        let c = &world.companions[n].creature;
        let _ = writeln!(text, "- {} \"{}\" (travels with the party): {}", c.id, c.name, c.description);
    }
    if world.quest.is_set() && world.quest.done.is_none() {
        let _ = writeln!(text, "The party's goal: {}", world.quest.goal);
    }
    for threat in world.threats.iter().filter(|t| !t.rumour.is_empty()) {
        let _ = writeln!(text, "A rumour going round: {}", threat.rumour);
    }
    let recent: Vec<&str> = world.story.log.iter().rev().take(3).map(|l| l.text.as_str()).collect();
    for line in recent.into_iter().rev() {
        let _ = writeln!(text, "Lately: {}", line.chars().take(240).collect::<String>());
    }
    text.push_str("\nWrite 6 to 8 short things these people say aloud of their own accord over the next minutes — to each other, to themselves, to the party passing by: a grumble, gossip, a hint about the goal or the rumour, a remark on what the party just did, a companion's opinion. One sentence each, at most 12 words, in character, no narration, no quotation marks. Use only the ids listed.\n\nAnswer with this JSON object:\n{\"lines\": [{\"who\": \"id\", \"text\": \"…\"}]}");
    text
}

/// The story has grown long: the master writes the older part up as a chapter.
pub fn chapter(world: &World) -> String {
    chapter_upto(world, world.story.log.len().saturating_sub(KEPT_LINES))
}

/// A chapter of the first `upto` lines of the word-for-word story: the rest waits for a
/// later one, so that the request fits what the model reads.
pub fn chapter_upto(world: &World, upto: usize) -> String {
    let mut older = world.clone();
    older.story.log.truncate(upto);
    format!(
        "{}\nThe story above has grown long. Write it up as one chapter so that nothing which matters later is lost: who was met and what they want, what was learned, what was gained and lost, what was promised, what was left unfinished, in the order it happened — and which hero did, said, got and promised what, by name, for at a table of several each player's hero has a story of its own. Be detailed; this chapter replaces the story above. Then list as \"facts\" anything that must hold true for the rest of the campaign and is not yet among the facts.\n\nAnswer with this JSON object:\n{{\"chapter\": \"…\", \"facts\": [\"…\"]}}",
        situation(&older, 0..)
    )
}

/// The oldest `count` chapters, written up as one, so that the chapters themselves do not
/// outgrow what the model reads.
pub fn merge_chapters(world: &World, count: usize) -> String {
    let count = count.min(world.story.chapters.len());
    let chapters: Vec<String> = world.story.chapters[..count].iter().enumerate().map(|(n, c)| format!("CHAPTER {}:\n{c}", n + 1)).collect();
    format!(
        "World: {}. {}\n\nFACTS:\n{}\n\nThese are the oldest chapters of the campaign so far:\n\n{}\n\nWrite them up as ONE chapter that keeps everything which matters later — who was met and what they want, what was learned, gained, lost and promised, what is unfinished, and which hero did and got what, by name — in the order it happened, as detailed as it needs to be but shorter than all of them together. Then list as \"facts\" anything that must hold true for the rest of the campaign and is not yet among the facts.\n\nAnswer with this JSON object:\n{{\"chapter\": \"…\", \"facts\": [\"…\"]}}",
        world.title,
        world.summary,
        world.story.facts.join("\n"),
        chapters.join("\n\n")
    )
}

/// A rough count of the tokens a text takes: three characters to a token is close for
/// mixed text and errs on the side of writing the story up early.
pub fn tokens(text: &str) -> usize {
    text.chars().count().div_ceil(3)
}

/// What writing the story up shorter takes next: a chapter of the first lines of the
/// word-for-word story, or the oldest chapters into one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compaction {
    Chapter { upto: usize },
    Merge { count: usize },
}

/// Share of what the model reads at which the story is written up shorter.
pub const COMPACT_AT: f32 = 0.85;

/// Whether the story should be written up shorter for a request of a turn to stay within
/// `room` tokens (what the model reads, less what is kept for its answer), and how: a piece
/// small enough that the request to write it up fits in `room` too, so that no story is ever
/// too long to go on with. `urgent` when a turn would not fit at all.
pub fn compaction(world: &World, room: usize) -> Option<(Compaction, bool)> {
    let turn = tokens(&play_prompt(world)) + tokens(&turn(world, "", None));
    let urgent = turn >= room;
    if (turn as f32) < room as f32 * COMPACT_AT {
        return None;
    }
    let fits = |prompt: &str| tokens(&play_prompt(world)) + tokens(prompt) <= room * 9 / 10;
    let story = &world.story;
    // Chapters that have grown to more than a third of the room are written up first.
    let chapters: usize = story.chapters.iter().map(|c| tokens(c)).sum();
    let merge = |story: &crate::story::Story| -> Option<Compaction> {
        (2..=story.chapters.len()).rev().find(|&count| fits(&merge_chapters(world, count))).or((story.chapters.len() >= 2).then_some(2)).map(|count| Compaction::Merge { count })
    };
    if chapters * 3 > room && story.chapters.len() >= 2 {
        return merge(story).map(|m| (m, urgent));
    }
    let most = story.log.len().saturating_sub(KEPT_LINES);
    if most == 0 {
        return merge(story).map(|m| (m, urgent));
    }
    // As much of the oldest story as one request can carry: halved until it fits.
    let mut upto = most;
    while upto > 1 && !fits(&chapter_upto(world, upto)) {
        upto /= 2;
    }
    Some((Compaction::Chapter { upto }, urgent))
}

/// A request to draw a place that so far is only a name. `from` is where the party walks in
/// from.
pub fn new_location(world: &World, id: &str, from: &str) -> String {
    new_location_sized(world, id, from, false)
}

/// Another quarter of a large place: `k` is 1 for the north-east, 2 for the south-west, 3 for
/// the south-east. `drawn` are the quarters already there, north-west first.
pub fn location_quarter(world: &World, name: &str, description: &str, legend: &std::collections::BTreeMap<String, String>, drawn: &[Vec<String>], k: usize) -> String {
    let which = ["north-west", "north-east", "south-west", "south-east"];
    let mut text = format!("WORLD: {}. {}\nTone: {}.\n\nYou are drawing the large place \"{name}\": {description}\nIt is drawn in four quarters of {MAP_H} rows of {MAP_W} characters. Already drawn:\n", world.title, world.summary, world.tone);
    for (n, rows) in drawn.iter().enumerate() {
        let _ = writeln!(text, "\nThe {} quarter:", which[n]);
        for row in rows {
            let _ = writeln!(text, "  {row}");
        }
    }
    let _ = write!(text, "\nLEGEND so far:");
    for (ch, tile) in legend {
        let _ = write!(text, " {ch} {tile},");
    }
    let _ = write!(text, "\nTILES THAT EXIST:");
    for tile in world.tiles.keys() {
        let _ = write!(text, " {tile}");
    }
    let _ = write!(text, "\nSPRITES THAT EXIST:");
    for sprite in world.sprites.keys() {
        let _ = write!(text, " {sprite}");
    }
    let joins = match k {
        1 => "Its WEST side (column 0) joins the east side of the north-west quarter: roads, walls, water and paths must run on across the seam.",
        2 => "Its NORTH side (row 0) joins the south side of the north-west quarter: roads, walls, water and paths must run on across the seam.",
        _ => "Its WEST side joins the south-west quarter and its NORTH side joins the north-east quarter: roads, walls, water and paths must run on across both seams.",
    };
    let _ = write!(
        text,
        "\n\nNow draw the {} quarter. {joins} Its outer sides are the edge of the whole place: ways out to other places may lie there, nowhere else. Use the same legend; add a character only for a tile not in it yet. Coordinates of props, creatures, exits and secrets are counted inside this quarter (x 0-{}, y 0-{}).\n\nAnswer with this JSON object:\n{{\"legend\": {{}}, \"map\": [\"{MAP_W} characters\", \"… {MAP_H} rows\"], \"new_tiles\": [], \"sprites\": [], \"props\": [], \"creatures\": [], \"exits\": [], \"neighbors\": [], \"secrets\": []}}\nEach list holds things shaped as in a place you draw whole.",
        which[k],
        MAP_W - 1,
        MAP_H - 1
    );
    text
}

/// As `new_location`; with `large`, the master may draw a place twice as wide and twice as
/// tall. Asked only of the best models: a slip in so long a map spoils the whole place.
pub fn new_location_sized(world: &World, id: &str, from: &str, large: bool) -> String {
    let mut text = String::new();
    if !large {
        let _ = writeln!(
            text,
            "A LARGE PLACE, IN QUARTERS. When this place truly is big — a town, a harbour, a forest, a battlefield — add \"large\": true. The map you draw now ({} rows of {}) is then only its NORTH-WEST quarter: put the way back and the party's spawn in it, and leave its east and south sides open. You will be asked for the north-east, south-west and south-east quarters next, each with what is already drawn. A small place — a room, a hut, a clearing — is drawn whole, without \"large\".\n",
            MAP_H, MAP_W
        );
    }
    if large {
        let _ = writeln!(
            text,
            "A LARGE PLACE. When this place truly is big — a town, a harbour, a forest, a battlefield, a warren of caves — you may draw it large: \"map\" is then {} rows of exactly {} characters (x 0-{}, y 0-{}), with streets, quarters, several buildings, more people and things spread across all of it, and exits on its far edges. Count the characters of every row. A small place — a room, a hut, a clearing, a cave — stays {} rows of {}.\n",
            crate::model::MAX_H,
            crate::model::MAX_W,
            crate::model::MAX_W - 1,
            crate::model::MAX_H - 1,
            MAP_H,
            MAP_W
        );
    }
    let _ = writeln!(text, "WORLD: {}. {}\nTone: {}.", world.title, world.summary, world.tone);
    let _ = write!(text, "\nPALETTE:");
    for (i, [r, g, b]) in world.palette.colors.iter().enumerate() {
        let _ = write!(text, " {}=#{r:02x}{g:02x}{b:02x}", char_of(i as u8 + 1));
    }
    let free: String = (world.palette.len()..crate::model::PALETTE_MAX).map(|i| char_of(i as u8)).collect();
    let _ = write!(text, "\nFREE PALETTE LETTERS (give each a colour in \"palette\" to draw with it): {}", if free.is_empty() { "none" } else { &free });
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
    let _ = write!(text, "{}", place_context(world, id, from));
    // The human master draws this place again, as they want it.
    if !world.redraw_wish.trim().is_empty() {
        let ways: Vec<String> = world.locations.get(id).map(|l| l.exits.iter().map(|e| format!("{} ({})", e.to, world.place_name(&e.to))).collect()).unwrap_or_default();
        let _ = write!(text, "\nTHE GAME MASTER HAS THIS PLACE DRAWN AGAIN, AS THEY WANT IT: {}\nKeep the same id and its ways out leading where they lead now: {}.\n", world.redraw_wish.trim(), if ways.is_empty() { "none".to_string() } else { ways.join(", ") });
    }
    let (name, description) = world.stubs.get(id).map(|s| (s.name.as_str(), s.description.as_str())).unwrap_or((id, ""));
    let _ = write!(
        text,
        "\nDraw the place \"{id}\": {name}. {description}\nYou are NOT bound to what was drawn before. The story moves on: when this place is unlike the last ones — another land, the sky, open space, a ship, a dream, a city after a swamp — give it ground, walls, things and creatures of its own: draw \"new_tiles\" and \"sprites\" for it and add colours in \"palette\" under the free letters; reuse an old tile or sprite only where that very thing would be found here. The hour and the sky follow on from where the party comes from ({}, {}) unless this place is indoors, underground or the story says they changed.\nThe party walks in from \"{from}\" ({}): one exit must lead back to \"{from}\", on the side that makes sense. Other exits may lead to places that exist or to new ones.\n\nAnswer with this JSON object (\"id\" must be \"{id}\"):\n{LOCATION_SHAPE}",
        world.locations.get(from).map_or("day", |l| if l.light.is_empty() { "day" } else { l.light.as_str() }),
        world.locations.get(from).map_or("clear", |l| if l.weather.is_empty() { "clear" } else { l.weather.as_str() }),
        world.place_name(from)
    );
    text
}

/// What a new place is drawn with besides the world's lists: how this world's pictures
/// look (one creature and one tile, pixel for pixel, so new ones are drawn alike), which
/// side of the last place the way here leaves from (the way back goes on the facing side),
/// and the last words of the story (why the party comes).
fn place_context(world: &World, id: &str, from: &str) -> String {
    let mut text = String::new();
    let creature = world.locations.get(from).into_iter().chain(world.locations.values()).flat_map(|l| l.creatures.iter()).find_map(|c| world.sprites.get(&c.sprite).filter(|s| s.rows.len() >= 8).map(|s| (c.sprite.clone(), s)));
    let tile = world.tiles.iter().filter(|(_, t)| !t.walkable && t.sprite.rows.len() >= 8).max_by_key(|(_, t)| t.sprite.rows.iter().flat_map(|r| r.chars()).collect::<std::collections::BTreeSet<_>>().len());
    if creature.is_some() || tile.is_some() {
        let _ = write!(text, "\nTHE LOOK OF THIS WORLD — draw new pictures alike (detail, outline, shading):");
        if let Some((name, sprite)) = creature {
            let _ = write!(text, "\nThe creature \"{name}\":\n{}", sprite.rows.join("\n"));
        }
        if let Some((name, def)) = tile {
            let _ = write!(text, "\nThe tile \"{name}\":\n{}", def.sprite.rows.join("\n"));
        }
    }
    if let Some(exit) = world.locations.get(from).and_then(|l| l.exits.iter().find(|e| e.to == id).map(|e| (e, l.size()))) {
        let ((e, (w, h)), name) = (exit, world.place_name(from));
        let side = if e.x == 0 { "west" } else if e.x + 1 >= w { "east" } else if e.y == 0 { "north" } else if e.y + 1 >= h { "south" } else { "" };
        let back = match side { "west" => "east", "east" => "west", "north" => "south", "south" => "north", _ => "" };
        if back.is_empty() {
            let _ = write!(text, "\n\nThe way here leaves {name} from inside it, at ({}, {}).", e.x, e.y);
        } else {
            let _ = write!(text, "\n\nThe way here leaves {name} on its {side} side: the way back is on the {back} side of this place.");
        }
    }
    let recent: Vec<&str> = world.story.log.iter().rev().take(4).map(|l| l.text.as_str()).collect();
    if !recent.is_empty() {
        let _ = write!(text, "\nTHE LAST OF THE STORY:\n{}", recent.into_iter().rev().collect::<Vec<_>>().join("\n"));
    }
    text.push('\n');
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

    fn test_world() -> World {
        let draft: WorldDraft = serde_json::from_str(&fake::world("test")).unwrap();
        build_world(&draft, "test", "Russian").unwrap().0
    }

    #[test]
    fn drawing_rules_ask_for_prepared_actions_and_explain_effects() {
        let p = system_prompt("English");
        assert!(p.contains("ACTIONS.") && p.contains("\"op\": \"give\"") && p.contains("\"self\""));
        assert!(p.contains("\"op\": \"check\"") && p.contains("\"op\": \"fight\"") && p.contains("\"monster\""));
    }

    #[test]
    fn telling_rules_follow_the_tables_language_and_content_switches() {
        let mut w = test_world();
        let open = play_prompt(&w);
        assert!(open.contains("written in Russian") && open.contains("graphically") && open.contains("explicit"));
        w.violence = false;
        w.sex = false;
        let closed = play_prompt(&w);
        assert!(closed.contains("off the page") && closed.contains("fade to black") && !closed.contains("explicit"));
    }

    #[test]
    fn a_table_that_reads_several_languages_is_told_in_each() {
        let mut w = test_world();
        assert!(!play_prompt(&w).contains("told_in"), "one language: nothing more is asked");
        w.table_languages = vec!["Russian".into(), "English".into()];
        let prompt = play_prompt(&w);
        assert!(prompt.contains("read English") && prompt.contains("\"told_in\": {\"English\": \"…\"}"), "{prompt}");
        assert!(!prompt.contains("read Russian"), "the world's own language is not asked twice");
        // What the master wrote comes back to each reader in their language.
        let draft: crate::TurnDraft = serde_json::from_str(r#"{"narration": "Дверь скрипит.", "told_in": {"English": "The door creaks.", "Russian": "x"}}"#).unwrap();
        let (turn, _) = w.take_turn("open the door", &draft);
        assert_eq!(turn.told_in.keys().collect::<Vec<_>>(), ["English"]);
        let english = turn.read_in("English");
        assert_eq!((english.narration.as_str(), english.language.as_str()), ("The door creaks.", "English"));
        let russian = turn.read_in("Russian");
        assert_eq!((russian.narration.as_str(), russian.language.as_str()), ("Дверь скрипит.", ""));
        assert!(russian.told_in.is_empty(), "a reader gets one language, not all");
    }

    #[test]
    fn a_turn_request_shows_the_place_the_things_the_bag_and_the_story() {
        let mut w = test_world();
        w.perform("cat", 1);
        w.story.remember("The well is dry.");
        w.story.chapters.push("They arrived at dusk.".into());
        let text = turn(&w, "  I look under the roots ", Some("fire"));
        assert!(text.contains("- prop fire at (") && text.contains("Campfire"));
        assert!(!text.contains("creature cat"), "the cat left");
        assert!(text.contains("- The well is dry.") && text.contains("CHAPTER 1: They arrived at dusk."));
        assert!(text.contains("Cat hair"));
        assert!(text.contains("PARTY: Shoo it away") && text.contains("GM: The cat slips away"));
        assert!(text.contains("«I look under the roots»") && text.contains("next to \"fire\""));
        assert!(text.contains("- exit at (") && text.contains("The East Road"));
    }

    #[test]
    fn a_chapter_request_leaves_out_the_lines_that_stay_word_for_word() {
        let mut w = test_world();
        for n in 0..10 {
            w.story.say(Speaker::Master, &format!("line number {n}"));
        }
        let text = chapter(&w);
        assert!(text.contains("line number 3") && !text.contains("line number 4"));
        assert!(text.contains("\"chapter\""));
    }

    #[test]
    fn world_request_carries_the_hosts_wish_or_says_the_choice_is_free() {
        assert!(new_world("динозавры и лазеры", true).contains("«динозавры и лазеры»"));
        assert!(new_world("  ", false).contains("left the choice to you"));
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

    #[test]
    fn two_turns_one_after_another_begin_alike_up_to_what_is_true_now() {
        let mut w = test_world();
        w.story.say(Speaker::Master, "The fire crackles.");
        let first = turn(&w, "we look around", None);
        // A turn later: the story has grown at its end, the clock and the party have moved.
        w.story.say(Speaker::Player, "we look around");
        w.story.say(Speaker::Master, "Nothing stirs.");
        w.clock += 25;
        w.player.x += 16.0;
        let second = turn(&w, "we walk on", None);
        let same = first.chars().zip(second.chars()).take_while(|(a, b)| a == b).count();
        let story_ends = first.find("WHAT IS TRUE NOW").unwrap();
        // Everything before the story's newest lines is the same: only what changes is after.
        let first_line = first.find("GM: The fire crackles.").unwrap() + "GM: The fire crackles.".len();
        assert!(same >= first_line, "the turns part at {same}, before the story at {first_line}");
        assert!(story_ends > first_line);
        assert!(first.find("TIME:").unwrap() > story_ends);
    }
}
