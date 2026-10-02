//! The key screen: enter Gemini keys, see which ones work, and watch how much of each model
//! is left.

use crate::art::Art;
use crate::gm::{Gm, GmTest, Health, KeyBook, masked, trouble_text};
use crate::i18n::{Lang, T, wait_text};
use crate::menu::{Nav, Screen, screen_root, spawn_panel};
use crate::store::Settings;
use crate::theme::*;
use crate::widgets::{Activate, BUTTON_HEIGHT, backing, button, scroll_list};
use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigation;
use llm::{Availability, KeyId, Tier};

#[derive(Component, Clone, Copy)]
enum KeyAction {
    Add,
    Remove(KeyId),
    Recheck(KeyId),
    TestGm,
}

#[derive(Component)]
struct TestResult;

#[derive(Component)]
struct KeyInput;

#[derive(Component)]
struct KeyList;

#[derive(Component)]
struct ModelList;

const FIELD_BORDER: f32 = 4.0;
const KEY_ROW_GAP: f32 = 2.0;
/// Keys beyond this many scroll, so the sheet keeps fitting the screen.
const VISIBLE_KEY_ROWS: f32 = 4.0;
/// Where a player gets a key. An address, so it is the same in every language.
const KEY_PAGE: &str = "aistudio.google.com/apikey";

fn model_status(a: Availability, now: u64, lang: Lang) -> String {
    match (a.live_keys, a.free_keys, a.next_free) {
        (0, _, _) => "—".to_string(),
        (_, 0, Some(until)) => wait_text(until.saturating_sub(now), lang),
        (live, free, _) => format!("{free}/{live}"),
    }
}

fn column(gap: f32) -> Node {
    Node { flex_direction: FlexDirection::Column, row_gap: px(gap), ..default() }
}

fn row(gap: f32) -> Node {
    Node { align_items: AlignItems::Center, column_gap: px(gap), ..default() }
}

fn spawn_keys(mut commands: Commands, art: Res<Art>, fonts: Res<Fonts>) {
    let ink = TextColor(color(INK));
    commands.spawn(screen_root(Screen::Keys)).with_children(|root| {
        let sheet = Node { margin: UiRect::top(px(12)), width: px(452), ..column(6.0) };
        spawn_panel(root, &art, sheet, |sheet| {
            sheet.spawn((Text::default(), T::Keys, fonts.title(1), ink));
            sheet.spawn(Node { column_gap: px(12), align_items: AlignItems::FlexStart, ..default() }).with_children(|columns| {
                columns.spawn(Node { width: px(216), flex_shrink: 0.0, ..column(5.0) }).with_children(|left| {
                    left.spawn((Text::default(), T::KeysHelp, fonts.body(), ink));
                    left.spawn((Text::new(KEY_PAGE), fonts.body(), TextColor(color(WAX_DARK))));
                    left.spawn(row(4.0)).with_children(|line| {
                        line.spawn(Node {
                            flex_grow: 1.0,
                            height: px(16),
                            padding: UiRect::axes(px(5), px(0)),
                            align_items: AlignItems::Center,
                            overflow: Overflow::clip(),
                            ..default()
                        })
                        .with_children(|field| {
                            field.spawn(backing(art.field.clone(), FIELD_BORDER));
                            field.spawn((
                                KeyInput,
                                AutoFocus,
                                AutoDirectionalNavigation::default(),
                                Node { width: percent(100), ..default() },
                                EditableText { allow_newlines: false, max_characters: Some(200), ..default() },
                                TextLayout::no_wrap(),
                                fonts.body(),
                                ink,
                                TextCursorStyle { color: color(INK), selection_color: color(BRASS_LIGHT), ..default() },
                            ));
                        });
                        button(line, &art, &fonts, T::Add, true, KeyAction::Add);
                    });
                    left.spawn((Text::default(), T::KeysInputHint, fonts.body(), TextColor(color(FADED))));
                    left.spawn((scroll_list(column(KEY_ROW_GAP), VISIBLE_KEY_ROWS * (BUTTON_HEIGHT + KEY_ROW_GAP)), KeyList));
                });
                columns.spawn(Node { flex_grow: 1.0, ..column(3.0) }).with_children(|right| {
                    right.spawn((Text::default(), T::ModelsTitle, fonts.body(), TextColor(color(WAX_DARK))));
                    right.spawn((Text::default(), T::ModelsLegend, fonts.body(), TextColor(color(FADED))));
                    right.spawn((column(1.0), ModelList));
                });
            });
            sheet.spawn(row(6.0)).with_children(|line| {
                button(line, &art, &fonts, T::Back, true, Nav::Back);
                button(line, &art, &fonts, T::TestGm, true, KeyAction::TestGm);
                line.spawn((Node { flex_grow: 1.0, flex_basis: px(0), ..default() }, Text::default(), TestResult, fonts.body(), ink));
            });
        });
    });
}

fn health_text(health: &Health) -> (T, Rgba) {
    match health {
        Health::Checking => (T::KeyChecking, FADED),
        Health::Working => (T::KeyWorking, MOSS),
        Health::Rejected => (T::KeyRejected, WAX),
        Health::Unreachable => (T::KeyUnreachable, WAX),
    }
}

/// The list of keys is redrawn only when it changes: redrawing it under the cursor would
/// swallow a click that is half way through.
fn redraw_keys(
    mut commands: Commands,
    art: Res<Art>,
    fonts: Res<Fonts>,
    book: Res<KeyBook>,
    list: Query<Entity, With<KeyList>>,
    fresh: Query<(), Added<KeyList>>,
) {
    let Ok(list) = list.single() else { return };
    if !book.is_changed() && fresh.is_empty() {
        return;
    }
    commands.entity(list).despawn_children().with_children(|rows| {
        if book.entries.is_empty() {
            rows.spawn((Text::default(), T::NoKeys, fonts.body(), TextColor(color(WAX))));
        }
        for entry in &book.entries {
            let (status, status_color) = health_text(&entry.health);
            rows.spawn(row(4.0)).with_children(|line| {
                line.spawn((Node { width: px(64), flex_shrink: 0.0, ..default() }, Text::new(masked(&entry.secret)), TextLayout::no_wrap(), fonts.body(), TextColor(color(INK))));
                line.spawn((Node { flex_grow: 1.0, ..default() }, Text::default(), status, fonts.body(), TextColor(color(status_color))));
                if entry.health == Health::Unreachable {
                    button(line, &art, &fonts, T::Recheck, true, KeyAction::Recheck(entry.id));
                }
                button(line, &art, &fonts, T::Remove, true, KeyAction::Remove(entry.id));
            });
        }
    });
}

/// The meter has nothing to click, so it can refresh every second and count rests down.
fn redraw_models(
    mut commands: Commands,
    fonts: Res<Fonts>,
    gm: Res<Gm>,
    book: Res<KeyBook>,
    lang: Res<Lang>,
    settings: Res<Settings>,
    time: Res<Time>,
    mut since: Local<f32>,
    list: Query<Entity, With<ModelList>>,
    fresh: Query<(), Added<ModelList>>,
) {
    let Ok(list) = list.single() else { return };
    *since += time.delta_secs();
    if *since < 1.0 && !book.is_changed() && !lang.is_changed() && fresh.is_empty() {
        return;
    }
    *since = 0.0;
    let now = gm.dispatcher.now();
    let models: Vec<(String, Tier, Availability)> =
        gm.dispatcher.with_pool(|pool| pool.chain().iter().map(|m| (m.id.clone(), m.tier, pool.availability(now, &m.id))).collect());
    commands.entity(list).despawn_children().with_children(|rows| {
        if models.is_empty() {
            rows.spawn((Text::default(), T::ModelsUnknown, fonts.body(), TextColor(color(FADED))));
        }
        for (id, tier, availability) in models {
            // Models below the chosen fallback tier are listed, but dimmed: they will not be used.
            let allowed = tier <= settings.lowest_tier;
            let name = if allowed { INK } else { FADED };
            let state = if !allowed {
                PARCHMENT_DARK
            } else if availability.free_keys > 0 {
                MOSS
            } else {
                WAX
            };
            rows.spawn(Node { justify_content: JustifyContent::SpaceBetween, column_gap: px(6), ..default() }).with_children(|line| {
                line.spawn((Text::new(id), fonts.body(), TextColor(color(name))));
                line.spawn((Text::new(model_status(availability, now, *lang)), fonts.body(), TextColor(color(state))));
            });
        }
    });
}

fn add_from_input(gm: &Gm, book: &mut KeyBook, input: &mut EditableText) {
    let typed = input.value().to_string();
    // A duplicate stays in the field so the player can see what was refused.
    if gm.add_key(book, &typed) {
        input.clear();
    }
}

/// What the last test of the game master came to, in words and in the colour of the news.
fn test_text(test: &GmTest, now: u64, lang: Lang) -> (String, Rgba) {
    match test {
        GmTest::Idle => (T::TestGmHint.text(lang).to_string(), FADED),
        GmTest::Running => (T::TestRunning.text(lang).to_string(), FADED),
        GmTest::Answered { model, seconds, text } => (format!("{model} {} {seconds:.1} {}: {text}", T::TestAnswered.text(lang), T::Seconds.text(lang)), MOSS),
        GmTest::Failed(trouble) => (trouble_text(trouble, now, lang), WAX),
    }
}

fn show_test(gm: Res<Gm>, test: Res<GmTest>, lang: Res<Lang>, mut line: Query<(&mut Text, &mut TextColor), With<TestResult>>, fresh: Query<(), Added<TestResult>>) {
    if !test.is_changed() && !lang.is_changed() && fresh.is_empty() {
        return;
    }
    let (text, ink) = test_text(&test, gm.dispatcher.now(), *lang);
    for (mut line, mut line_color) in &mut line {
        line.0 = text.clone();
        line_color.0 = color(ink);
    }
}

fn on_activate(
    used: On<Activate>,
    buttons: Query<&KeyAction>,
    gm: Res<Gm>,
    settings: Res<Settings>,
    mut book: ResMut<KeyBook>,
    mut test: ResMut<GmTest>,
    mut input: Query<&mut EditableText, With<KeyInput>>,
) {
    let Ok(action) = buttons.get(used.entity) else { return };
    match *action {
        KeyAction::Add => {
            if let Ok(mut input) = input.single_mut() {
                add_from_input(&gm, &mut book, &mut input);
            }
        }
        KeyAction::Remove(id) => gm.remove_key(&mut book, id),
        KeyAction::Recheck(id) => gm.recheck(&mut book, id),
        KeyAction::TestGm => gm.test(&mut test, &settings),
    }
}

fn submit_on_enter(
    keyboard: Res<ButtonInput<KeyCode>>,
    focus: Res<InputFocus>,
    gm: Res<Gm>,
    mut book: ResMut<KeyBook>,
    mut input: Query<(Entity, &mut EditableText), With<KeyInput>>,
) {
    if !keyboard.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
        return;
    }
    let Ok((entity, mut input)) = input.single_mut() else { return };
    if focus.get() == Some(entity) {
        add_from_input(&gm, &mut book, &mut input);
    }
}

pub struct KeysPlugin;

impl Plugin for KeysPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::Keys), spawn_keys)
            .add_systems(Update, (submit_on_enter, redraw_keys, redraw_models, show_test).chain().run_if(in_state(Screen::Keys)))
            .add_observer(on_activate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gm::Trouble;

    #[test]
    fn test_result_names_the_model_or_says_what_went_wrong() {
        let answered = GmTest::Answered { model: "gemini-9-flash".into(), seconds: 4.26, text: "Добро пожаловать.".into() };
        assert_eq!(test_text(&answered, 0, Lang::Ru), ("gemini-9-flash ответил за 4.3 с: Добро пожаловать.".to_string(), MOSS));
        let spent = GmTest::Failed(Trouble::Exhausted { retry_at: 3_700_000 });
        assert_eq!(test_text(&spent, 100_000, Lang::En).0, "Every allowed model is resting. The first returns in 1 h 0 min.");
        assert_eq!(test_text(&GmTest::Failed(Trouble::NoKeys), 0, Lang::En), ("No working keys.".to_string(), WAX));
        assert_eq!(test_text(&GmTest::Idle, 0, Lang::En).0, "One real request with the current settings. Spends one turn.");
    }

    #[test]
    fn model_status_covers_no_keys_free_and_resting() {
        let free = Availability { free_keys: 2, live_keys: 3, next_free: Some(50) };
        assert_eq!(model_status(free, 0, Lang::En), "2/3");
        let resting = Availability { free_keys: 0, live_keys: 3, next_free: Some(190_000) };
        assert_eq!(model_status(resting, 10_000, Lang::En), "3 min");
        assert_eq!(model_status(Availability::default(), 0, Lang::Ru), "—");
    }
}
