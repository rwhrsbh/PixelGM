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
    HudMenuHint: "Esc — меню", "Esc for the menu";
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
    ThinkingHint: "Чем дольше мастер думает, тем умнее ответ и тем дольше ожидание.", "The longer the game master thinks, the smarter the answer and the longer the wait.";
    SetFallback: "Запасные модели", "Fallback models";
    FallbackFlash: "Только flash", "Flash only";
    FallbackLite: "До flash-lite", "Down to flash-lite";
    FallbackGemma: "До Gemma", "Down to Gemma";
    FallbackHint: "Когда лучшие модели исчерпаны, мастер переходит на более слабые — не ниже выбранной ступени.", "When the best models are spent the game master falls back to weaker ones, no lower than the chosen tier.";
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
