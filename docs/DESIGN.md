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
- Internal resolution 480×270, integer upscaling.
- Top-down grid, 16×16 tiles, a world made of separate locations (about 30×20 cells) joined by
  exits. Building interiors are their own locations.
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
