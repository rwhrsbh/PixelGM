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
    HudHint: "I сумка C герой J журнал M карта Esc", "I bag C hero J journal M map Esc";
    LobbyHelp: "Один игрок создаёт игру и ведёт мир; остальные подключаются к нему — через Steam или по адресу. Создав игру, начни или загрузи мир как обычно.", "One player hosts and holds the world; the others join through Steam or at the host's address. After hosting, start or load a world as usual.";
    LobbyName: "Твоё имя", "Your name";
    LobbyAddress: "Адрес хоста", "Host address";
    LobbyJoin: "Подключиться", "Join";
    LobbyHost: "Создать игру по адресу", "Host by address";
    SteamHost: "Создать игру в Steam", "Host on Steam";
    SteamUnavailable: "Steam не запущен: остаётся подключение по адресу.", "Steam is not running: only a direct address is left.";
    SteamFriends: "Друзья ведут игру:", "Friends hosting:";
    SteamNoFriends: "Никто из друзей в Steam сейчас не ведёт игру.", "No Steam friend is hosting right now.";
    SteamOpening: "Steam: открываю лобби…", "Steam: opening the lobby…";
    SteamOpen: "Steam: лобби открыто. Друзья увидят игру в этом меню у себя.", "Steam: the lobby is open. Friends will see the game in this menu.";
    SteamRefused: "Steam не дал открыть лобби:", "Steam refused the lobby:";
    DirectTitle: "Напрямую, по адресу", "Directly, by address";
    LobbyLeave: "Отключиться", "Leave";
    LobbyHosting: "Игра создана. Адрес для друзей:", "Hosting. Address for friends:";
    LobbyPlayers: "За столом:", "At the table:";
    LobbyJoined: "Подключено. Ждём мир от хоста…", "Connected. Waiting for the host's world…";
    LobbyWrongVersion: "У хоста другая версия игры.", "The host runs another version of the game.";
    LobbyLost: "Связь с хостом потеряна. Игра сохранена — можно продолжить одному.", "The host is gone. The game was saved; you can go on alone.";
    LobbyHostFailed: "Не удалось создать игру: порт занят.", "Could not host: the port is in use.";
    LobbyJoinFailed: "Не удалось подключиться.", "Could not connect.";
    TableTalk: "Столу", "To the table";
    WritingHintTable: "Enter — мастеру, это тратит ход. «Столу» — только игрокам: мастер не слышит.", "Enter sends it to the game master and spends a turn. \"To the table\" is for the players only: the master does not hear it.";
    FreeHeroes: "Герои без игрока — можно взять:", "Heroes without a player, free to take:";
    SeatTake: "Сесть на место мастера", "Take the master's seat";
    SeatLeave: "Отдать место модели", "Give the seat back to the model";
    SeatNoHero: "Вести игру без героя", "Run the game without a hero";
    SeatYours: "Ты ведёшь игру. G — панель мастера.", "You run the game. G opens the master's panel.";
    HudMaster: " G", " G";
    DraftTitle: "Черновик модели", "The model's draft";
    Accept: "Принять", "Accept";
    Reject: "Отклонить", "Reject";
    Redo: "Переделать", "Redo";
    RedoLabel: "Указание:", "Direction:";
    MasterTitle: "Место мастера", "The master's seat";
    ToModel: "Модели", "To the model";
    Narrate: "Рассказать", "Tell";
    WhisperTo: "Шёпот:", "Whisper to";
    NextPlayer: "Другому", "Someone else";
    ToolCreature: "Существо", "Creature";
    MoodNeutral: "мирное", "peaceful";
    MoodHostile: "враждебное", "hostile";
    ToolRemove: "Убрать рядом", "Remove nearest";
    ToolTile: "Тайл", "Tile";
    ToolGold: "Золото", "Gold";
    ToolHours: "Часы +", "Hours +";
    ToolSky: "Небо/час", "Sky/hour";
    ToolSay: "Реплика", "Say";
    ToolBrush: "Кисть", "Brush";
    BrushOn: "Кисть: клики по карте ставят", "Brush: clicks on the map lay";
    ToolDamage: "Урон", "Damage";
    ToolHeal: "Лечение", "Heal";
    ToolGive: "Предмет", "Item";
    ToolFight: "Бой", "Fight";
    ToolRoll: "Тайный d20", "Hidden d20";
    ToolInvent: "Придумать в библиотеку", "Invent for the library";
    MasterHint: "Поле — параметр кнопки: указание или текст, монстр SRD (goblin), id тайла, кости (2d6), предмет, бонус к d20.", "The field is the button's parameter: a direction or text, an SRD monster (goblin), a tile id, dice (2d6), an item, a d20 bonus.";
    MasterDeclined: "Мастер отклонил ответ.", "The master threw the answer away.";
    WhisperMark: "(только тебе)", "(for you alone)";
    CardSaved: "В библиотеке:", "In the library:";
    NeedText: "Сначала впиши параметр в поле.", "Type the parameter into the field first.";
    NothingNear: "Рядом ничего нет.", "Nothing is near.";
    NobodyToWhisper: "Шептать некому.", "There is nobody to whisper to.";
    Inventing: "Модель придумывает существо…", "The model is inventing the creature…";
    TableMixed: "Правила: книга и выдумки мастера", "Rules: the book and the master's inventions";
    TableBook: "Правила: только по книге", "Rules: by the book only";
    LimitsOn: "Лимиты силы: есть", "Power limits: on";
    LimitsOff: "Лимиты силы: нет", "Power limits: off";
    ShortRest: "Короткий отдых", "Short rest";
    ShortRested: "Партия перевела дух", "The party has caught its breath";
    NewHero: "Новый герой (прежний погиб)", "A new hero (the old one died)";
    FeatOffer: "★ Новая способность: открой лист героя (C)", "★ A new ability: open the hero sheet (C)";
    FeatChoose: "Выбери одну способность:", "Choose one ability:";
    Feats: "Особое:", "Special:";
    SecretSkill: "тайный навык", "a secret skill";
    StuntName: "Импровизация", "Improvised action";
    StuntHint: "Что делает герой? Мастер назначит проверку; действие потратится.", "What does the hero do? The master names the check; the action is spent.";
    Cantrips: "Заговоры", "Cantrips";
    AtWill: "без счёта", "at will";
    PerRest: "за отдых", "per rest";
    HitDice: "Кости хитов:", "Hit dice:";
    Report: "Собрать отчёт", "Collect a report";
    ReportDone: "Отчёт собран, папка открыта", "The report is collected; its folder is open";
    ReportFailed: "Отчёт собрать не удалось", "The report could not be collected";
    SetMusic: "Музыка", "Music";
    SetSound: "Звуки", "Sound";
    Volume0: "выкл", "off";
    Volume1: "четверть", "25%";
    Volume2: "половина", "50%";
    Volume3: "три четверти", "75%";
    Volume4: "в полную силу", "100%";
    Edition51: "Свод правил: SRD 5.1", "Rules edition: SRD 5.1";
    Edition52: "Свод правил: SRD 5.2 (бета)", "Rules edition: SRD 5.2 (beta)";
    NextLevels: "Следующие уровни:", "Next levels go to:";
    SetVoice: "Голос мастера", "The master's voice";
    VoiceHint: "Мастер читает рассказ вслух. Тратит квоту голосовых моделей, отдельную от квоты мастера.", "The master reads the telling aloud. Spends the quota of the speaking models, separate from the master's own.";
    MasterBusy: "Мастер занят другим вопросом. Подожди ответа.", "The master is busy with another question. Wait for the answer.";
    HeroTitle: "Твой герой", "Your hero";
    HeroName: "Имя", "Name";
    HeroStory: "О герое", "About";
    HeroRace: "Народ", "People";
    BagNoRoom: "Не влезло в сумку:", "No room in the bag:";
    BagEmptySlot: "пусто", "empty";
    BagDamage: "урон", "damage";
    BagTwoHanded: "двуручное", "two-handed";
    BagRanged: "дальность", "range";
    SlotMain: "Оружие в руке", "Weapon in hand";
    SlotSpare: "Запасное оружие", "Spare weapon";
    SlotShield: "Щит", "Shield";
    SlotBody: "Доспех", "Armour";
    Graphics: "Графика…", "Graphics…";
    GfxBloom: "Свечение", "Glow";
    GfxVignette: "Виньетка", "Vignette";
    GfxScanLines: "Строки экрана", "Scan lines";
    GfxGlass: "Выпуклое стекло", "Curved glass";
    GfxWeather: "Погода", "Weather";
    GfxNight: "Темнота ночи", "Dark of night";
    GfxCellHint: "Сетка клеток", "Cell grid";
    GfxPreviewOnly: "Только для превью:", "For the preview only:";
    GfxPreviewHour: "Время суток", "Hour";
    GfxPreviewSky: "Небо", "Sky";
    SkyClear: "ясно", "clear";
    SkySun: "солнце", "sun";
    SkyRain: "дождь", "rain";
    SkyStorm: "гроза", "storm";
    SkySnow: "снег", "snow";
    SkyFog: "туман", "fog";
    SetGrade: "Цветокоррекция", "Colour grade";
    GradeHint: "Тон и эффекты поверх картинки мира; интерфейс не меняется", "The tone and effects over the world; the interface stays as it is";
    GradeClean: "чистая", "clean";
    GradeWarm: "тёплая", "warm";
    GradeCold: "холодная", "cold";
    GradeNoir: "нуар", "noir";
    GradeSepia: "сепия", "sepia";
    GradeVivid: "сочная", "vivid";
    GradeDream: "сон", "dream";
    GradeCrt: "кинескоп", "CRT";
    HudFallen: "пал", "fallen";
    HudAway: "нет", "away";
    WalkUpTo: "подойти к:", "walk up to";
    CannotReach: "Туда не пройти.", "There is no way there.";
    BagHeals: "лечит", "heals";
    BagLight: "свет, клеток:", "light, cells:";
    BagUseHint: "ПКМ — применить · Alt+ПКМ — сказать мастеру, для чего · клик за сумкой — бросить", "Right click: use · Alt+right click: tell the master what for · click outside: drop";
    UsesThing: "использует", "uses";
    PickUp: "поднять", "pick up";
    ChestOpen: "Открыть", "Open";
    ChestPick: "Взломать замок", "Pick the lock";
    ChestForce: "Выбить", "Force it";
    ChestLocked: "Заперто. Нужен ключ — или взломать, или выбить.", "Locked. It needs a key, a pick or force.";
    ChestEmpty: "Пусто.", "Empty.";
    ChestTook: "Взято:", "Taken:";
    ChestFailed: "Не поддаётся:", "It holds:";
    Fold: "Свернуть", "Fold";
    Unfold: "Развернуть", "Unfold";
    FoldedHint: "Рассказ свёрнут (H)", "The telling is folded (H)";
    TabLook: "Внешность", "Appearance";
    TabStats: "Способности", "Abilities";
    LookBuild: "Сложение", "Build";
    LookSkin: "Кожа", "Skin";
    LookHair: "Причёска", "Hair";
    LookHairColor: "Цвет волос", "Hair colour";
    LookBeard: "Борода", "Beard";
    LookEyes: "Глаза", "Eyes";
    LookCloth: "Одежда", "Clothes";
    LookTrim: "Отделка", "Trim";
    LookHelmet: "Шлем", "Helmet";
    LookRandom: "Случайно", "Random";
    LookTurn: "Повернуть", "Turn";
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
    FightItems: "Предметы", "Items";
    SheetSize: "Размер", "Size";
    GoldCount: "Золото:", "Gold:";
    Companions: "Спутники", "Companions";
    PackWord: "Ноша", "Pack";
    Tired: "устал", "tired";
    RedrawPlace: "Перерисовать это место", "Draw this place again";
    RedrawNoWay: "Некуда отступить: рядом нет нарисованных мест", "No drawn place beside this one to walk in from";
    SomethingComes: "Кто-то приближается…", "Something is coming…";
    RestBroken: "Отдых прерван: на лагерь напали", "The rest is broken: the camp is attacked";
    Rumours: "Слухи", "Rumours";
    StandingWords: "враги|недруги|холодны|равнодушны|приветливы|друзья|союзники", "enemies|hostile|cold|indifferent|warm|friends|allies";
    BagBetter: "к попаданию и урону / к защите", "to hit and damage / to armour";
    Price: "Цена:", "Price:";
    Trade: "Торговать", "Trade";
    ShopClosed: "Ночью лавка закрыта", "The shop is shut at night";
    ShopTitle: "Лавка", "Shop";
    ShopHint: "Клик — купить. ПКМ по своей вещи — продать за полцены.", "Click: buy. Right click on a thing of yours: sell for half.";
    Haggle: "Торговаться", "Haggle";
    HaggleWon: "Сторговались: дешевле на пятую часть", "A bargain: a fifth off";
    HaggleLost: "Торг не удался: цены выросли", "No bargain: prices went up";
    NoGold: "Не хватает золота", "Not enough gold";
    NoRoom: "Нет места в сумке", "No room in the bag";
    Bought: "Куплено:", "Bought:";
    Sold: "Продано:", "Sold:";
    ShopUnwanted: "Это здесь не купят", "Nobody here wants that";
    RarityUncommon: "необычный", "uncommon";
    RarityRare: "редкий", "rare";
    RarityLegendary: "легендарный", "legendary";
    NotSafe: "Здесь небезопасно: рядом враги", "It is not safe here: enemies are near";
    NeedRation: "Нечего есть: нужен паёк на каждого", "Nothing to eat: each needs a ration";
    DayWord: "День", "Day";
    ModeStory: "Режим: сюжет", "Mode: story";
    ModeSandbox: "Режим: песочница", "Mode: sandbox";
    LiveOn: "Живой мир: да", "Live world: on";
    LiveOff: "Живой мир: нет", "Live world: off";
    DifficultyCasual: "Сложность: лёгкая", "Difficulty: casual";
    DifficultyHonest: "Сложность: честная", "Difficulty: honest";
    DifficultyHardcore: "Сложность: хардкор", "Difficulty: hardcore";
    Goal: "Цель:", "Goal:";
    QuestWon: "Задание выполнено", "Quest done";
    QuestLost: "Задание провалено", "Quest failed";
    NewGoal: "Новая цель:", "New goal:";
    NewTask: "Поручение:", "Task:";
    TaskDone: "Выполнено:", "Done:";
    UseForWhat: "для чего? Напишите, как и на что", "what for? Say how and on what";
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
    DefeatCasual: "Вас одолели, но вы приходите в себя: у каждого один хит.", "You were beaten, but you come to: a hit point each.";
    FightKeys: "1 идти · 2-3 оружие · 4 предметы · 5 уклон · 6 конец хода · F1-F6 под рукой · I сумка", "1 move · 2-3 weapon · 4 items · 5 dodge · 6 end turn · F1-F6 at hand · I bag";
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
    SetOnlineMusic: "Музыка из сети", "Music from the network";
    OnlineMusicHint: "Сотни треков (Kevin MacLeod, CC BY 4.0; OpenGameArt, CC0) подкачиваются по настроению и хранятся на диске; без сети играет скачанное", "Hundreds of pieces (Kevin MacLeod, CC BY 4.0; OpenGameArt, CC0) are fetched by mood and kept on disk; offline, what was fetched plays";
    ProviderOpen: "Свой провайдер…", "Own provider…";
    Provider: "Свой провайдер", "Own provider";
    ProviderHelp: "Вместо Gemini — любой сервис в формате OpenAI: OpenAI, OpenRouter, локальная модель в Ollama (http://localhost:11434/v1) или LM Studio (http://localhost:1234/v1)", "Instead of Gemini, any service speaking the OpenAI protocol: OpenAI, OpenRouter, a local model in Ollama (http://localhost:11434/v1) or LM Studio (http://localhost:1234/v1)";
    ProviderUrl: "Адрес", "Address";
    ProviderModel: "Модель", "Model";
    ProviderKey: "Ключ", "Key";
    ProviderNoKey: "Ключ не задан (локальной модели не нужен)", "No key (a local model needs none)";
    ProviderKeyKept: "Ключ сохранён и зашифрован; новый заменит его", "A key is kept, sealed; a new one replaces it";
    ProviderUse: "Использовать вместо Gemini", "Use instead of Gemini";
    ProviderSave: "Сохранить", "Save";
    ProviderWorks: "Работает:", "Works:";
    ProviderFails: "Не отвечает:", "No answer:";
    SetSize: "Размер окна", "Window size";
    SizeAuto: "по экрану", "fit the screen";
    Size960: "окно 960×540", "960×540";
    Size1280: "окно 1280×720", "1280×720";
    Size1440: "окно 1440×810", "1440×810";
    Size1600: "окно 1600×900", "1600×900";
    Size1920: "окно 1920×1080", "1920×1080";
    Size2560: "окно 2560×1440", "2560×1440";
    SetFill: "Картинка", "Picture";
    FillScreen: "на весь экран", "fill the screen";
    FillSharp: "чёткие пиксели", "sharp pixels";
    FillHint: "На весь экран — без чёрных полос; чёткие пиксели — одинаковые пиксели, но с полосами по краям", "Fill: no black bars; sharp: every pixel the same size, with bars at the edges";
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
