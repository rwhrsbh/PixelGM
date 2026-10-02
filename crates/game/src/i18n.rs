//! Interface strings. Every string exists in both languages or the build fails.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    Ru,
    En,
}

macro_rules! texts {
    ($($name:ident: $ru:literal, $en:literal;)*) => {
        /// A translatable string. Put it on an entity next to `Text` and the text follows the
        /// current language.
        #[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
        pub enum T { $($name),* }

        impl T {
            pub fn text(self, lang: Lang) -> &'static str {
                match (self, lang) {
                    $((T::$name, Lang::Ru) => $ru, (T::$name, Lang::En) => $en,)*
                }
            }

            #[cfg(test)]
            const ALL: &'static [T] = &[$(T::$name),*];
        }
    };
}

texts! {
    Tagline: "настольная игра, которую ведёт нейросеть", "a tabletop game run by a neural network";
    NewGame: "Новая игра", "New game";
    Continue: "Продолжить", "Continue";
    Multiplayer: "Сетевая игра", "Multiplayer";
    Keys: "Ключи Gemini", "Gemini keys";
    Settings: "Настройки", "Settings";
    Quit: "Выход", "Quit";
    Back: "Назад", "Back";

    KeysHelp: "Бесплатный ключ даёт около 20 ходов мастера в сутки на каждую модель. Ключ с оплатой работает без суточного лимита. Ключ выдаёт Google AI Studio:",
        "A free key allows about 20 game master turns a day on each model. A key with billing has no daily limit. Google AI Studio gives out keys:";
    KeysInputHint: "Ctrl+V — вставить, Enter — добавить", "Ctrl+V to paste, Enter to add";
    Add: "Добавить", "Add";
    NoKeys: "Ключей пока нет. Без ключа мастер не сможет вести игру.", "No keys yet. Without a key the game master cannot run the game.";
    KeyChecking: "проверка…", "checking…";
    KeyWorking: "работает", "working";
    KeyRejected: "отклонён", "rejected";
    KeyUnreachable: "нет связи", "unreachable";
    Recheck: "Повторить", "Retry";
    Remove: "Убрать", "Remove";
    ModelsTitle: "Модели по порядку", "Models, in order";
    ModelsUnknown: "Список моделей появится, когда заработает хотя бы один ключ.", "The model list appears once at least one key works.";
    ModelsLegend: "Справа — сколько ключей свободно, или через сколько вернётся квота.", "On the right: how many keys are free, or how soon the quota returns.";
    TestGm: "Проверить мастера", "Test the game master";
    TestGmHint: "Один настоящий запрос с текущими настройками. Тратит один ход.", "One real request with the current settings. Spends one turn.";
    TestRunning: "Мастер думает…", "The game master is thinking…";
    TestAnswered: "ответил за", "answered in";
    TestNoKeys: "Нет рабочих ключей.", "No working keys.";
    TestExhausted: "Все разрешённые модели отдыхают. Первая вернётся через", "Every allowed model is resting. The first returns in";
    TestNetwork: "Нет связи с Gemini.", "Gemini cannot be reached.";
    TestBlocked: "Модель отказалась отвечать:", "The model refused to answer:";
    TestRejected: "Запрос отклонён:", "The request was rejected:";
    TestUnreadable: "Мастер ответил неразборчиво:", "The game master's answer could not be read:";
    LoadGame: "Загрузить", "Load";
    NoSaves: "Сохранений пока нет.", "No saves yet.";
    AutoSave: "авто", "auto";
    ManualSave: "вручную", "manual";
    Ago: "назад", "ago";
    NewWorldTitle: "Новый мир", "New world";
    NewWorldAsk: "Каким будет мир? Опиши его парой фраз — или оставь поле пустым, и мастер решит сам.", "What world shall it be? Describe it in a sentence or two, or leave the field empty and the game master will decide.";
    CreateWorld: "Создать мир", "Create the world";
    NeedKey: "Мастеру нужен ключ Gemini. Сначала добавь его.", "The game master needs a Gemini key. Add one first.";
    Creating: "Мастер придумывает и рисует мир", "The game master is inventing and drawing the world";
    CreatingHint: "Это один большой запрос: обычно от полуминуты до двух минут.", "This is one large request: usually half a minute to two minutes.";
    Elapsed: "Прошло", "Elapsed:";
    CreateFailed: "Мир создать не удалось.", "The world could not be created.";
    HudHint: "E — действие · C — герой · J — журнал · Esc — меню", "E to act · C hero · J journal · Esc menu";
    HeroTitle: "Твой герой", "Your hero";
    HeroName: "Имя", "Name";
    HeroStory: "О герое", "About";
    HeroRace: "Народ", "People";
    HeroClass: "Класс", "Class";
    MethodPointBuy: "Покупка очков", "Point buy";
    MethodArray: "Стандартный набор", "Standard array";
    MethodRoll: "Бросок 4d6", "Roll 4d6";
    Reroll: "Перебросить", "Reroll";
    PointsLeft: "Очков осталось:", "Points left:";
    SwapHint: "Нажми «Поменять» у двух характеристик.", "Press Swap on two abilities.";
    Swap: "Поменять", "Swap";
    HeroSkills: "Навыки:", "Skills:";
    HitPoints: "Хиты", "HP";
    ArmorClass: "КД", "AC";
    Spells: "Заклинания", "Spells";
    CreateHero: "В путь", "Set out";
    NeedName: "Герою нужно имя.", "The hero needs a name.";
    NeedSkills: "Выбери навыки, столько:", "Choose this many skills:";
    Level: "уровень", "level";
    Experience: "Опыт", "XP";
    Weapons: "Оружие:", "Weapons:";
    Slots: "Ячейки:", "Slots:";
    PointsToPlace: "Очков характеристик к распределению:", "Ability points to place:";
    CheckTitle: "Проверка", "Check";
    Difficulty: "Сл", "DC";
    RollDie: "Бросить d20", "Roll the d20";
    CheckSuccess: "успех", "success";
    CheckFailure: "провал", "failure";
    LongRest: "Долгий отдых", "Long rest";
    Rested: "Партия отдохнула", "The party has rested";
    FightTurn: "Ход:", "Turn:";
    Movement: "шагов", "steps";
    Move: "Идти", "Move";
    EndTurn: "Конец хода", "End turn";
    Cancel: "Отмена", "Cancel";
    Dodge: "Уклонение", "Dodge";
    Dash: "Рывок", "Dash";
    Disengage: "Отход", "Disengage";
    MoveHint: "Стрелки или мышь — клетка, Enter — идти, Esc — отмена.", "Arrows or mouse pick a cell, Enter moves, Esc cancels.";
    AimHint: "Стрелки или мышь — цель, Enter — подтвердить, Esc — отмена.", "Arrows or mouse pick a target, Enter confirms, Esc cancels.";
    NoTargets: "В досягаемости никого нет.", "Nobody is in range.";
    FightHit: "попадание", "hits";
    FightMiss: "промах", "misses";
    FightCrit: "критический удар", "critical hit";
    FightDamage: "урон", "damage";
    FightSave: "спасбросок", "save";
    FightSaved: "устоял", "saved";
    FightFailed: "не устоял", "failed";
    Against: "против", "against";
    FightHeals: "лечит", "heals";
    FightFalls: "падает без сознания", "falls unconscious";
    FightDies: "погибает", "dies";
    FightRises: "снова на ногах", "is back up";
    DeathSave: "спасбросок от смерти", "death save";
    Victory: "Победа", "Victory";
    Defeat: "Поражение", "Defeat";
    XpEach: "Опыта каждому:", "XP each:";
    DefeatAsk: "Вся партия повержена.", "The whole party is down.";
    MasterDecides: "Пусть решит мастер", "Let the master decide";
    DefeatSaid: "Вся партия пала в бою. Что с нами стало?", "The whole party fell in the fight. What became of us?";
    OwnAction: "Своё действие", "Something else";
    Leave: "Отойти", "Leave";
    WhatDoYouDo: "Что вы делаете?", "What do you do?";
    Say: "Отправить", "Send";
    WritingHint: "Enter — отправить мастеру. Это тратит один ход.", "Enter sends it to the game master. This spends one turn.";
    Onward: "Дальше", "Go on";
    RefusedTitle: "Мастер не ответил", "The master did not answer";
    Journal: "Журнал", "Journal";
    Carried: "С собой:", "Carrying:";
    CarryNothing: "С собой ничего нет.", "Carrying nothing.";
    JournalEmpty: "История ещё не началась.", "The story has not begun yet.";
    SetPredraw: "Соседние места заранее", "Draw neighbours ahead";
    Yes: "Да", "Yes";
    No: "Нет", "No";
    PredrawHint: "Вход без ожидания, но каждое место тратит ход заранее.", "No wait at the door, but each place spends a turn in advance.";
    SetInitiative: "Мастер действует сам", "The master acts unprompted";
    InitiativeOff: "Никогда", "Never";
    InitiativeRare: "Редко", "Rarely";
    InitiativeOften: "Часто", "Often";
    InitiativeHint: "После долгой тишины мастер устраивает событие. Тратит ход.", "After a long silence the master makes something happen. Spends a turn.";
    ViolenceOn: "Насилие: подробно", "Violence: graphic";
    ViolenceOff: "Насилие: за кадром", "Violence: off the page";
    SexOn: "Секс: откровенно", "Sex: explicit";
    SexOff: "Секс: за кадром", "Sex: off the page";
    Drawing: "Мастер рисует место:", "The game master is drawing:";
    DrawingHint: "Обычно это занимает до пары минут.", "This usually takes up to a couple of minutes.";
    StuckTitle: "Место не нарисовано", "The place was not drawn";
    Sketch: "Набросать без мастера", "Sketch it without the master";
    StayHere: "Остаться здесь", "Stay here";
    Paused: "Пауза", "Paused";
    SaveGame: "Сохранить", "Save";
    Saved: "Сохранено", "Saved";
    ToMainMenu: "В главное меню", "Main menu";
    Hours: "ч", "h";
    Minutes: "мин", "min";
    Seconds: "с", "s";

    SetLanguage: "Язык", "Language";
    LanguageName: "Русский", "English";
    SetWindow: "Окно", "Window";
    Windowed: "В окне", "Windowed";
    Fullscreen: "Во весь экран", "Fullscreen";
    SetThinking: "Размышления мастера", "Game master thinking";
    ThinkingOff: "Выключены", "Off";
    ThinkingLow: "Короткие", "Short";
    ThinkingMedium: "Средние", "Medium";
    ThinkingHigh: "Долгие", "Long";
    ThinkingHint: "Дольше думает — умнее ответ и дольше ожидание.", "Longer thinking: smarter answers, longer waits.";
    SetFallback: "Запасные модели", "Fallback models";
    FallbackFlash: "Только flash", "Flash only";
    FallbackLite: "До flash-lite", "Down to flash-lite";
    FallbackGemma: "До Gemma", "Down to Gemma";
    FallbackHint: "Когда лучшие модели исчерпаны, мастер берёт слабее — не ниже этой.", "When the best models are spent the master uses weaker ones, no lower than this.";
}

/// A wait rounded up, in the two largest units that matter.
pub fn wait_text(ms: u64, lang: Lang) -> String {
    let secs = ms.div_ceil(1000);
    let (hours, minutes) = (secs / 3600, secs % 3600 / 60);
    if hours > 0 {
        format!("{hours} {} {minutes} {}", T::Hours.text(lang), T::Minutes.text(lang))
    } else if minutes > 0 {
        format!("{minutes} {}", T::Minutes.text(lang))
    } else {
        format!("{secs} {}", T::Seconds.text(lang))
    }
}

fn apply_language(lang: Res<Lang>, mut texts: Query<(&T, &mut Text, Ref<T>)>) {
    for (key, mut text, changed) in &mut texts {
        if lang.is_changed() || changed.is_changed() {
            text.0 = key.text(*lang).to_string();
        }
    }
}

pub struct I18nPlugin;

impl Plugin for I18nPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Lang>().add_systems(PostUpdate, apply_language);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_are_rounded_up_and_use_the_largest_units() {
        assert_eq!(wait_text(29_594_000, Lang::Ru), "8 ч 13 мин");
        assert_eq!(wait_text(125_000, Lang::En), "2 min");
        assert_eq!(wait_text(1, Lang::En), "1 s");
        assert_eq!(wait_text(59_001, Lang::Ru), "1 мин");
    }

    #[test]
    fn no_string_is_empty_in_either_language() {
        for t in T::ALL {
            assert!(!t.text(Lang::Ru).is_empty() && !t.text(Lang::En).is_empty(), "{t:?}");
        }
    }

    #[test]
    fn russian_strings_are_actually_russian() {
        for t in T::ALL {
            assert!(t.text(Lang::Ru).chars().any(|c| ('а'..='я').contains(&c) || ('А'..='Я').contains(&c)), "{t:?}");
        }
    }
}
