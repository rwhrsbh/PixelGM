//! Every effect the model can write, as a form the human master fills in: the op is picked
//! from a list, its fields are asked one by one with the things of the map to choose from,
//! and the result is the very JSON the model would have written. So whatever the model can
//! do to the world, the master can too.

use crate::i18n::{Lang, T};
use crate::ui::egui::{self, Ui};
use crate::ui::kit::{button, hint};
use serde_json::{Value, json};
use world::World;

/// What a field of an effect holds, which says how it is asked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Text,
    Long,
    Number,
    /// Dice like 2d6+1, or a plain number.
    Dice,
    /// A creature or a thing of this place, by id.
    Thing,
    /// A creature, a thing, a hero or the whole party.
    Someone,
    Hero,
    /// Some of the heroes, ticked one by one.
    Heroes,
    /// The ground under a cell: x and y.
    Cell,
    Tile,
    Bool,
    Place,
    /// One of the words listed after the field's name.
    Pick(&'static [&'static str]),
    /// A list of words, one a line.
    Lines,
    /// Steps of their own, one under another: what an "if", a "repeat" or a script does.
    Steps,
}

pub struct Field {
    pub key: &'static str,
    pub label: T,
    pub kind: Kind,
}

const fn f(key: &'static str, label: T, kind: Kind) -> Field {
    Field { key, label, kind }
}

pub struct Op {
    pub op: &'static str,
    pub label: T,
    /// A group of the list: what the effect is about.
    pub group: usize,
    pub fields: &'static [Field],
}

const MOTIONS: &[&str] = &["jump", "crouch", "sit", "lie", "fall", "shake", "spin", "strike", "dance", "wave", "nod", "bow", "cower", "stagger"];
const KINDS: &[&str] = &["weapon", "armor", "shield", "light", "potion", "food", "throwable", "key", "tool", "junk", "quest"];
const MARKS: &[&str] = &["blood", "dung", "puddle", "scorch", "ash", "slime", "vomit", "wine", "paint", "rubble", "coins", "bones"];
const MOODS: &[&str] = &["calm", "explore", "tense", "battle", "boss", "mystery", "horror", "sad", "tavern", "triumph", "dungeon"];
const ATTITUDES: &[&str] = &["hostile", "neutral", "friendly"];
const SECRETS: &[&str] = &["hidden", "trap", "plate"];
const LIGHTS: &[&str] = &["day", "dusk", "night", "dark"];
const WEATHERS: &[&str] = &["clear", "sun", "rain", "storm", "snow", "fog"];
const ABILITIES: &[&str] = &["str", "dex", "con", "int", "wis", "cha"];
const DONE: &[&str] = &["", "won", "lost"];
const ORDER: &[&str] = &["in_turn", "random"];
const TRIGGERS: &[&str] = &["", "enter", "death", "hour", "var"];

/// The groups of the list, in order.
pub const GROUPS: [T; 7] = [T::FxGroupParty, T::FxGroupThings, T::FxGroupMap, T::FxGroupWorld, T::FxGroupStory, T::FxGroupSights, T::FxGroupScripts];

pub const OPS: &[Op] = &[
    Op { op: "damage", label: T::FxDamage, group: 0, fields: &[f("amount", T::FxDamageAmount, Kind::Dice), f("id", T::FxDamageId, Kind::Someone)] },
    Op { op: "heal", label: T::FxHeal, group: 0, fields: &[f("amount", T::FxHealAmount, Kind::Dice), f("id", T::FxHealId, Kind::Someone)] },
    Op { op: "give", label: T::FxGive, group: 0, fields: &[f("name", T::FxGiveName, Kind::Text), f("description", T::FxGiveDescription, Kind::Text), f("kind", T::FxGiveKind, Kind::Pick(KINDS)), f("heal", T::FxGiveHeal, Kind::Dice), f("damage", T::FxGiveDamage, Kind::Dice), f("to", T::FxGiveTo, Kind::Hero)] },
    Op { op: "stash", label: T::FxStash, group: 1, fields: &[f("id", T::FxStashId, Kind::Thing), f("name", T::FxStashName, Kind::Text), f("description", T::FxStashDescription, Kind::Text), f("kind", T::FxStashKind, Kind::Pick(KINDS)), f("heal", T::FxStashHeal, Kind::Dice), f("damage", T::FxStashDamage, Kind::Dice), f("price", T::FxStashPrice, Kind::Number)] },
    Op { op: "rest", label: T::FxRest, group: 0, fields: &[f("kind", T::FxRestKind, Kind::Pick(&["long", "short"]))] },
    Op { op: "peace", label: T::FxPeace, group: 1, fields: &[] },
    Op { op: "drop", label: T::FxDrop, group: 2, fields: &[f("cell", T::FxDropCell, Kind::Cell), f("name", T::FxDropName, Kind::Text), f("description", T::FxDropDescription, Kind::Text), f("kind", T::FxDropKind, Kind::Pick(KINDS)), f("heal", T::FxDropHeal, Kind::Dice), f("damage", T::FxDropDamage, Kind::Dice), f("price", T::FxDropPrice, Kind::Number)] },
    Op { op: "hand", label: T::FxHand, group: 0, fields: &[f("name", T::FxHandName, Kind::Text), f("to", T::FxHandTo, Kind::Hero)] },
    Op { op: "take", label: T::FxTake, group: 0, fields: &[f("name", T::FxTakeName, Kind::Text)] },
    Op { op: "gold", label: T::FxGold, group: 0, fields: &[f("amount", T::FxGoldAmount, Kind::Number), f("to", T::FxGoldTo, Kind::Hero)] },
    Op { op: "xp", label: T::FxXp, group: 0, fields: &[f("amount", T::FxXpAmount, Kind::Number)] },
    Op { op: "check", label: T::FxCheck, group: 0, fields: &[f("ability", T::FxCheckAbility, Kind::Pick(ABILITIES)), f("skill", T::FxCheckSkill, Kind::Text), f("dc", T::FxCheckDc, Kind::Number), f("success.narration", T::FxCheckSuccessNarration, Kind::Long), f("failure.narration", T::FxCheckFailureNarration, Kind::Long)] },
    Op { op: "deed", label: T::FxDeed, group: 0, fields: &[f("hero", T::FxDeedHero, Kind::Hero)] },
    Op { op: "revive", label: T::FxRevive, group: 0, fields: &[f("hero", T::FxReviveHero, Kind::Hero)] },
    Op { op: "move", label: T::FxMove, group: 1, fields: &[f("id", T::FxMoveId, Kind::Someone), f("cell", T::FxMoveCell, Kind::Cell)] },
    Op { op: "spawn", label: T::FxSpawn, group: 1, fields: &[f("kind", T::FxSpawnKind, Kind::Pick(&["creature", "prop"])), f("id", T::FxSpawnId, Kind::Text), f("name", T::FxSpawnName, Kind::Text), f("description", T::FxSpawnDescription, Kind::Text), f("cell", T::FxSpawnCell, Kind::Cell), f("sprite", T::FxSpawnSprite, Kind::Text), f("monster", T::FxSpawnMonster, Kind::Text), f("attitude", T::FxSpawnAttitude, Kind::Pick(ATTITUDES)), f("size", T::FxSpawnSize, Kind::Number), f("blocking", T::FxSpawnBlocking, Kind::Bool)] },
    Op { op: "remove", label: T::FxRemove, group: 1, fields: &[f("id", T::FxRemoveId, Kind::Thing)] },
    Op { op: "kill", label: T::FxKill, group: 1, fields: &[f("id", T::FxKillId, Kind::Thing)] },
    Op { op: "attitude", label: T::FxAttitude, group: 1, fields: &[f("id", T::FxAttitudeId, Kind::Thing), f("attitude", T::FxAttitudeAttitude, Kind::Pick(ATTITUDES))] },
    Op { op: "fight", label: T::FxFight, group: 1, fields: &[f("with", T::FxFightWith, Kind::Thing)] },
    Op { op: "describe", label: T::FxDescribe, group: 1, fields: &[f("id", T::FxDescribeId, Kind::Thing), f("description", T::FxDescribeDescription, Kind::Long)] },
    Op { op: "join", label: T::FxJoin, group: 1, fields: &[f("id", T::FxJoinId, Kind::Thing)] },
    Op { op: "dismiss", label: T::FxDismiss, group: 1, fields: &[f("id", T::FxDismissId, Kind::Thing)] },
    Op { op: "edit", label: T::FxEdit, group: 1, fields: &[f("id", T::FxEditId, Kind::Thing), f("name", T::FxEditName, Kind::Text), f("size", T::FxEditSize, Kind::Number), f("ac", T::FxEditAc, Kind::Number), f("hp", T::FxEditHp, Kind::Number), f("speed", T::FxEditSpeed, Kind::Number), f("xp", T::FxEditXp, Kind::Number), f("monster", T::FxEditMonster, Kind::Text), f("blocking", T::FxEditBlocking, Kind::Bool), f("sprite", T::FxEditSprite, Kind::Text)] },
    Op { op: "action", label: T::FxAction, group: 6, fields: &[f("id", T::FxActionId, Kind::Thing), f("label", T::FxActionLabel, Kind::Text), f("outcome", T::FxActionOutcome, Kind::Long), f("needs", T::FxActionNeeds, Kind::Text)] },
    Op { op: "lines", label: T::FxLines, group: 1, fields: &[f("id", T::FxLinesId, Kind::Thing), f("lines", T::FxLinesLines, Kind::Lines), f("every", T::FxLinesEvery, Kind::Number), f("order", T::FxLinesOrder, Kind::Pick(ORDER))] },
    Op { op: "tile", label: T::FxTile, group: 2, fields: &[f("cell", T::FxTileCell, Kind::Cell), f("tile", T::FxTileTile, Kind::Tile)] },
    Op { op: "mark", label: T::FxMark, group: 2, fields: &[f("cell", T::FxMarkCell, Kind::Cell), f("kind", T::FxMarkKind, Kind::Pick(MARKS))] },
    Op { op: "clean", label: T::FxClean, group: 2, fields: &[f("cell", T::FxCleanCell, Kind::Cell)] },
    Op { op: "secret", label: T::FxSecret, group: 2, fields: &[f("cell", T::FxSecretCell, Kind::Cell), f("kind", T::FxSecretKind, Kind::Pick(SECRETS)), f("dc", T::FxSecretDc, Kind::Number), f("note", T::FxSecretNote, Kind::Text)] },
    Op { op: "exit", label: T::FxExit, group: 2, fields: &[f("cell", T::FxExitCell, Kind::Cell), f("to", T::FxExitTo, Kind::Place), f("label", T::FxExitLabel, Kind::Text)] },
    Op { op: "travel", label: T::FxTravel, group: 2, fields: &[f("to", T::FxTravelTo, Kind::Place), f("name", T::FxTravelName, Kind::Text), f("description", T::FxTravelDescription, Kind::Long)] },
    Op { op: "time", label: T::FxTime, group: 3, fields: &[f("hours", T::FxTimeHours, Kind::Number)] },
    Op { op: "light", label: T::FxLight, group: 3, fields: &[f("light", T::FxLightLight, Kind::Pick(LIGHTS))] },
    Op { op: "weather", label: T::FxWeather, group: 3, fields: &[f("weather", T::FxWeatherWeather, Kind::Pick(WEATHERS))] },
    Op { op: "music", label: T::FxMusic, group: 3, fields: &[f("mood", T::FxMusicMood, Kind::Pick(MOODS))] },
    Op { op: "fact", label: T::FxFact, group: 4, fields: &[f("text", T::FxFactText, Kind::Long)] },
    Op { op: "quest", label: T::FxQuest, group: 4, fields: &[f("goal", T::FxQuestGoal, Kind::Text), f("advance", T::FxQuestAdvance, Kind::Bool), f("done", T::FxQuestDone, Kind::Pick(DONE))] },
    Op { op: "quest_new", label: T::FxQuestNew, group: 4, fields: &[f("title", T::FxQuestNewTitle, Kind::Text), f("goal", T::FxQuestNewGoal, Kind::Text), f("acts", T::FxQuestNewActs, Kind::Lines), f("ending", T::FxQuestNewEnding, Kind::Text), f("failure", T::FxQuestNewFailure, Kind::Text)] },
    Op { op: "task", label: T::FxTask, group: 4, fields: &[f("name", T::FxTaskName, Kind::Text), f("step", T::FxTaskStep, Kind::Text), f("done", T::FxTaskDone, Kind::Bool)] },
    Op { op: "faction", label: T::FxFaction, group: 4, fields: &[f("id", T::FxFactionId, Kind::Text), f("name", T::FxFactionName, Kind::Text), f("description", T::FxFactionDescription, Kind::Text), f("standing", T::FxFactionStanding, Kind::Number)] },
    Op { op: "standing", label: T::FxStanding, group: 4, fields: &[f("id", T::FxStandingId, Kind::Text), f("change", T::FxStandingChange, Kind::Number)] },
    Op { op: "thread", label: T::FxThread, group: 4, fields: &[f("text", T::FxThreadText, Kind::Long)] },
    Op { op: "thread_done", label: T::FxThreadDone, group: 4, fields: &[f("text", T::FxThreadDoneText, Kind::Text)] },
    Op { op: "threat", label: T::FxThreat, group: 4, fields: &[f("name", T::FxThreatName, Kind::Text), f("what", T::FxThreatWhat, Kind::Long), f("rumour", T::FxThreatRumour, Kind::Text), f("hours", T::FxThreatHours, Kind::Number)] },
    Op { op: "threat_end", label: T::FxThreatEnd, group: 4, fields: &[f("name", T::FxThreatEndName, Kind::Text)] },
    Op { op: "shake", label: T::FxShake, group: 5, fields: &[f("seconds", T::FxShakeSeconds, Kind::Number), f("who", T::FxShakeWho, Kind::Heroes), f("cell", T::FxShakeCell, Kind::Cell), f("radius", T::FxShakeRadius, Kind::Number)] },
    Op { op: "animate", label: T::FxAnimate, group: 5, fields: &[f("id", T::FxAnimateId, Kind::Someone), f("motion", T::FxAnimateMotion, Kind::Pick(MOTIONS)), f("who", T::FxAnimateWho, Kind::Heroes), f("cell", T::FxAnimateCell, Kind::Cell), f("radius", T::FxAnimateRadius, Kind::Number)] },
    Op { op: "say", label: T::FxSay, group: 5, fields: &[f("id", T::FxSayId, Kind::Someone), f("text", T::FxSayText, Kind::Text), f("who", T::FxSayWho, Kind::Heroes), f("cell", T::FxSayCell, Kind::Cell), f("radius", T::FxSayRadius, Kind::Number)] },
    Op { op: "wait", label: T::FxWait, group: 6, fields: &[f("seconds", T::FxWaitSeconds, Kind::Number)] },
    Op { op: "set", label: T::FxSet, group: 6, fields: &[f("var", T::FxSetVar, Kind::Text), f("value", T::FxSetValue, Kind::Number)] },
    Op { op: "add", label: T::FxAdd, group: 6, fields: &[f("var", T::FxAddVar, Kind::Text), f("value", T::FxAddValue, Kind::Number)] },
    Op { op: "script", label: T::FxScript, group: 6, fields: &[f("id", T::FxScriptId, Kind::Text)] },
    Op { op: "if", label: T::FxIf, group: 6, fields: &[f("var", T::FxIfVar, Kind::Text), f("is", T::FxIfIs, Kind::Number), f("at_least", T::FxIfAtLeast, Kind::Number), f("then", T::FxIfThen, Kind::Steps), f("else", T::FxIfElse, Kind::Steps)] },
    Op { op: "random", label: T::FxRandom, group: 6, fields: &[f("choices.0", T::FxRandomChoices0, Kind::Steps), f("choices.1", T::FxRandomChoices1, Kind::Steps), f("choices.2", T::FxRandomChoices2, Kind::Steps)] },
    Op { op: "repeat", label: T::FxRepeat, group: 6, fields: &[f("times", T::FxRepeatTimes, Kind::Number), f("effects", T::FxRepeatEffects, Kind::Steps)] },
    Op { op: "define", label: T::FxDefine, group: 6, fields: &[f("id", T::FxDefineId, Kind::Text), f("on", T::FxDefineOn, Kind::Pick(TRIGGERS)), f("place", T::FxDefinePlace, Kind::Place), f("of", T::FxDefineOf, Kind::Thing), f("var", T::FxDefineVar, Kind::Text), f("at", T::FxDefineAt, Kind::Number), f("effects", T::FxDefineEffects, Kind::Steps)] },
];

/// The op's entry, when the game knows it.
pub fn op(name: &str) -> Option<&'static Op> {
    OPS.iter().find(|o| o.op == name)
}

/// An effect in a line a master can read: its name and what it is set to.
pub fn describe(effect: &Value, world: &World, lang: Lang) -> String {
    let name = effect["op"].as_str().unwrap_or("?");
    let Some(op) = op(name) else { return effect.to_string() };
    let shown = |v: &Value| match v {
        Value::String(s) => world.here().creatures.iter().find(|c| c.id == *s).map(|c| c.name.clone()).or_else(|| world.here().props.iter().find(|p| p.id == *s).map(|p| p.name.clone())).unwrap_or_else(|| s.clone()),
        Value::Array(list) => list.iter().map(|v| v.as_str().map_or(v.to_string(), str::to_string)).collect::<Vec<_>>().join(", "),
        other => other.to_string(),
    };
    let mut parts = Vec::new();
    for field in op.fields {
        let value = match field.kind {
            Kind::Cell => match (effect.get("x"), effect.get("y")) {
                (Some(x), Some(y)) => Value::String(format!("({x}, {y})")),
                _ => Value::Null,
            },
            _ => pointer(effect, field.key).cloned().unwrap_or(Value::Null),
        };
        if value.is_null() || value == json!("") || value == json!([]) {
            continue;
        }
        let text: String = shown(&value).chars().take(40).collect();
        parts.push(text);
    }
    format!("{}: {}", op.label.text(lang), parts.join(" · "))
}

fn pointer<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.pointer(&format!("/{}", key.replace('.', "/")))
}

/// The form being filled: which op, and the text of each field.
#[derive(Default, Clone)]
pub struct Form {
    pub op: usize,
    pub values: std::collections::HashMap<&'static str, String>,
    pub open: bool,
    /// Only the fields: the effect is chosen by the caller, who also has the buttons.
    pub fixed: bool,
    /// The forms of the steps inside, by field.
    pub subs: std::collections::HashMap<&'static str, Form>,
}

/// The form for one effect; returns the effect once "Add" is pressed.
pub fn builder(ui: &mut Ui, form: &mut Form, world: &World, lang: Lang) -> Option<Value> {
    if !form.open {
        if ui.add(button(T::FxAddAChange.text(lang))).clicked() {
            form.open = true;
        }
        return None;
    }
    let mut made = None;
    crate::ui::egui::Frame::new().fill(crate::ui::theme::c(crate::theme::PARCHMENT_SHADE)).inner_margin(6.0).show(ui, |ui| {
        let current = &OPS[form.op.min(OPS.len() - 1)];
        let name = |o: &Op| o.label.text(lang);
        if !form.fixed {
        egui::ComboBox::from_id_salt(("effect_op", ui.id())).width(260.0).height(320.0).selected_text(name(current)).show_ui(ui, |ui| {
            for (g, group) in GROUPS.iter().enumerate() {
                ui.label(egui::RichText::new(group.text(lang)).strong());
                for (n, o) in OPS.iter().enumerate().filter(|(_, o)| o.group == g) {
                    if ui.selectable_label(form.op == n, format!("   {}", name(o))).clicked() {
                        form.op = n;
                        form.values.clear();
                    }
                }
            }
        });
        }
        let current = &OPS[form.op.min(OPS.len() - 1)];
        let here = world.here();
        let things: Vec<(String, String)> = here.creatures.iter().map(|c| (c.id.clone(), c.name.clone())).chain(here.props.iter().map(|p| (p.id.clone(), p.name.clone()))).collect();
        let heroes: Vec<String> = world.party.iter().map(|h| h.name.clone()).collect();
        let (px, py) = world.player.cell();
        for field in current.fields {
            let value = form.values.entry(field.key).or_insert_with(|| match field.kind {
                Kind::Cell => format!("{}, {}", px.max(0), py.max(0)),
                Kind::Pick(list) => list.first().map_or(String::new(), |s| s.to_string()),
                Kind::Number => String::new(),
                _ => String::new(),
            });
            ui.horizontal(|row| {
                row.add_sized([130.0, 18.0], egui::Label::new(field.label.text(lang)).wrap());
                match field.kind {
                    Kind::Thing | Kind::Someone => {
                        let shown = things.iter().find(|(id, _)| id == value).map_or(value.clone(), |(_, n)| n.clone());
                        egui::ComboBox::from_id_salt(("thing", field.key)).width(150.0).selected_text(if shown.is_empty() { "—".to_string() } else { shown }).show_ui(row, |ui| {
                            if field.kind == Kind::Someone {
                                ui.selectable_value(value, String::new(), T::FxTheParty.text(lang));
                                for hero in &heroes {
                                    ui.selectable_value(value, hero.clone(), hero.as_str());
                                }
                            }
                            for (id, name) in &things {
                                ui.selectable_value(value, id.clone(), format!("{name} ({id})"));
                            }
                        });
                    }
                    Kind::Hero => {
                        egui::ComboBox::from_id_salt(("hero", field.key)).width(150.0).selected_text(if value.is_empty() { "—".to_string() } else { value.clone() }).show_ui(row, |ui| {
                            ui.selectable_value(value, String::new(), "—");
                            for hero in &heroes {
                                ui.selectable_value(value, hero.clone(), hero.as_str());
                            }
                        });
                    }
                    Kind::Heroes => {
                        let mut ticked: Vec<String> = value.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
                        row.vertical(|col| {
                            for hero in &heroes {
                                let mut on = ticked.contains(hero);
                                if col.checkbox(&mut on, hero.as_str()).changed() {
                                    if on {
                                        ticked.push(hero.clone());
                                    } else {
                                        ticked.retain(|h| h != hero);
                                    }
                                }
                            }
                        });
                        *value = ticked.join(", ");
                    }
                    Kind::Tile => {
                        let mut tiles: Vec<(&String, &String)> = world.tiles.iter().map(|(id, t)| (id, &t.name)).collect();
                        tiles.sort_by(|a, b| a.1.cmp(b.1));
                        let shown = world.tiles.get(value.as_str()).map_or(value.clone(), |t| t.name.clone());
                        egui::ComboBox::from_id_salt(("tile", field.key)).width(150.0).height(260.0).selected_text(shown).show_ui(row, |ui| {
                            for (id, name) in tiles {
                                ui.selectable_value(value, id.clone(), name.as_str());
                            }
                        });
                    }
                    Kind::Place => {
                        let shown = world.locations.get(value.as_str()).map_or(value.clone(), |l| l.name.clone());
                        egui::ComboBox::from_id_salt(("place", field.key)).width(150.0).selected_text(shown).show_ui(row, |ui| {
                            for (id, place) in &world.locations {
                                ui.selectable_value(value, id.clone(), place.name.as_str());
                            }
                        });
                        row.add(egui::TextEdit::singleline(value).desired_width(80.0).hint_text(T::FxOrANewId.text(lang)));
                    }
                    Kind::Pick(list) => {
                        egui::ComboBox::from_id_salt(("pick", field.key)).width(150.0).selected_text(if value.is_empty() { "—".to_string() } else { value.clone() }).show_ui(row, |ui| {
                            for word in list {
                                ui.selectable_value(value, word.to_string(), if word.is_empty() { "—" } else { word });
                            }
                        });
                    }
                    Kind::Bool => {
                        let mut on = value == "true";
                        if row.checkbox(&mut on, "").changed() {
                            *value = on.to_string();
                        }
                    }
                    Kind::Long | Kind::Lines => {
                        row.add(egui::TextEdit::multiline(value).desired_rows(2).desired_width(200.0));
                    }
                    Kind::Cell => {
                        row.add(egui::TextEdit::singleline(value).desired_width(70.0).hint_text("x, y"));
                        hint(row, T::FxNowWhereTheHero.text(lang));
                    }
                    Kind::Steps => {
                        let mut list: Vec<Value> = serde_json::from_str(value).unwrap_or_default();
                        let sub = form.subs.entry(field.key).or_default();
                        row.vertical(|col| {
                            egui::CollapsingHeader::new(format!("{} ({})", T::FxSteps.text(lang), list.len())).id_salt(("steps", field.key, col.id())).show(col, |col| {
                                let mut words = String::new();
                                steps_only(col, &mut list, sub, &mut words, world, lang);
                            });
                        });
                        *value = serde_json::to_string(&list).unwrap_or_default();
                    }
                    _ => {
                        row.add(egui::TextEdit::singleline(value).desired_width(200.0));
                    }
                }
            });
        }
        if form.fixed {
            return;
        }
        ui.horizontal(|row| {
            if row.add(button(T::FxAdd2.text(lang)).primary()).clicked() {
                made = Some(build(current, &form.values));
                form.open = false;
                form.values.clear();
            }
            if row.add(button(T::FxCancel.text(lang))).clicked() {
                form.open = false;
            }
        });
    });
    made
}

/// The effect the form makes, as the model would have written it.
pub fn build(op: &Op, values: &std::collections::HashMap<&'static str, String>) -> Value {
    let mut effect = json!({ "op": op.op });
    for field in op.fields {
        let Some(text) = values.get(field.key).map(|s| s.trim()) else { continue };
        if text.is_empty() {
            continue;
        }
        let value = match field.kind {
            Kind::Number => text.replace(',', ".").parse::<f64>().map(|n| if n.fract() == 0.0 { json!(n as i64) } else { json!(n) }).unwrap_or(json!(text)),
            Kind::Bool => json!(text == "true"),
            Kind::Heroes => json!(text.split(',').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>()),
            Kind::Lines => json!(text.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>()),
            Kind::Steps => {
                let list: Vec<Value> = serde_json::from_str(text).unwrap_or_default();
                if list.is_empty() && !field.key.starts_with("choices.") {
                    continue;
                }
                json!(list)
            }
            Kind::Cell => {
                let numbers: Vec<i64> = text.split([',', ' ', ';']).filter_map(|s| s.trim().parse().ok()).collect();
                if let [x, y, ..] = numbers[..] {
                    effect["x"] = json!(x);
                    effect["y"] = json!(y);
                }
                continue;
            }
            _ => json!(text),
        };
        // "with" of a fight is a list; "success.narration" goes inside its branch.
        if field.key == "with" {
            effect["with"] = json!([value]);
        } else if let Some(n) = field.key.strip_prefix("choices.") {
            // The ways of a "random", in order; an empty one is left out.
            let _ = n;
            if value.as_array().is_some_and(|a| !a.is_empty()) {
                let choices = effect["choices"].as_array().cloned().unwrap_or_default();
                effect["choices"] = json!(choices.into_iter().chain(std::iter::once(value)).collect::<Vec<_>>());
            }
        } else if let Some((branch, key)) = field.key.split_once('.') {
            effect[branch] = json!({ key: value, "effects": [] });
        } else {
            effect[field.key] = value;
        }
    }
    effect
}

/// Steps inside an "if", a "repeat" or a script: the list and the form to add to it.
fn steps_only(ui: &mut Ui, list: &mut Vec<Value>, form: &mut Form, _words: &mut String, world: &World, lang: Lang) {
    let (mut up, mut gone) = (None, None);
    for (n, effect) in list.iter().enumerate() {
        ui.horizontal(|row| {
            row.label(format!("{}. {}", n + 1, describe(effect, world, lang)));
            if n > 0 && row.add(button("▲")).clicked() {
                up = Some(n);
            }
            if row.add(button("×")).clicked() {
                gone = Some(n);
            }
        });
    }
    if let Some(n) = up {
        list.swap(n, n - 1);
    }
    if let Some(n) = gone {
        list.remove(n);
    }
    if let Some(effect) = builder(ui, form, world, lang) {
        list.push(effect);
    }
}

/// A script being put together: effects one under another, in the order they happen, each
/// can be moved up or down or taken out; new ones come from the form of every effect, or the
/// model writes them from words. Returns the words when the model is to be asked.
pub fn script_list(ui: &mut Ui, list: &mut Vec<Value>, form: &mut Form, words: &mut String, asking: bool, world: &World, lang: Lang) -> Option<String> {
    if list.is_empty() {
        hint(ui, T::FxEmptyYetAddThe.text(lang));
    }
    let (mut up, mut gone) = (None, None);
    for (n, effect) in list.iter().enumerate() {
        ui.horizontal(|row| {
            row.label(egui::RichText::new(format!("{}. {}", n + 1, describe(effect, world, lang))).color(crate::ui::theme::c(crate::theme::INK)));
            if n > 0 && row.add(button("▲")).clicked() {
                up = Some(n);
            }
            if row.add(button("×")).clicked() {
                gone = Some(n);
            }
        });
    }
    if let Some(n) = up {
        list.swap(n, n - 1);
    }
    if let Some(n) = gone {
        list.remove(n);
    }
    if let Some(effect) = builder(ui, form, world, lang) {
        list.push(effect);
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new(T::FxOrDescribeItIn.text(lang)).strong());
    ui.add(egui::TextEdit::multiline(words).desired_rows(2).desired_width(f32::INFINITY).hint_text(T::FxEGABlast.text(lang)));
    let mut ask = None;
    ui.horizontal(|row| {
        if row.add(button(T::FxTheAiWritesIt.text(lang)).enabled(!words.trim().is_empty() && !asking, if asking { T::FxTheAiIsWriting.text(lang) } else { T::FxDescribeItFirst.text(lang) })).clicked() {
            ask = Some(words.trim().to_string());
        }
        if asking {
            row.spinner();
        }
    });
    ask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filled_form_is_the_effect_the_model_would_write() {
        let tile = op("tile").unwrap();
        let values = [("cell", "3, 4".to_string()), ("tile", "rubble".to_string())].into_iter().collect();
        assert_eq!(build(tile, &values), json!({ "op": "tile", "x": 3, "y": 4, "tile": "rubble" }));
        let check = op("check").unwrap();
        let values = [("ability", "dex".to_string()), ("dc", "14".to_string()), ("success.narration", "You leap it.".to_string())].into_iter().collect();
        let effect = build(check, &values);
        assert_eq!(effect["dc"], json!(14));
        assert_eq!(effect["success"]["narration"], json!("You leap it."));
        let lines = op("lines").unwrap();
        let values = [("id", "guard".to_string()), ("lines", "Halt!\nWho goes there?".to_string())].into_iter().collect();
        assert_eq!(build(lines, &values)["lines"], json!(["Halt!", "Who goes there?"]));
    }

    #[test]
    fn every_effect_the_game_knows_is_in_the_masters_form() {
        // The tags of every effect, read from the game's own source, must all be offered.
        let source = include_str!("../../../world/src/story.rs");
        let start = source.find("pub enum Effect {").unwrap();
        let end = start + source[start..].find("\n}\n").unwrap();
        let mut missing = Vec::new();
        for line in source[start..end].lines() {
            let line = line.trim_start();
            let Some(name) = line.strip_suffix(" {").or_else(|| line.strip_suffix(" { id: String },")).or_else(|| line.strip_suffix(",")) else { continue };
            let name = name.split([' ', '{', '(']).next().unwrap_or("");
            if name.is_empty() || !name.chars().next().unwrap().is_ascii_uppercase() || line.starts_with("///") || line.starts_with('#') {
                continue;
            }
            let snake: String = name.chars().enumerate().flat_map(|(i, c)| if c.is_ascii_uppercase() && i > 0 { vec!['_', c.to_ascii_lowercase()] } else { vec![c.to_ascii_lowercase()] }).collect();
            if op(&snake).is_none() {
                missing.push(snake);
            }
        }
        assert!(missing.is_empty(), "not offered to the human master: {missing:?}");
    }

    #[test]
    fn every_op_the_master_offers_is_one_the_game_reads_or_will() {
        // Each op in the list is told to the model too, so both masters can use it.
        for op in OPS {
            assert!(!op.fields.is_empty() || matches!(op.op, "xp" | "peace"), "{}", op.op);
        }
    }
}
