//! Starting a game: the host says what world they want, and waits while the game master
//! invents and draws it.

use crate::art::Art;
use crate::coop::Session;
use crate::gm::{Gm, Health, KeyBook, Trouble, trouble_text};
use crate::i18n::{Lang, T};
use crate::master::{Answer, Ask, Master};
use crate::menu::{Nav, ReturnTo, Screen, screen_root, spawn_panel};
use crate::play;
use crate::store::{Settings, Store};
use crate::theme::*;
use crate::widgets::{Activate, backing, button};
use bevy::input_focus::AutoFocus;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigation;

#[derive(Component, Clone, Copy)]
enum Action {
    Create,
    Retry,
    Keys,
    Violence,
    Sex,
    Book,
    Limits,
    Method,
    Edition,
    Story,
    Live,
    Difficulty,
}

#[derive(Component)]
struct WishInput;

#[derive(Component)]
struct StatusLine;

#[derive(Component)]
struct FailureButtons;

/// The world being made: what was asked for, how long it has taken, and what went wrong.
#[derive(Resource, Default)]
struct Making {
    request: String,
    seconds: f32,
    trouble: Option<Trouble>,
    /// The table's content switches; both are on unless the host turns them off here.
    no_violence: bool,
    no_sex: bool,
    /// How the table plays: by the book or with inventions, with caps or without, and how
    /// ability scores are made.
    table: world::TableRules,
}

fn book_label(making: &Making) -> T {
    if making.table.book_only { T::TableBook } else { T::TableMixed }
}

fn limits_label(making: &Making) -> T {
    if making.table.no_limits { T::LimitsOff } else { T::LimitsOn }
}

fn edition_label(making: &Making) -> T {
    if making.table.srd_2024 { T::Edition52 } else { T::Edition51 }
}

fn method_label(making: &Making) -> T {
    match making.table.method {
        rules::AbilityMethod::PointBuy => T::MethodPointBuy,
        rules::AbilityMethod::StandardArray => T::MethodArray,
        rules::AbilityMethod::Roll => T::MethodRoll,
    }
}

fn story_label(making: &Making) -> T {
    if making.table.sandbox { T::ModeSandbox } else { T::ModeStory }
}

fn live_label(making: &Making) -> T {
    if making.table.live { T::LiveOn } else { T::LiveOff }
}

fn difficulty_label(making: &Making) -> T {
    match making.table.difficulty {
        world::Difficulty::Casual => T::DifficultyCasual,
        world::Difficulty::Honest => T::DifficultyHonest,
        world::Difficulty::Hardcore => T::DifficultyHardcore,
    }
}

fn violence_label(making: &Making) -> T {
    if making.no_violence { T::ViolenceOff } else { T::ViolenceOn }
}

fn sex_label(making: &Making) -> T {
    if making.no_sex { T::SexOff } else { T::SexOn }
}

const FIELD_BORDER: f32 = 4.0;

/// Whether there is anyone to draw a world: a key of the host's, or guests whose keys can
/// be asked.
fn can_ask(master: &Master, book: &KeyBook, session: &Session, settings: &Settings) -> bool {
    master.fake || !settings.providers().is_empty() || book.entries.iter().any(|e| e.health == Health::Working || e.health == Health::Checking) || !session.players().is_empty()
}

fn spawn_form(mut commands: Commands, art: Res<Art>, fonts: Res<Fonts>, book: Res<KeyBook>, master: Res<Master>, making: Res<Making>, session: Res<Session>, settings: Res<Settings>) {
    build_form(&mut commands, &art, &fonts, &making, can_ask(&master, &book, &session, &settings));
}

/// A key may start working, or a guest sit down, while the form is open: the form is drawn
/// again, keeping what was typed.
#[allow(clippy::too_many_arguments)]
fn refresh_form(
    mut commands: Commands,
    art: Res<Art>,
    fonts: Res<Fonts>,
    book: Res<KeyBook>,
    master: Res<Master>,
    mut making: ResMut<Making>,
    session: Res<Session>,
    roots: Query<Entity, With<DespawnOnExit<Screen>>>,
    typed: Query<&EditableText, With<WishInput>>,
    fresh: Query<(), Added<WishInput>>,
    mut built: Local<Option<bool>>,
    settings: Res<Settings>,
) {
    let now = can_ask(&master, &book, &session, &settings);
    if !fresh.is_empty() || *built == Some(now) {
        *built = Some(now);
        return;
    }
    *built = Some(now);
    if let Ok(typed) = typed.single() {
        making.request = typed.value().to_string();
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    build_form(&mut commands, &art, &fonts, &making, now);
}

fn build_form(commands: &mut Commands, art: &Art, fonts: &Fonts, making: &Making, can_ask: bool) {
    let ink = TextColor(color(INK));
    commands.spawn(screen_root(Screen::NewGame)).with_children(|root| {
        let sheet = Node { margin: UiRect::top(px(14)), width: px(420), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() };
        spawn_panel(root, art, sheet, |sheet| {
            sheet.spawn((Text::default(), T::NewWorldTitle, fonts.title(1), ink));
            sheet.spawn((Text::default(), T::NewWorldAsk, fonts.body(), ink));
            sheet.spawn(Node { height: px(48), padding: UiRect::all(px(5)), overflow: Overflow::clip(), ..default() }).with_children(|field| {
                field.spawn(backing(art.field.clone(), FIELD_BORDER));
                field.spawn((
                    WishInput,
                    AutoFocus,
                    AutoDirectionalNavigation::default(),
                    Node { width: percent(100), height: percent(100), ..default() },
                    // The last wish is kept, so going back to add a key does not lose it.
                    EditableText { allow_newlines: false, max_characters: Some(400), ..EditableText::new(&making.request) },
                    TextLayout { linebreak: LineBreak::WordOrCharacter, ..default() },
                    fonts.body(),
                    ink,
                    TextCursorStyle { color: color(INK), selection_color: color(BRASS_LIGHT), ..default() },
                ));
            });
            sheet.spawn(Node { column_gap: px(6), ..default() }).with_children(|line| {
                button(line, art, fonts, violence_label(making), true, Action::Violence);
                button(line, art, fonts, sex_label(making), true, Action::Sex);
            });
            sheet.spawn(Node { column_gap: px(6), row_gap: px(3), flex_wrap: FlexWrap::Wrap, ..default() }).with_children(|line| {
                button(line, art, fonts, story_label(making), true, Action::Story);
                button(line, art, fonts, live_label(making), true, Action::Live);
                button(line, art, fonts, difficulty_label(making), true, Action::Difficulty);
            });
            sheet.spawn(Node { column_gap: px(6), row_gap: px(3), flex_wrap: FlexWrap::Wrap, ..default() }).with_children(|line| {
                button(line, art, fonts, book_label(making), true, Action::Book);
                button(line, art, fonts, limits_label(making), true, Action::Limits);
                button(line, art, fonts, method_label(making), true, Action::Method);
                button(line, art, fonts, edition_label(making), true, Action::Edition);
            });
            if !can_ask {
                sheet.spawn((Text::default(), T::NeedKey, fonts.body(), TextColor(color(WAX))));
            }
            sheet.spawn(Node { column_gap: px(6), ..default() }).with_children(|line| {
                button(line, art, fonts, T::Back, true, Nav::Back);
                button(line, art, fonts, T::CreateWorld, can_ask, Action::Create);
                if !can_ask {
                    button(line, art, fonts, T::Keys, true, Action::Keys);
                }
            });
        });
    });
}

fn spawn_waiting(mut commands: Commands, art: Res<Art>, fonts: Res<Fonts>) {
    commands.spawn(screen_root(Screen::Creating)).with_children(|root| {
        let sheet = Node { margin: UiRect::top(px(70)), width: px(300), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() };
        spawn_panel(root, &art, sheet, |sheet| {
            sheet.spawn((Text::default(), T::NewWorldTitle, fonts.title(1), TextColor(color(INK))));
            sheet.spawn((Text::default(), StatusLine, fonts.body(), TextColor(color(INK))));
            sheet.spawn((FailureButtons, Node { column_gap: px(6), display: Display::None, ..default() })).with_children(|line| {
                button(line, &art, &fonts, T::Back, true, Nav::Go(Screen::NewGame));
                button(line, &art, &fonts, T::Recheck, true, Action::Retry);
                button(line, &art, &fonts, T::Keys, true, Action::Keys);
            });
        });
    });
}

fn start_making(making: &mut Making, request: String, gm: &Gm, store: &Store, settings: &Settings, master: &Master) {
    *making = Making { request: request.clone(), seconds: 0.0, trouble: None, no_violence: making.no_violence, no_sex: making.no_sex, table: making.table };
    master.ask(gm, store, settings, Ask::NewWorld { request, story: !making.table.sandbox });
}

#[allow(clippy::too_many_arguments)]
fn on_activate(
    used: On<Activate>,
    buttons: Query<&Action>,
    input: Query<&EditableText, With<WishInput>>,
    screen: Res<State<Screen>>,
    mut making: ResMut<Making>,
    mut return_to: ResMut<ReturnTo>,
    mut next: ResMut<NextState<Screen>>,
    gm: Res<Gm>,
    store: Res<Store>,
    settings: Res<Settings>,
    master: Res<Master>,
) {
    let Ok(action) = buttons.get(used.entity) else { return };
    match action {
        Action::Create => {
            let request = input.single().map(|i| i.value().to_string()).unwrap_or_default();
            start_making(&mut making, request.trim().to_string(), &gm, &store, &settings, &master);
            next.set(Screen::Creating);
        }
        Action::Retry => {
            let request = making.request.clone();
            start_making(&mut making, request, &gm, &store, &settings, &master);
        }
        Action::Violence => making.no_violence = !making.no_violence,
        Action::Sex => making.no_sex = !making.no_sex,
        Action::Book => making.table.book_only = !making.table.book_only,
        Action::Limits => making.table.no_limits = !making.table.no_limits,
        Action::Edition => making.table.srd_2024 = !making.table.srd_2024,
        Action::Story => making.table.sandbox = !making.table.sandbox,
        Action::Live => making.table.live = !making.table.live,
        Action::Difficulty => {
            making.table.difficulty = match making.table.difficulty {
                world::Difficulty::Casual => world::Difficulty::Honest,
                world::Difficulty::Honest => world::Difficulty::Hardcore,
                world::Difficulty::Hardcore => world::Difficulty::Casual,
            }
        }
        Action::Method => {
            making.table.method = match making.table.method {
                rules::AbilityMethod::PointBuy => rules::AbilityMethod::StandardArray,
                rules::AbilityMethod::StandardArray => rules::AbilityMethod::Roll,
                rules::AbilityMethod::Roll => rules::AbilityMethod::PointBuy,
            }
        }
        Action::Keys => {
            // Keep what was typed, and come back to this form afterwards.
            if let Ok(input) = input.single() {
                making.request = input.value().to_string();
            }
            return_to.0 = Some(if *screen.get() == Screen::Creating { Screen::NewGame } else { *screen.get() });
            next.set(Screen::Keys);
        }
    }
}

/// The switch buttons say what they are set to.
fn show_switches(making: Res<Making>, buttons: Query<(&Action, &Children)>, mut labels: Query<&mut T>) {
    if !making.is_changed() {
        return;
    }
    for (action, children) in &buttons {
        let wanted = match action {
            Action::Violence => violence_label(&making),
            Action::Sex => sex_label(&making),
            Action::Book => book_label(&making),
            Action::Limits => limits_label(&making),
            Action::Edition => edition_label(&making),
            Action::Method => method_label(&making),
            _ => continue,
        };
        for child in children {
            if let Ok(mut label) = labels.get_mut(*child)
                && *label != wanted
            {
                *label = wanted;
            }
        }
    }
}

/// Dots that come and go, so a long wait visibly is not a freeze.
fn dots(seconds: f32) -> &'static str {
    ["", ".", "..", "..."][(seconds * 2.0) as usize % 4]
}

#[allow(clippy::too_many_arguments)]
fn wait_for_world(
    mut commands: Commands,
    time: Res<Time>,
    master: Res<Master>,
    gm: Res<Gm>,
    lang: Res<Lang>,
    mut making: ResMut<Making>,
    mut next: ResMut<NextState<Screen>>,
    mut status: Query<(&mut Text, &mut TextColor), With<StatusLine>>,
    mut buttons: Query<&mut Node, With<FailureButtons>>,
) {
    for answer in master.take() {
        let Answer::World(result) = answer else { continue };
        match result {
            Ok((mut world, notes)) => {
                for note in notes {
                    info!("world repaired: {note}");
                }
                world.violence = !making.no_violence;
                world.sex = !making.no_sex;
                world.table = making.table;
                let world_id = format!("{:x}", crate::store::unix_now());
                play::begin(&mut commands, &mut next, *world, world_id);
                return;
            }
            Err(trouble) => making.trouble = Some(trouble),
        }
    }
    if making.trouble.is_none() {
        making.seconds += time.delta_secs();
    }
    let (text, ink) = match &making.trouble {
        Some(trouble) => (format!("{}\n{}", T::CreateFailed.text(*lang), trouble_text(trouble, gm.dispatcher.now(), *lang)), WAX),
        None => (format!("{}{}\n{}\n{} {} {}", T::Creating.text(*lang), dots(making.seconds), T::CreatingHint.text(*lang), T::Elapsed.text(*lang), making.seconds as u32, T::Seconds.text(*lang)), INK),
    };
    for (mut line, mut line_color) in &mut status {
        if line.0 != text {
            line.0 = text.clone();
            line_color.0 = color(ink);
        }
    }
    let display = if making.trouble.is_some() { Display::Flex } else { Display::None };
    for mut node in &mut buttons {
        if node.display != display {
            node.display = display;
        }
    }
}

pub struct NewGamePlugin;

impl Plugin for NewGamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Making>()
            .add_systems(OnEnter(Screen::NewGame), spawn_form)
            .add_systems(OnEnter(Screen::Creating), spawn_waiting)
            .add_systems(Update, (wait_for_world.run_if(in_state(Screen::Creating)), (show_switches, refresh_form).run_if(in_state(Screen::NewGame))))
            .add_observer(on_activate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dots_cycle_twice_a_second_and_start_empty() {
        assert_eq!([dots(0.0), dots(0.5), dots(1.0), dots(1.5), dots(2.0)], ["", ".", "..", "...", ""]);
    }
}
