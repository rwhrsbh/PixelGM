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
    NoSavesYet: "Сохранений пока нет: начни новую игру.", "No saves yet: start a new game.";
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
    ModelsUnknown: "Список моделей появится, когда заработает хотя бы один ключ.", "The model list appears once at least one key works.";
    ModelsLegend: "Выше — спрашивается раньше. Справа — свободные ключи или когда вернётся квота.", "Higher is asked sooner. On the right: keys free, or how soon the quota returns.";
    ModelsReset: "Сброс", "Reset";
    ModelOff: "выкл", "off";
    ModelOffLarge: "Pro · выкл", "Pro · off";
    ModelOffNew: "новая · выкл", "new · off";
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
    LoadAbout: "Щелчок — продолжить с этого места. Сверху — самые свежие.", "Click to go on from there. The newest are at the top.";
    AutoSave: "авто", "auto";
    ManualSave: "вручную", "manual";
    Ago: "назад", "ago";
    NewWorldTitle: "Новый мир", "New world";
    NwStepWorld: "Мир", "World";
    NwStepGame: "Игра", "Game";
    NwStepRules: "Правила", "Rules";
    NwNext: "Далее", "Next";
    NwCancel: "Отмена", "Cancel";
    NwMode: "Режим", "Mode";
    NwStory: "сюжет", "story";
    NwSandbox: "песочница", "sandbox";
    NwModeHint: "Сюжет — у мира есть главное задание и концовка. Песочница — просто живи в мире.", "Story: the world has a main quest and an ending. Sandbox: just live in the world.";
    NwLive: "Живой мир: люди говорят и ходят сами", "Live world: people talk and walk on their own";
    NwDifficulty: "Сложность", "Difficulty";
    NwCasual: "лёгкая", "casual";
    NwHonest: "честная", "honest";
    NwHardcore: "хардкор", "hardcore";
    NwCasualHint: "Поражение — очнёшься с 1 хитом. Сохраняйся когда хочешь.", "Defeat wakes you with 1 hit point. Save whenever you like.";
    NwHonestHint: "Герой может погибнуть. Сохраняйся когда хочешь.", "A hero may die. Save whenever you like.";
    NwHardcoreHint: "Смерть окончательна, одно автосохранение, усталость, враги сильнее.", "Death is final, one automatic save, weariness, stronger foes.";
    NwViolence: "Насилие", "Violence";
    NwSex: "Секс", "Sex";
    NwGraphic: "подробно", "graphic";
    NwExplicit: "откровенно", "explicit";
    NwOffPage: "за кадром", "off the page";
    NwRules: "Правила", "Rules";
    NwMixed: "книга и выдумки мастера", "the book and the master's inventions";
    NwBook: "только книга", "the book only";
    NwLimits: "Лимиты силы по уровню", "Power held to the level";
    NwMethod: "Характеристики", "Ability scores";
    NwEdition: "Свод правил", "Rules edition";
    NwSrd52: "SRD 5.2 (бета)", "SRD 5.2 (beta)";
    NwSuggest: "Подсказки:", "Ideas:";
    NwIdeaDark: "Тёмное фэнтези", "Dark fantasy";
    NwIdeaSea: "Пираты и острова", "Pirates and islands";
    NwIdeaSpace: "Космос", "Space";
    NwIdeaWaste: "Постапокалипсис", "After the fall";
    NwIdeaNoir: "Детектив", "Detective";
    NewWorldAsk: "Каким будет мир? Опиши его парой фраз — или оставь поле пустым, и мастер решит сам.", "What world shall it be? Describe it in a sentence or two, or leave the field empty and the game master will decide.";
    CreateWorld: "Создать мир", "Create the world";
    NeedKey: "Мастеру нужен ключ Gemini. Сначала добавь его.", "The game master needs a Gemini key. Add one first.";
    Creating: "Мастер придумывает и рисует мир", "The game master is inventing and drawing the world";
    CreatingHint: "Это один большой запрос: обычно от полуминуты до двух минут.", "This is one large request: usually half a minute to two minutes.";
    Elapsed: "Прошло", "Elapsed:";
    CreateFailed: "Мир создать не удалось.", "The world could not be created.";
    HudHint: "I сумка C герой J журнал M карта", "I bag C hero J journal M map";
    LobbyName: "Твоё имя", "Your name";
    LobbyJoin: "Подключиться", "Join";
    SteamUnavailable: "Steam не запущен: остаётся подключение по адресу.", "Steam is not running: only a direct address is left.";
    SteamOpen: "Steam: лобби открыто. Друзья увидят игру в этом меню у себя.", "Steam: the lobby is open. Friends will see the game in this menu.";
    LobbyAlready: "Ты уже за столом: сначала отключись.", "You are at a table already: leave it first.";
    AddressFirst: "Сначала впиши адрес хоста.", "Type the host's address first.";
    LobbyNoHero: "без героя", "no hero yet";
    SteamInvite: "Пригласить друзей", "Invite friends";
    SteamAsk: "Друзья в сети — нажми, чтобы позвать:", "Friends online — press to ask:";
    LobbyWrongVersion: "У хоста другая версия игры.", "The host runs another version of the game.";
    LobbyLost: "Связь с хостом потеряна. Игра сохранена — можно продолжить одному.", "The host is gone. The game was saved; you can go on alone.";
    LobbyHostFailed: "Не удалось создать игру: порт занят.", "Could not host: the port is in use.";
    LobbyKicked: "Хост убрал тебя со стола или за ним нет мест.", "The host took you off the table, or it has no seat left.";
    TableChoosing: "выбирают мир", "choosing a world";
    TablePlaying: "идёт игра", "playing";
    KickHint: "Убрать игрока со стола", "Take this player off the table";
    Ready: "готов", "ready";
    NotReady: "не готов", "not ready";
    SeatFree: "Свободное место", "A free seat";
    Invite: "Пригласить", "Invite";
    TableOf: "Стол:", "Table of";
    LobbyAbout: "Твой стол уже открыт: зови друзей на свободные места или зайди за чужой стол справа.", "Your table is open already: ask friends to the free seats, or join another table at the right.";
    LobbyGuestAbout: "Ты за чужим столом: нажми «Готов», а когда хост начнёт, игра откроется сама.", "You are at somebody's table: press Ready, and the game opens by itself when the host begins.";
    Party: "Партия", "Party";
    TableCode: "Код стола", "The table's code";
    Copy: "Копировать", "Copy";
    CopyHint: "Код в буфер обмена — отправь его другу", "The code to the clipboard, to send to a friend";
    OpenTable: "Открыть свой стол", "Open my table";
    FindTable: "Другие столы", "Other tables";
    NoTablesSeen: "Столов в сети не видно. Попроси у хоста код — он у него слева, под партией.", "No table is seen on the network. Ask the host for the code: it is at the left, under the party.";
    OtherVersion: "У этого стола другая версия игры.", "That table runs another version of the game.";
    CodeHint: "Код стола или адрес (LAN, ZeroTier, Radmin). Ctrl+V — вставить.", "A table's code or address (LAN, ZeroTier, Radmin). Ctrl+V pastes.";
    TalkEmpty: "Здесь пишут за столом: Enter — отправить.", "Talk at the table goes here: Enter sends.";
    TalkTitle: "Разговор за столом", "Talk at the table";
    LobbyBackHint: "В меню; стол остаётся открытым", "To the menu; the table stays open";
    LeaveTable: "Покинуть стол", "Leave the table";
    NotReadyAction: "Не готов", "Not ready";
    ReadyAction: "Готов", "Ready";
    BackToGame: "Вернуться в игру", "Back to the game";
    Begin: "Начать", "Begin";
    NotAllReady: "Не все готовы: опоздавшие войдут позже.", "Not everyone is ready: those late come in later.";
    BeginHint: "С чего начнёт стол: новый мир или сохранение. Гости войдут, как только он будет готов.", "Where the table begins: a new world or a save. Guests come in as soon as it is ready.";
    SteamOverlay: "Окно приглашений Steam", "Steam's invite window";
    InviteByCode: "Отправь другу код стола: он впишет его справа, в «Другие столы».", "Send a friend the table's code: they type it at the right, under Other tables.";
    Close: "Закрыть", "Close";
    Rejoining: "Связь с хостом потеряна. Подключаюсь…", "Lost the host. Connecting…";
    LedThere: "Мастер ведёт твоего героя — WASD, чтобы остановиться.", "The master leads your hero — WASD to stop.";
    OfferForOther: "Этот вариант для героя:", "This choice is for the hero";
    PlayAlone: "Играть одному", "Play alone";
    PlayAloneHint: "Продолжить со своей копии мира, без стола", "Go on from your copy of the world, without the table";
    ToMenu: "В меню", "To the menu";
    NetLocal: "Локальная сеть", "Local network";
    NetInternet: "Интернет", "Internet";
    InternetClosed: "Роутер не открыл порт: из интернета войдут только через Steam, ZeroTier или Radmin.", "The router did not open the port: from the internet only Steam, ZeroTier or Radmin get in.";
    LobbyJoinFailed: "Не удалось подключиться.", "Could not connect.";
    TableTalk: "Столу", "To the table";
    WritingHintTable: "Enter — мастеру, это тратит ход. «Столу» — только игрокам: мастер не слышит.", "Enter sends it to the game master and spends a turn. \"To the table\" is for the players only: the master does not hear it.";
    FreeHeroes: "Герои без игрока — можно взять:", "Heroes without a player, free to take:";
    SeatTake: "Сесть на место мастера", "Take the master's seat";
    SeatLeave: "Отдать место модели", "Give the seat back to the model";
    SeatNoHero: "Вести игру без героя", "Run the game without a hero";
    SeatYours: "Ты ведёшь игру. G — панель мастера.", "You run the game. G opens the master's panel.";
    HudMaster: " · G пульт", " · G desk";
    DraftTitle: "Черновик модели", "The model's draft";
    Accept: "Принять", "Accept";
    Reject: "Отклонить", "Reject";
    Redo: "Переделать", "Redo";
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
    ShortRest: "Короткий отдых", "Short rest";
    ShortRested: "Партия перевела дух", "The party has caught its breath";
    NewHero: "Новый герой (прежний погиб)", "A new hero (the old one died)";
    FeatOffer: "★ Новая способность: открой лист героя (C)", "★ A new ability: open the hero sheet (C)";
    FeatChoose: "Выбери одну способность:", "Choose one ability:";

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
    NextLevels: "Новые уровни получать как:", "Take new levels as:";
    NextLevelsHint: "нажми, чтобы сменить класс (мультикласс)", "press to change the class (multiclassing)";
    SheetToNext: "до уровня", "to level";
    SheetTop: "выше расти некуда", "the highest level";
    SheetBody: "Здоровье и защита", "Health and defence";
    SheetSpeed: "Скорость", "Speed";
    SheetFeet: "фт", "ft";
    SheetProficiency: "Бонус мастерства", "Proficiency bonus";
    SheetBodyHint: "КД — что врагу надо выбросить, чтобы попасть. Кости хитов лечат на отдыхе.", "AC is what a foe must roll to hit. Hit dice heal on a short rest.";
    SheetAbilities: "Характеристики (в скобках — прибавка к броскам)", "Abilities (in brackets: what is added to rolls)";
    SheetSkills: "Навыки, которым обучен", "Trained skills";
    SheetNoSkills: "нет", "none";
    SheetWeapons: "Оружие", "Weapons";
    SheetAttack: "атака", "attack";
    SheetDamage: "урон", "damage";
    SheetSpells: "Заклинания", "Spells";
    SheetSpecial: "Особые умения", "Special abilities";
    ProvNimSlow: "Бесплатные модели NIM стоят в очереди и могут отвечать по 10–15 минут. Игра ждёт без ограничения по времени; «Размышления мастера: нет» ниже ускоряют ответ.", "NIM's free models queue and may take 10–15 minutes to answer. The game waits with no time limit; thinking switched off below makes answers sooner.";
    SetVoice: "Голос мастера", "The master's voice";
    VoiceHint: "Мастер читает рассказ вслух. Тратит квоту голосовых моделей, отдельную от квоты мастера.", "The master reads the telling aloud. Spends the quota of the speaking models, separate from the master's own.";
    MasterBusy: "Мастер занят другим вопросом. Подожди ответа.", "The master is busy with another question. Wait for the answer.";
    ChoiceGone: "Этого выбора уже нет: его сделал кто-то другой, или сцена ушла дальше.", "That choice is gone: someone else took it, or the scene has moved on.";
    ActVoidLeft: "Ты ушёл оттуда раньше, чем мастер ответил: задуманное не случилось.", "You left before the master answered: what you meant to do did not happen.";
    ActVoidFight: "Начался бой — задуманное не случилось.", "A fight broke out — what you meant to do did not happen.";
    HeroTitle: "Твой герой", "Your hero";
    HeroAbout: "Имя, народ и класс; внешность и способности — на вкладках. Готово — «В путь».", "A name, a people and a class; the looks and the numbers are on the tabs. Done: Set out.";
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
    GfxAbout: "Слева сразу видно, как выглядит каждая настройка.", "The scene at the left shows at once what each setting looks like.";
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
    BagAbout: "Щелчок — взять вещь, ещё щелчок — положить. Положенная на другую вещь меняется с ней местами.", "Click to pick a thing up, click again to put it down. Put on another thing, it takes that one's place.";
    BagQuickHint: "F1–F6 над вещью — держать её под рукой; потом F1–F6 её применяют.", "F1–F6 over a thing keeps it at hand; later F1–F6 use it.";
    GoldDropHint: "Щелчок — положить 10 монет на землю, чтобы другой их подобрал.", "Click to lay 10 coins on the ground for someone else to pick up.";
    OtherHero: "Сумка другого героя", "Another hero's bag";
    CompanionsAbout: "С вещью в руке: щелчок по руке или спине — отдать оружие или доспех, по ноше — положить. Без вещи — забрать.", "With a thing in hand: click a hand or back to give a weapon or armour, the pack to put it in. With none: take it back.";
    CompanionHand: "Оружие", "Weapon";
    CompanionBack: "Доспех", "Armour";
    ShelfEmpty: "На этой полке ничего нет.", "Nothing lies on this shelf.";
    HaggleDone: "Здесь уже торговались.", "You have already haggled here.";
    CloseBag: "Закрыть", "Close";
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
    ShelfWeapons: "Оружие", "Weapons";
    ShelfArmour: "Броня", "Armour";
    ShelfSupplies: "Припасы", "Supplies";
    ShelfOther: "Прочее", "Other";
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
    MasterDecides: "Пусть решит мастер", "Let the master decide";
    DefeatSaid: "Вся партия пала в бою. Что с нами стало?", "The whole party fell in the fight. What became of us?";
    OwnAction: "Своё действие", "Something else";
    Leave: "Отойти", "Leave";
    WhatDoYouDo: "Что вы делаете?", "What do you do?";
    Say: "Отправить", "Send";
    WritingHint: "Enter — отправить мастеру. Это тратит один ход.", "Enter sends it to the game master. This spends one turn.";
    Onward: "Дальше", "Go on";
    ActionSpent: "Действие на этот ход уже потрачено.", "This turn's action is already spent.";
    NoStepsLeft: "Шагов на этот ход не осталось.", "No steps are left this turn.";
    OthersTurn: "Ходит другой. Ждём своей очереди.", "Someone else acts. Wait for your turn.";
    FightPlaying: "Ход разыгрывается…", "The turn is being played out…";
    SpellsAbout: "Заклинание со звёздочкой судит мастер.", "A spell with a star is judged by the master.";
    ItemsAbout: "Выпить или съесть — тратит действие.", "Drinking or eating spends the action.";
    DodgeWhy: "Атаки по тебе — с помехой до следующего хода.", "Attacks on you have disadvantage until your next turn.";
    DashWhy: "Вдвое больше шагов на этот ход.", "Twice the steps this turn.";
    DisengageWhy: "Уйти, не получив удара вслед.", "Walk away without a parting blow.";
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
    Drawing: "Мастер рисует место:", "The game master is drawing:";
    DrawingHint: "Обычно это занимает до пары минут.", "This usually takes up to a couple of minutes.";
    StuckTitle: "Место не нарисовано", "The place was not drawn";
    Sketch: "Набросать без мастера", "Sketch it without the master";
    StayHere: "Остаться здесь", "Stay here";
    Paused: "Пауза", "Paused";
    CharacterTitle: "Герои", "The heroes";
    SeatTaken: "Место мастера уже занято другим игроком.", "Another player sits in the master's seat.";
    RedrawInFight: "Во время боя место не перерисовать.", "The place cannot be drawn again during a fight.";
    PausedAbout: "Игра стоит. Отсюда — сохранение, отдых, ключи мастера и выход.", "The game stands still. From here: saving, resting, the master's keys and leaving.";
    JournalAbout: "Задание, что у партии с собой, и всё, что было сказано.", "The quest, what the party carries, and everything said so far.";
    CharacterAbout: "Листы героев: характеристики, навыки, умения. Здесь же — очки и новые умения.", "The heroes' sheets: abilities, skills, features. Points and new abilities are placed here.";
    StoryKeys: "1–5 — выбрать · Enter — дальше · H — свернуть · ходить можно", "1–5 choose · Enter go on · H fold · you can walk";
    LookKeys: "1–5 — действие · Esc — отойти", "1–5 act · Esc leave";
    AskingAbout: "Мастер отвечает. Пока можно ходить, открыть сумку или поговорить со столом.", "The master is answering. Meanwhile you can walk, open the bag or talk to the table.";
    StuckAbout: "Мастер не смог нарисовать место. Можно спросить ещё раз, нарисовать набросок или остаться.", "The master could not draw the place. Ask again, have a sketch drawn, or stay.";
    RefusedAbout: "Действие не потеряно: можно спросить ещё раз или проверить ключи.", "Nothing is lost: ask again, or look at the keys.";
    JudgeAbout: "Игроки ещё не видели ответ. Поправь текст и пропусти, отклони или попроси переделать.", "The players have not seen it yet. Edit the text and let it through, throw it away, or ask for it redone.";
    RedoHint: "что переделать…", "what to redo…";
    SayHint: "что делает твой герой…", "what your hero does…";
    MasterFieldHint: "указание модели или параметр кнопки…", "a direction for the model, or a tool's parameter…";
    SaveGame: "Сохранить", "Save";
    Saved: "Сохранено", "Saved";
    ToMainMenu: "В главное меню", "Main menu";
    Hours: "ч", "h";
    Minutes: "мин", "min";
    Seconds: "с", "s";

    SetLanguage: "Язык", "Language";
    SettingsAbout: "Всё меняется сразу и сохраняется само. Значение меняют ◀ ▶, щелчок, колесо или стрелки.", "Everything takes effect at once and is saved by itself. ◀ ▶, a click, the wheel or the arrows change a value.";
    TabGame: "Игра", "Game";
    TabScreen: "Экран", "Screen";
    TabSound: "Звук", "Sound";
    SetScreen: "Экран интерфейса", "Interface for";
    ScreenAuto: "Авто", "Auto";
    ScreenHint: "Под какой экран рисовать меню и надписи. «Авто» — по высоте окна.", "Which screen the menus and words are drawn for. Auto goes by the window's height.";
    SetUiSize: "Размер интерфейса", "Interface size";
    UiSmaller: "Мельче", "Smaller";
    UiNormal: "Обычный", "Normal";
    UiLarger: "Крупнее", "Larger";
    UiSizeHint: "Крупнее или мельче все меню, кнопки и надписи.", "All menus, buttons and words larger or smaller.";
    LanguageHint: "Язык меню и рассказа мастера.", "The language of the menus and of the master's telling.";
    WindowHint: "Во весь экран или в окне. F11 — переключить в любой момент.", "Full screen or in a window. F11 switches at any time.";
    SizeHint: "Размер окна, когда игра не во весь экран.", "The window's size when not in full screen.";
    MusicHint: "Громкость музыки.", "How loud the music is.";
    SoundHint: "Громкость звуков: шаги, удары, кнопки.", "How loud the sounds are: steps, blows, buttons.";
    LanguageName: "Русский", "English";
    SetWindow: "Окно", "Window";
    SetOnlineMusic: "Музыка из сети", "Music from the network";
    OnlineMusicHint: "Сотни треков (Kevin MacLeod, CC BY 4.0; OpenGameArt, CC0) подкачиваются по настроению и хранятся на диске; без сети играет скачанное", "Hundreds of pieces (Kevin MacLeod, CC BY 4.0; OpenGameArt, CC0) are fetched by mood and kept on disk; offline, what was fetched plays";
    KeysOutAgain: "Ключи исчерпаны. Мастер спросит снова через", "The keys are spent. The master asks again in";
    KeysOutSeconds: "с", "s";
    KeysOutTitle: "Ключи мастера исчерпаны", "The master's keys are spent";
    KeysOutText: "Ни у кого за столом нет ключа, которым мастер мог бы ответить. Можно подождать, пока квота вернётся, или ввести новый ключ. Игра продолжится, когда решат все.", "Nobody at the table has a key the master could answer with. Wait for the quota to come back, or enter a new key. The game goes on once everyone has decided.";
    KeysOutWait: "Подождать", "Wait";
    KeysOutEnter: "Ввести ключ", "Enter a key";
    KeysOutChosen: "Твой выбор принят.", "Your choice is taken.";
    KeysOutWaiting: "Ждём:", "Waiting for:";
    MasterGemini: "Мастер: Gemini", "Master: Gemini";
    MasterNim: "Мастер: NVIDIA NIM", "Master: NVIDIA NIM";
    MasterOpenRouter: "Мастер: OpenRouter", "Master: OpenRouter";
    MasterCloudflare: "Мастер: Cloudflare", "Master: Cloudflare";
    MasterCustom: "Мастер: свой сервер", "Master: own server";
    MasterOpenCodeGo: "Мастер: OpenCode Go", "Master: OpenCode Go";
    RunsGame: "● Этот мастер ведёт игру", "● This master runs the game";
    MakeMaster: "Сделать мастером", "Make this the master";
    MakeMasterHint: "Мастер один: прежний перестанет вести игру.", "One master at a time: the one before stops.";
    SectionKeys: "Ключи", "Keys";
    SectionModels: "Модели — сверху спрашиваются раньше", "Models — the higher, the sooner asked";
    SectionAnswer: "Как отвечает мастер", "How the master answers";
    ProvKeysHelp: "Ключи берутся по очереди; можно вставить несколько через пробел.", "The keys take turns; several may be pasted at once, parted by spaces.";
    GoKeysHelp: "Ключ — в консоли OpenCode, раздел Go; нужна подписка Go. Затем «Загрузить список» и выбор моделей.", "The key is in the OpenCode console under Go; a Go subscription is needed. Then Fetch the list and pick models.";
    GoUrlHint: "Пусто — адрес по умолчанию. Впиши свой, если OpenCode его сменит.", "Empty: the usual address. Type another if OpenCode moves it.";
    ModelOn: "вкл", "on";
    ModelOffShort: "выкл", "off";
    ModelUpHint: "Спрашивать раньше", "Ask sooner";
    ModelDownHint: "Спрашивать позже", "Ask later";
    ModelDropHint: "Убрать из списка", "Take off the list";
    ModelSwitchHint: "Не спрашивать эту модель или снова спрашивать", "Stop or start asking this model";
    MasterSettings: "Мастер (ИИ)…", "Master (AI)…";
    AiTitle: "Мастер (ИИ)", "Master (AI)";
    KeysAbout: "Мастер ходит по ключам по очереди. Модели спрашиваются сверху вниз: ▲ ▼ — порядок, «выкл» — не спрашивать.", "The master takes the keys in turn. Models are asked from the top down: ▲ ▼ set the order, “off” stops asking one.";
    ProviderAbout: "Ключи берутся по очереди, модели пробуются сверху вниз. Всё сохраняется само.", "The keys take turns, the models are tried from the top down. Everything is saved by itself.";
    KeyTypeFirst: "Сначала вставь ключ в поле.", "Paste a key in the field first.";
    MasterMark: "● — этот мастер ведёт игру.", "● — this master runs the game.";
    ModelTypeFirst: "Сначала впиши или выбери модель.", "Type or pick a model first.";
    KeysProvider: "Ключи и модели провайдера", "The provider's keys and models";
    ProvAccount: "ID аккаунта", "Account ID";
    ProvChat: "Модели мастера", "Master's models";
    ProvVoice: "Голос диктора", "Narrator's voice";
    ProvVoiceName: "Голос", "Voice";
    ProvNone: "Ничего не выбрано", "Nothing chosen";
    ProvAdd: "Добавить", "Add";
    ProvNoMatch: "В списке нет моделей с таким текстом.", "No model on offer has that in its name.";
    ProvFetch: "Загрузить список", "Fetch the list";
    ProvFetching: "Загружаю список моделей…", "Fetching the models…";
    ProvOffered: "Моделей в списке:", "Models on offer:";
    ProvFetchHint: "Нажми «Загрузить список», затем выбери модели; текст в поле сужает список.", "Press Fetch the list, then pick models; what is typed narrows the list.";
    ProviderUrl: "Адрес", "Address";
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
    SetFirstToken: "Ждать первый токен", "Wait for the first token";
    FirstTokenHint: "Сколько секунд терпеть молчание модели, прежде чем спросить следующую. От 10 до 300.", "How many seconds of a model's silence to bear before asking the next one. From 10 to 300.";
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
