# PixelGM — design decisions

Working title. A 2D pixel-art co-op tabletop RPG where a Gemini model is the game master and
draws the world. Built for the author and friends: not sold, but finished and polished.

Everything below was decided in the design interview of 2026-10-02. Change it here when a
decision changes.

## Product

- 1–4 players plus an optional human game master. Single player is the same host without a lobby.
- Windows first, Linux and Steam Deck later. Keyboard + mouse and gamepad; free text from a pad
  goes through the Steam on-screen keyboard.
- Interface in Russian and English; the font carries full Cyrillic. The GM answers each player
  in that player's language.
- 18+: violence and sexual content are in scope, with host toggles per world (both on by
  default). Model safety settings are sent as `OFF`. No content filter in game code.
- Endless campaign.

## Engine and layout

- Rust + Bevy, everything built from code, no editor scenes.
- World canvas 960×540 (two canvas pixels per world pixel); the interface is laid out on a
  480×270 grid. Scaled by whole or half steps to the window; the window opens at the largest
  whole multiple that fits the screen.
- Top-down grid, 16×16 tiles, a world made of separate locations (about 30×20 cells) joined by
  exits. Building interiors are their own locations.
- Picture (user, after seeing the first version: "looks like the 90s"): stock grounds are
  painted in code at twice the master's measure (scene.rs) — ragged edges where grounds meet,
  wall faces and shadows, standing trees/bushes/rocks, animated water, a vignette. The master
  sets each place's light (day/dusk/night/dark); the dark is a layer with holes around lamps
  (props named like fire, torch…) and around every hero. Master-drawn tiles and sprites are
  enlarged with EPX, not redrawn.
- Heroes are not drawn by the master: doll.rs builds them from looks (chosen at creation, a tab
  of its own with a live walking figure) and gear — armour, helmet, shield and main weapon show
  on the body. Four directions × four walk frames.
- The master's own tiles are read by their names (cobble, pier, wall, water…, either language)
  and painted like the stock grounds; a reading that contradicts the tile's walkability is never
  taken. Blocking drawn tiles stand up with a face and shadow; Alt (and the first seconds in a
  place) shows a grid of open, blocked and exit cells.
- Buildings (walled areas reached only through a door) get roofs that fade while the hero is
  inside. Things by water are mirrored in it.
- Weather per place (clear, sun, rain, storm, snow, fog), set and changed by the master; rain
  rings water, storms flash, fog drifts, sun throws light shafts and cloud shadows.
- Graphics screen with a live preview: colour grade presets (clean, warm, cold, noir, sepia,
  vivid, dream, CRT) and each effect on/off (glow, vignette, scan lines, curved glass, weather,
  dark of night, cell grid). The interface is drawn after the effects. HDR camera; lamps glow.
- User, after a real game: "the narration says the gargoyles approach but nothing moves". The
  prompt now demands an effect for every visible change; new effects `mark` (blood, dung, ash…
  left on a cell) and `animate` (jump, crouch, strike, fall, dance…, numbered cues in the world
  so every player sees them once). Stunt texts may not declare who falls — the dice decide.
- Party frames (face, level, HP, AC) top right. Tellings fold away (H); long choices wrap.
- Bag: the party's shared grid (12×10), items sized by shape, gear wearable in four slots
  (main, spare, shield, body) with the doll and AC updating; guests' moves go through the host,
  each player changes only their own hero's gear.
- At a shared table the master names the acting hero in the third person; players elsewhere
  read the first sentence of each telling in the chat.
- After the second real game (user): the master must live in the world, not beside it. The
  master is now sent the map, what is reachable and how many steps away, each hero's gear and
  bag; it must walk the party up before touching things, start fights instead of narrating
  blows, and use travel to send heroes elsewhere. Choices continue the scene (outcome at once,
  then the master's next turn). Fight results go into the story and the facts.
- Each hero has its own bag (D2 style, right half of the screen, world playable on the left):
  swap-on-drop, placement highlight, right click to use, click outside to throw on the ground
  with an arc; things on the ground are picked up with E (how players trade). Items carry a
  kind, size, the master's own picture, healing, harm and light; torches go in the off hand,
  show on the doll and light far.
- Interactive world: chests (contents, lock, key; open, pick with Dex, force with Str), hidden
  things found by passive Perception, traps underfoot, bodies left by the dead (searchable,
  revivable), marks, animations, map changes (tile), spawned creatures, weather and hour.
- Exits are taken on purpose (E, or stepping off the map by the arrow); exit arrows and names
  are drawn; a new place always leads somewhere new.
- Music: ~28 hours of mood-tagged pieces (Kevin MacLeod CC BY 4.0, OpenGameArt CC0) fetched in
  the background into the user folder (1.5 GB cap), offline plays what is cached; the master
  may set a place's mood.
- Own provider: any OpenAI-compatible endpoint (OpenAI, OpenRouter, Ollama, LM Studio) instead
  of Gemini; key sealed with DPAPI.
- Builds: `.cargo/config.toml` links with rust-lld; an incremental build of the game is ~40 s.
  Changing Bevy features or the linker rebuilds everything (20–35 min).
- Platforms: Windows now. Bevy also builds for Android/iOS natively and for the web (wasm,
  WebGPU/WebGL2). The web build would need WebSocket/WebRTC instead of TCP and Steam, browser
  storage instead of files, and no DPAPI; Android needs touch controls and Android Keystore.
- Shell style: a pixel game-master table (wood, parchment, candles, dice, GM screen). The GM
  picks an accent palette per world and the shell is recoloured to match.
- Everything is animated in one style: procedural motion (tween, squash, flash, shake,
  particles, typewriter text, palette fades), plus two drawn frames for important creatures.
- Sound: free CC0 effects and chiptune, picked by mood tags. GM voice (TTS) is an option, off
  by default, built last, with the same key and model rotation.

## Gemini access

- Keys belong to players. The host can enter several; clients can enter theirs at any moment
  of the game. Keys are stored encrypted (Windows DPAPI).
- A client's key never leaves the client's machine: the host hands the request to that client,
  the client calls Gemini and returns the answer.
- No model id is written in code. The chain is rebuilt from the `v1` and `v1beta` model lists
  (on duplicates `v1beta` wins): flash newest to oldest, then flash-lite newest to oldest, then
  Gemma largest to smallest. Stable names go before previews. Image, TTS, live, audio,
  embedding and similar variants are excluded by name tokens.
- One order for every task, including sprites and translation: the user judged flash-lite too
  weak to be preferred for anything.
- Model lists differ between keys (projects), so the chain is the union of every key's lists;
  a key that lacks a model answers 404 for it and only that pair is put to rest.
- Spent quotas and each model's thinking dialect are saved (`quota.json`, keys named by a
  fingerprint) so a restart neither forgets them nor burns requests rediscovering them.
- Cooldowns are kept per (key, model) pair, because quotas are per model:
  - 429 → next key on the same model; the pair rests for the server's `retryDelay`.
  - Every key exhausted for a model → next model.
  - 500/503/timeout → the model rests; another key would not help.
  - 404 → drop the model and relist.
  - Key rejected → key marked dead.
- When the host's keys fail, clients' keys are used.
- When all flash models are spent: host setting — fall to lite then Gemma (default, shown in
  the UI) or pause. When everything is spent: pause screen with a countdown and a key field.
- A meter of what is left per key and model is always visible.
- Thinking: the host picks a level or turns it off. Old models take a token budget, new ones a
  level, some accept neither; found by trying and remembered per model.
- Gemma is the last resort; for translation and chronicle compression it is used only when
  flash-lite is unavailable.
- Measured free limit: 20 requests a day per model per project. A paid key is supported as a
  normal key; the menu says so in one line.

## Request thrift (follows from the 20-a-day limit)

- A GM answer carries narration, world changes and 3–4 suggested actions, each with its outcome
  already resolved. Picking a suggestion costs no request; only a player's own text does.
- One location is one request: map, new tiles, creature sprites, NPCs and stubs of the
  neighbouring locations.
- Neighbours exist immediately as stubs (name, description, exits, contents); they are drawn in
  the background by priority. Entering one that is not drawn yet shows a short loading screen.
- Combat by the book runs entirely in code, monster turns included. The GM is called at the
  start (setup, a pack of hit and miss lines), at the end, and for improvised actions.
- Player actions are queued and sent in batches, one request at a time per scene.
- GM initiative (events when players are idle) exists, is rare, and its frequency is a host
  setting.
- Translation between players' languages is a separate request, by the key of the client who
  needs it when possible, cached. Names of skills and items are translated once.

## Game master

- The GM seat is taken by the AI or by a human, and can change hands at any moment. A human GM
  can be the host or a client.
- The GM does not hold the rules: code owns dice, HP, inventory and legality. The GM answers
  with JSON (plain JSON in text, so every tier of the chain can produce it); code validates and
  retries on garbage.
- The GM edits a scene with patch operations (set tile, spawn, remove, redraw sprite) instead
  of regenerating it, and may create or change other locations and exits.
- Human GM: writes directives the model executes; sees the model's draft before players and
  accepts, edits or rejects it; has direct tools (place tile, spawn, edit stats, hidden roll,
  private message); can ask the model for anything (for example a new monster) and keeps the
  result in a personal library on their machine, from which it is copied into a world.
- Private messages to one player exist for both kinds of GM.

## Rules

- D&D 5e SRD 5.1 by the book: all classes to level 20, spells, monsters, equipment, feats.
  Multiclassing comes after everything else. SRD 5.2 is offered as a beta ruleset.
- Data: `data/srd` (from 5e-bits/5e-database), with a Russian translation memory.
- Two modes per world: "book only", and mixed, where the GM adds and invents on top.
- Setting is anything; the GM decides when the host leaves it blank. In a non-fantasy world the
  GM renames and redescribes the SRD classes (same mechanics) and may add its own; in book-only
  mode they stay as written.
- A character has a name and a free description that the GM uses for story hooks.
- Ability scores: standard array, point buy or 4d6, chosen by the host (point buy by default).
- Invented abilities, spells and items are assembled from primitives the engine knows (damage,
  heal, condition, move, summon, buff, change tiles; range, area, cost, cooldown) and checked
  against a power budget for the level. What does not fit is a story effect resolved by a GM
  check. SRD spells that cannot be mechanised are resolved the same way.
- Level-up in mixed mode: book features always, plus 2–3 invented choices on every second
  level, drawn from the character's history. XP for enemies is counted by code; story XP comes
  from the GM within a per-level cap.
- Secret skills from unusual deeds are rare; code holds the counter and the chance.
- Caps on item power, gold and damage per level are on by default; the host can switch to
  "no limits".
- Death: death saves as in 5e. A dead character returns through the story or is replaced by a
  new level 1 character. A dead party continues in the story (captivity, afterlife) or loads a
  save, host's choice.
- Dice: an animated d20; players roll their own checks by pressing; enemies and routine rolls
  are automatic; the GM has hidden rolls.

## Play flow

- Outside combat: free real-time walking; an interact key opens a prompt with the GM's
  suggestions; free text any time. No canned action buttons.
- In combat: turn-based on the grid; the player picks from their own sheet (attacks, spells,
  items) or writes an improvised action.
- Combat starts when a hostile creature notices the party (radius checked by code) or when the
  GM declares it.
- The party may split: each scene has its own clock and GM queue, at most one active scene per
  player. Requests for a scene use the keys of the players in it first. The GM reconciles time
  when scenes meet.
- Large creatures occupy several cells.
- Two chat channels: action (goes to the GM) and table talk (the GM does not see it).

## Memory

- Structured state (sheets, inventory, skills, world map, NPCs, quests) is never summarised and
  is always sent whole.
- The story log is compressed at 80% of a working token budget (configurable, well below the
  model window so smaller models still fit): a detailed chronicle by chapters plus an
  append-only list of world facts.

## Saves and sessions

- Autosave after every GM answer plus manual slots.
- The save is distributed to every player. A client can continue alone from their copy and
  plays their own character; branches never merge back.
- Returning players get their character automatically; a new player may take a free one;
  characters whose players are absent disappear from play.
- Joining mid-game is allowed; the GM writes the character in. On disconnect the character
  follows passively and is not targeted until the player returns.

## Network

- Host-authoritative from day one, single player included.
- Steam lobbies and P2P (Spacewar app id during development) plus direct IP under "advanced".
  The same local transport runs several instances on one machine for automated tests.
- Steam P2P between two accounts cannot be tested on one machine; it stays marked unverified
  until one run with a second account.

## Testing

- The author has nobody to play with: correctness is checked by automated tests, a scripted
  fake GM, several local instances, and screenshots taken from the game.
- A "collect report" button packs the log and the save.
- The game takes developer flags (`crates/game/src/shot.rs`): `--do` plays a script of clicks,
  typing and screenshots through the real input path, `--hidden true` keeps the window off the
  desktop, and `PIXELGM_HOME` gives each instance its own settings folder.

## Build order

1. Shell: window, font, menu, settings, key manager, model rotation.
2. World: location and sprite generation, walking, exits, saves.
3. GM: narration, actions, NPCs, queue, memory and compression.
4. Rules 5.1: character, checks, combat, spells, monsters, level-up.
5. Co-op: lobby, sync, split party, save distribution.
6. Human GM: panel, drafts, library.
7. Polish: sound, animation, translation, 5.2 beta, multiclass, voice.

## Second arc: a game worth staying in (agreed 2026-10-04)

The first build worked but was dull: nothing to do between the master's answers, no goal,
a world that did not answer back, and only plain choices. What follows was settled question
by question with the owner.

### Modes, chosen when a world is made

- **Story mode** (on by default): the master writes a main quest, three to five acts hidden
  from the player, and an ending. The goal is on screen and in a quest journal. The story can
  be lost — a bad ending is an ending — and then a new chapter starts with the same hero.
  Off is the first build's sandbox.
- **Live mode** (a switch): the master answers in batches and works in the background —
  people speak and walk, things happen while the party stands, companions comment, a scene
  is redrawn when it changes, hidden threat clocks run. Speech goes to flash-lite and Gemma.
- **Difficulty**: casual, honest (default), hardcore.

  |                  | Casual          | Honest            | Hardcore            |
  |------------------|-----------------|-------------------|---------------------|
  | Defeat           | wake with 1 hp  | a hero may die    | death is final      |
  | Saves            | free            | free              | one, automatic      |
  | Encounter budget | x0.75           | x1                | x1.25               |
  | Enemy hit points | numbers         | a bar             | a bar               |
  | Tiredness        | no              | no                | yes                 |
  | Ration to rest   | no              | yes               | yes                 |

- A provider of one's own, when switched on in the settings, is the whole master in place of
  Gemini.

### Interface

- A frame in the manner of Baldur's Gate: the map is never covered. Party portraits in a
  column on the right, the telling in a window below with three heights and the choices under
  it, then six quick slots and the buttons.
- Choices on 1–5, quick slots on F1–F6; in a fight the digits are the fight's actions.
- Text is typed fast and a click shows it all; the voice is off by default.
- The camera follows the hero; the wheel zooms from about 15×8 cells to the whole map, about
  20×11 by default. Each player has their own camera. A map of the place on M.
- In a fight: a strip of who acts when, and a bar over each enemy.

### World and time

- Places up to 60×32: a strong model draws one in a single request, a weak one in four
  pieces that each see the plan of the whole and the pieces already drawn.
- The clock always runs, a minute of the game to a second; a fight counts rounds of six
  seconds. It stops only on Esc alone, and alone when the window is not in front.
- Night is darker and more dangerous; people sleep, shops are shut. Tiredness only in hardcore.
- The master gives each place a danger of 0–5. Wandering encounters are rolled once a game
  hour (danger × 5% by day, × 10% by night); a third are not fights and come from lines the
  master wrote with the place. Where the master set encounters of its own, or said there are
  none, the game rolls none.
- Levers, plates, locks and hidden ways work by the rules, and show to a keen eye.

### Fights

- Enemies come in groups: the game counts them from a budget; the master, model or person,
  may also set groups itself.
- The bag opens in a fight: one free handling of a thing each turn, the rest costs the action.

### Things and money

- Gold is a count each hero carries, thrown down as a heap.
- Shops as in Diablo 2: the master draws the wares and what they do; the stock turns over
  once a day without a request; one haggle a visit; standing moves prices by a fifth.
- Rarity and properties come mostly from the master, held to the party's level by the game;
  properties rolled by the game come later.
- Level 20 at most; the main quest takes a hero to about 5–7.
- Short and long rests; a long one needs a safe place and a ration, and may be broken at night.

### Companions

- Someone joins by being talked round, hired, or won by a quest. Two at most alone; five
  fighters in all at a shared table.
- Out of a fight a companion walks alongside; in one, whoever won them over plays them. A bag
  of 4×4, gear can be changed. A monster's numbers, no levels, death is final.
- Alone, with the hero dead, the player plays the companion until the hero is raised.

### A world that answers

- Three to five factions from the master, and the people who matter, each with a standing
  shown once met.
- Threads: the master notes a deed that will come back; the game makes it bring one back
  every five to eight turns or on entering a place. The player does not see them.
- Threat clocks are heard of as rumours in the journal.
- Speech bubbles: six to eight lines for a place in one request, one every 20–40 seconds; a
  click starts a talk.
- Choices: up to five, at least one from a skill or a thing the hero has, and one risky with
  its chance shown.

### The person in the master's seat

A group of enemies by a click, a brush for cells, a bubble spoken for someone, things and
gold handed out, the clock and the sky, and "draw this for me" by the model.

### Order

0. First the faults: the game checks the master's answer and asks again when the telling
   names a thing no effect hands over; notices and the acting player's own view for prepared
   actions and for stunts in a fight.
1. The frame and the camera.
2. Story mode: quest, acts, goal, journal.
3. Groups, encounter budget, the strip of turns, quick slots, the bag in a fight.
4. Time, night, rest, gold, shops, rarity.
5. Companions.
6. Standing, threads, threat clocks, wandering encounters.
7. Live mode: bubbles, people about their business, batched answers.
8. Large places.
9. Mechanisms and puzzles.
10. The master's tools.

Along the way: "draw this place again" for the host, the name and picture of a worn thing of
the master's, Steam with two accounts.

### Where the second arc stands (2026-10-04)

Built and checked in hidden instances with the rule-based master, and once with real
Gemini (a world with quest, factions, a shop, danger and wanderers; a turn that gave a
thing, moved the quest and started a threat, with no faults found):

0. Answer checks with one more request; notices and the acting player's view.
1. The frame, the camera, zoom, the overview on M, numbered choices.
2. Story mode: quest, acts, goal line, tasks, journal; the modes on the new-world form.
3. Packs by budget or by the master's count, the strip of turns, quick slots, the bag in a
   fight with one free handling.
4. The clock, night, rest with rations, gold, shops, price / rarity / bonus.
5. Companions: join, dismiss, follow, fight under the player's hand.
6. Factions and standing, threads, threats, danger and wandering encounters.
7. Live world: bubbles from the cheap models, strolling, more frequent events.
8. Large places, 60 by 32, offered to the best models only.
9. Locks that want keys, plates; the master is told how to build mechanisms.
10. The master's seat: counted packs, gold, hours, sky, a spoken bubble, a brush.

Also: the host can have a place drawn again; a worn thing of the master's keeps its name
and picture; casual and hardcore change defeat, saving and what is shown of enemies.

Not done, on purpose or for lack of a way to check:

- A companion has no bag and its gear cannot be changed; a lone player whose hero is dead
  does not go on as the companion outside a fight.
- A weak model is never asked for a large place, so there are no four-piece maps.
- Tiredness on hardcore.
- Steam with two accounts was never tried: it needs two accounts.
