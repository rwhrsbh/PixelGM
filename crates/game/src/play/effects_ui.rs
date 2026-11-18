//! Every effect the model can write, as a form the human master fills in: the op is picked
//! from a list, its fields are asked one by one with the things of the map to choose from,
//! and the result is the very JSON the model would have written. So whatever the model can
//! do to the world, the master can too.

use crate::i18n::Lang;
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
    /// The ground under a cell: x and y.
    Cell,
    Tile,
    Bool,
    Place,
    /// One of the words listed after the field's name.
    Pick(&'static [&'static str]),
    /// A list of words, one a line.
    Lines,
}

pub struct Field {
    pub key: &'static str,
    pub ru: &'static str,
    pub en: &'static str,
    pub kind: Kind,
}

const fn f(key: &'static str, ru: &'static str, en: &'static str, kind: Kind) -> Field {
    Field { key, ru, en, kind }
}

pub struct Op {
    pub op: &'static str,
    pub ru: &'static str,
    pub en: &'static str,
    /// A group of the list: what the effect is about.
    pub group: usize,
    pub fields: &'static [Field],
}

const MOTIONS: &[&str] = &["jump", "crouch", "sit", "lie", "fall", "shake", "spin", "strike", "dance", "wave", "nod", "bow", "cower", "stagger"];
const KINDS: &[&str] = &["weapon", "armor", "light", "potion", "food", "throwable", "key", "tool", "junk", "quest"];
const MARKS: &[&str] = &["blood", "dung", "puddle", "scorch", "ash", "slime", "vomit", "wine", "paint", "rubble", "coins", "bones"];
const MOODS: &[&str] = &["calm", "explore", "tense", "battle", "boss", "mystery", "horror", "sad", "tavern", "triumph", "dungeon"];
const ATTITUDES: &[&str] = &["hostile", "neutral", "friendly"];
const SECRETS: &[&str] = &["hidden", "trap", "plate"];
const LIGHTS: &[&str] = &["day", "dusk", "night", "dark"];
const WEATHERS: &[&str] = &["clear", "sun", "rain", "storm", "snow", "fog"];
const ABILITIES: &[&str] = &["str", "dex", "con", "int", "wis", "cha"];
const DONE: &[&str] = &["", "won", "lost"];
const ORDER: &[&str] = &["in_turn", "random"];

/// The groups of the list, in order.
pub const GROUPS: [(&str, &str); 7] = [("Отряд", "The party"), ("Существа и вещи", "Creatures and things"), ("Карта", "The map"), ("Мир и время", "World and time"), ("История", "The story"), ("Зрелище", "Spectacle"), ("Сценарии", "Scripts")];

pub const OPS: &[Op] = &[
    Op { op: "damage", ru: "Урон", en: "Damage", group: 0, fields: &[f("amount", "Сколько", "Amount", Kind::Dice), f("id", "Кому (пусто — отряду)", "To whom (empty: the party)", Kind::Someone)] },
    Op { op: "heal", ru: "Лечение", en: "Healing", group: 0, fields: &[f("amount", "Сколько", "Amount", Kind::Dice), f("id", "Кому (пусто — отряду)", "To whom (empty: the party)", Kind::Someone)] },
    Op { op: "give", ru: "Выдать вещь", en: "Give a thing", group: 0, fields: &[f("name", "Название", "Name", Kind::Text), f("description", "Описание", "Description", Kind::Text), f("kind", "Вид", "Kind", Kind::Pick(KINDS)), f("heal", "Лечит", "Heals", Kind::Dice), f("damage", "Урон", "Damage", Kind::Dice), f("to", "Кому", "To whom", Kind::Hero)] },
    Op { op: "take", ru: "Забрать вещь", en: "Take a thing", group: 0, fields: &[f("name", "Название", "Name", Kind::Text)] },
    Op { op: "gold", ru: "Золото", en: "Gold", group: 0, fields: &[f("amount", "Сколько (минус — отнять)", "Amount (minus takes)", Kind::Number), f("to", "Кому", "To whom", Kind::Hero)] },
    Op { op: "xp", ru: "Опыт", en: "Experience", group: 0, fields: &[f("amount", "Сколько", "Amount", Kind::Number)] },
    Op { op: "check", ru: "Проверка (бросок)", en: "Check (a roll)", group: 0, fields: &[f("ability", "Характеристика", "Ability", Kind::Pick(ABILITIES)), f("skill", "Навык (index SRD)", "Skill (SRD index)", Kind::Text), f("dc", "Сложность", "Difficulty", Kind::Number), f("success.narration", "При успехе", "On success", Kind::Long), f("failure.narration", "При неудаче", "On failure", Kind::Long)] },
    Op { op: "deed", ru: "Подвиг героя", en: "A hero's deed", group: 0, fields: &[f("hero", "Герой", "Hero", Kind::Hero)] },
    Op { op: "revive", ru: "Воскресить", en: "Revive", group: 0, fields: &[f("hero", "Герой", "Hero", Kind::Hero)] },
    Op { op: "move", ru: "Переместить", en: "Move", group: 1, fields: &[f("id", "Кого", "Whom", Kind::Someone), f("cell", "Куда", "Where", Kind::Cell)] },
    Op { op: "spawn", ru: "Создать существо или вещь", en: "Spawn a creature or thing", group: 1, fields: &[f("kind", "Что", "What", Kind::Pick(&["creature", "prop"])), f("id", "id", "id", Kind::Text), f("name", "Имя", "Name", Kind::Text), f("description", "Описание", "Description", Kind::Text), f("cell", "Где", "Where", Kind::Cell), f("sprite", "Спрайт (id)", "Sprite (id)", Kind::Text), f("monster", "Монстр SRD", "SRD monster", Kind::Text), f("attitude", "Отношение", "Attitude", Kind::Pick(ATTITUDES)), f("size", "Размер, клеток", "Size, cells", Kind::Number), f("blocking", "Не пройти сквозь", "Blocks the way", Kind::Bool)] },
    Op { op: "remove", ru: "Убрать", en: "Remove", group: 1, fields: &[f("id", "Кого", "Whom", Kind::Thing)] },
    Op { op: "kill", ru: "Убить", en: "Kill", group: 1, fields: &[f("id", "Кого", "Whom", Kind::Thing)] },
    Op { op: "attitude", ru: "Отношение", en: "Attitude", group: 1, fields: &[f("id", "Кто", "Who", Kind::Thing), f("attitude", "Отношение", "Attitude", Kind::Pick(ATTITUDES))] },
    Op { op: "fight", ru: "Начать бой", en: "Start a fight", group: 1, fields: &[f("with", "С кем", "With whom", Kind::Thing)] },
    Op { op: "describe", ru: "Новое описание", en: "New description", group: 1, fields: &[f("id", "Что", "What", Kind::Thing), f("description", "Описание", "Description", Kind::Long)] },
    Op { op: "join", ru: "Присоединить к отряду", en: "Join the party", group: 1, fields: &[f("id", "Кто", "Who", Kind::Thing)] },
    Op { op: "dismiss", ru: "Отпустить спутника", en: "Dismiss a companion", group: 1, fields: &[f("id", "Кто", "Who", Kind::Thing)] },
    Op { op: "edit", ru: "Изменить существо/вещь", en: "Change a creature or thing", group: 1, fields: &[f("id", "Что", "What", Kind::Thing), f("name", "Новое имя", "New name", Kind::Text), f("size", "Размер, клеток", "Size, cells", Kind::Number), f("ac", "КД", "AC", Kind::Number), f("hp", "Хиты", "HP", Kind::Number), f("speed", "Скорость", "Speed", Kind::Number), f("xp", "Опыт за победу", "XP for beating it", Kind::Number), f("monster", "Монстр SRD", "SRD monster", Kind::Text), f("blocking", "Не пройти сквозь", "Blocks the way", Kind::Bool), f("sprite", "Спрайт (id)", "Sprite (id)", Kind::Text)] },
    Op { op: "action", ru: "Действие для вещи", en: "An action for a thing", group: 6, fields: &[f("id", "С чем", "With what", Kind::Thing), f("label", "Что выбирают игроки", "What the players pick", Kind::Text), f("outcome", "Что происходит", "What happens", Kind::Long), f("needs", "Нужна вещь", "Needs a thing", Kind::Text)] },
    Op { op: "lines", ru: "Реплики существа", en: "A creature's lines", group: 1, fields: &[f("id", "Кто", "Who", Kind::Thing), f("lines", "Фразы, по одной в строке", "Phrases, one a line", Kind::Lines), f("every", "Раз в сколько секунд", "Every how many seconds", Kind::Number), f("order", "Порядок", "Order", Kind::Pick(ORDER))] },
    Op { op: "tile", ru: "Сменить землю", en: "Change the ground", group: 2, fields: &[f("cell", "Клетка", "Cell", Kind::Cell), f("tile", "Тайл", "Tile", Kind::Tile)] },
    Op { op: "mark", ru: "След на земле", en: "A mark on the ground", group: 2, fields: &[f("cell", "Клетка", "Cell", Kind::Cell), f("kind", "Что", "What", Kind::Pick(MARKS))] },
    Op { op: "clean", ru: "Убрать след", en: "Clean a mark", group: 2, fields: &[f("cell", "Клетка", "Cell", Kind::Cell)] },
    Op { op: "secret", ru: "Тайник, ловушка, плита", en: "A cache, a trap, a plate", group: 2, fields: &[f("cell", "Клетка", "Cell", Kind::Cell), f("kind", "Вид", "Kind", Kind::Pick(SECRETS)), f("dc", "Сложность заметить", "Difficulty to notice", Kind::Number), f("note", "Что это", "What it is", Kind::Text)] },
    Op { op: "exit", ru: "Новый выход", en: "A new way out", group: 2, fields: &[f("cell", "Клетка", "Cell", Kind::Cell), f("to", "Куда (id места)", "Where to (place id)", Kind::Place), f("label", "Как выглядит", "What it looks like", Kind::Text)] },
    Op { op: "travel", ru: "Перенести в другое место", en: "Travel elsewhere", group: 2, fields: &[f("to", "Куда (id места)", "Where to (place id)", Kind::Place), f("name", "Имя нового места", "Name of a new place", Kind::Text), f("description", "Каким оно будет", "What it looks like", Kind::Long)] },
    Op { op: "time", ru: "Прошло время", en: "Time passes", group: 3, fields: &[f("hours", "Часов", "Hours", Kind::Number)] },
    Op { op: "light", ru: "Свет", en: "Light", group: 3, fields: &[f("light", "Свет", "Light", Kind::Pick(LIGHTS))] },
    Op { op: "weather", ru: "Погода", en: "Weather", group: 3, fields: &[f("weather", "Погода", "Weather", Kind::Pick(WEATHERS))] },
    Op { op: "music", ru: "Музыка места", en: "The place's music", group: 3, fields: &[f("mood", "Настроение", "Mood", Kind::Pick(MOODS))] },
    Op { op: "fact", ru: "Запомнить факт", en: "Remember a fact", group: 4, fields: &[f("text", "Факт", "Fact", Kind::Long)] },
    Op { op: "quest", ru: "Главное задание: шаг", en: "Main quest: a step", group: 4, fields: &[f("goal", "Новая цель", "New goal", Kind::Text), f("advance", "Следующий акт", "Next act", Kind::Bool), f("done", "Закончено", "Ended", Kind::Pick(DONE))] },
    Op { op: "quest_new", ru: "Новое главное задание", en: "A new main quest", group: 4, fields: &[f("title", "Название", "Title", Kind::Text), f("goal", "Цель", "Goal", Kind::Text), f("acts", "Акты, по одному в строке", "Acts, one a line", Kind::Lines), f("ending", "Победа", "Won", Kind::Text), f("failure", "Поражение", "Lost", Kind::Text)] },
    Op { op: "task", ru: "Побочное задание", en: "A side task", group: 4, fields: &[f("name", "Название", "Name", Kind::Text), f("step", "Что делать", "What to do", Kind::Text), f("done", "Выполнено", "Done", Kind::Bool)] },
    Op { op: "faction", ru: "Фракция", en: "A faction", group: 4, fields: &[f("id", "id", "id", Kind::Text), f("name", "Имя", "Name", Kind::Text), f("description", "Кто они", "Who they are", Kind::Text), f("standing", "Отношение −3…3", "Standing −3…3", Kind::Number)] },
    Op { op: "standing", ru: "Отношение фракции", en: "A faction's regard", group: 4, fields: &[f("id", "Фракция (id)", "Faction (id)", Kind::Text), f("change", "Изменение", "Change", Kind::Number)] },
    Op { op: "thread", ru: "Нить (вернётся позже)", en: "A thread (comes back)", group: 4, fields: &[f("text", "Что и кто вернётся", "What, and who comes back", Kind::Long)] },
    Op { op: "thread_done", ru: "Нить закрыта", en: "A thread closed", group: 4, fields: &[f("text", "Какая", "Which", Kind::Text)] },
    Op { op: "threat", ru: "Угроза по часам", en: "A threat on the clock", group: 4, fields: &[f("name", "Имя", "Name", Kind::Text), f("what", "Что случится", "What happens", Kind::Long), f("rumour", "Слухи", "Rumour", Kind::Text), f("hours", "Через часов", "In hours", Kind::Number)] },
    Op { op: "threat_end", ru: "Угроза снята", en: "A threat ended", group: 4, fields: &[f("name", "Имя", "Name", Kind::Text)] },
    Op { op: "shake", ru: "Тряска экрана", en: "Shake the screen", group: 5, fields: &[f("seconds", "Секунд", "Seconds", Kind::Number)] },
    Op { op: "animate", ru: "Движение", en: "A motion", group: 5, fields: &[f("id", "Кто", "Who", Kind::Someone), f("motion", "Движение", "Motion", Kind::Pick(MOTIONS))] },
    Op { op: "say", ru: "Реплика облачком", en: "A line in a bubble", group: 5, fields: &[f("id", "Кто", "Who", Kind::Someone), f("text", "Что говорит", "What they say", Kind::Text)] },
    Op { op: "wait", ru: "Пауза (дальше — позже)", en: "Wait (the rest later)", group: 6, fields: &[f("seconds", "Секунд", "Seconds", Kind::Number)] },
    Op { op: "set", ru: "Переменная = значение", en: "Variable = value", group: 6, fields: &[f("var", "Переменная", "Variable", Kind::Text), f("value", "Значение", "Value", Kind::Number)] },
    Op { op: "add", ru: "Переменная + число", en: "Variable + number", group: 6, fields: &[f("var", "Переменная", "Variable", Kind::Text), f("value", "Сколько", "By", Kind::Number)] },
    Op { op: "script", ru: "Запустить сценарий", en: "Run a script", group: 6, fields: &[f("id", "Сценарий (id)", "Script (id)", Kind::Text)] },
];

/// The op's entry, when the game knows it.
pub fn op(name: &str) -> Option<&'static Op> {
    OPS.iter().find(|o| o.op == name)
}

/// An effect in a line a master can read: its name and what it is set to.
pub fn describe(effect: &Value, world: &World, lang: Lang) -> String {
    let ru = lang == Lang::Ru;
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
    format!("{}: {}", if ru { op.ru } else { op.en }, parts.join(" · "))
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
}

/// The form for one effect; returns the effect once "Add" is pressed.
pub fn builder(ui: &mut Ui, form: &mut Form, world: &World, lang: Lang) -> Option<Value> {
    let ru = lang == Lang::Ru;
    if !form.open {
        if ui.add(button(if ru { "+ Добавить изменение…" } else { "+ Add a change…" })).clicked() {
            form.open = true;
        }
        return None;
    }
    let mut made = None;
    crate::ui::egui::Frame::new().fill(crate::ui::theme::c(crate::theme::PARCHMENT_SHADE)).inner_margin(6.0).show(ui, |ui| {
        let current = &OPS[form.op.min(OPS.len() - 1)];
        let name = |o: &Op| if ru { o.ru } else { o.en };
        if !form.fixed {
        egui::ComboBox::from_id_salt(("effect_op", ui.id())).width(260.0).height(320.0).selected_text(name(current)).show_ui(ui, |ui| {
            for (g, (gru, gen_)) in GROUPS.iter().enumerate() {
                ui.label(egui::RichText::new(if ru { *gru } else { *gen_ }).strong());
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
                row.add_sized([130.0, 18.0], egui::Label::new(if ru { field.ru } else { field.en }).wrap());
                match field.kind {
                    Kind::Thing | Kind::Someone => {
                        let shown = things.iter().find(|(id, _)| id == value).map_or(value.clone(), |(_, n)| n.clone());
                        egui::ComboBox::from_id_salt(("thing", field.key)).width(150.0).selected_text(if shown.is_empty() { "—".to_string() } else { shown }).show_ui(row, |ui| {
                            if field.kind == Kind::Someone {
                                ui.selectable_value(value, String::new(), if ru { "— отряд —" } else { "— the party —" });
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
                        row.add(egui::TextEdit::singleline(value).desired_width(80.0).hint_text(if ru { "или новый id" } else { "or a new id" }));
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
                        hint(row, if ru { "сейчас: где герой" } else { "now: where the hero is" });
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
            if row.add(button(if ru { "Добавить" } else { "Add" }).primary()).clicked() {
                made = Some(build(current, &form.values));
                form.open = false;
                form.values.clear();
            }
            if row.add(button(if ru { "Отмена" } else { "Cancel" })).clicked() {
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
            Kind::Lines => json!(text.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>()),
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
        } else if let Some((branch, key)) = field.key.split_once('.') {
            effect[branch] = json!({ key: value, "effects": [] });
        } else {
            effect[field.key] = value;
        }
    }
    effect
}

/// A script being put together: effects one under another, in the order they happen, each
/// can be moved up or down or taken out; new ones come from the form of every effect, or the
/// model writes them from words. Returns the words when the model is to be asked.
pub fn script_list(ui: &mut Ui, list: &mut Vec<Value>, form: &mut Form, words: &mut String, asking: bool, world: &World, lang: Lang) -> Option<String> {
    let ru = lang == Lang::Ru;
    if list.is_empty() {
        hint(ui, if ru { "Пока пусто. Добавь шаги по порядку: что случится, «Пауза», что потом…" } else { "Empty yet. Add the steps in order: what happens, a Wait, what then…" });
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
    ui.label(egui::RichText::new(if ru { "Или опиши словами — ИИ напишет шаги:" } else { "Or describe it in words: the AI writes the steps:" }).strong());
    ui.add(egui::TextEdit::multiline(words).desired_rows(2).desired_width(f32::INFINITY).hint_text(if ru { "например: взрыв, тряска, через 3 минуты ещё взрыв и обвал на выходе" } else { "e.g.: a blast, a shake, three minutes later another blast and the exit caves in" }));
    let mut ask = None;
    ui.horizontal(|row| {
        if row.add(button(if ru { "ИИ напишет" } else { "The AI writes it" }).enabled(!words.trim().is_empty() && !asking, if asking { if ru { "ИИ пишет…" } else { "The AI is writing…" } } else if ru { "Сначала опиши" } else { "Describe it first" })).clicked() {
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
    fn every_op_the_master_offers_is_one_the_game_reads_or_will() {
        // Each op in the list is told to the model too, so both masters can use it.
        for op in OPS {
            assert!(!op.fields.is_empty() || op.op == "xp", "{}", op.op);
        }
    }
}
