//! The key screen: enter Gemini keys, see which ones work, and watch how much of each model
//! is left.

use crate::art::Art;
use crate::gm::{Gm, GmTest, Health, KeyBook, masked, trouble_text};
use crate::i18n::{Lang, T, wait_text};
use crate::menu::{Nav, Screen, screen_root, spawn_panel};
use crate::store::Settings;
use crate::theme::*;
use crate::widgets::{Activate, BUTTON_HEIGHT, backing, button, scroll_list, text_button};
use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigation;
use llm::{Availability, Class, KeyId, Tier};

#[derive(Component, Clone, Copy)]
enum KeyAction {
    Add,
    Remove(KeyId),
    Recheck(KeyId),
    TestGm,
    /// To another master's keys and models.
    Provider(llm::providers::Kind),
    /// Gemini runs the game again.
    UseGemini,
}

#[derive(Component)]
struct MasterLine;

fn master_line(settings: Res<Settings>, lang: Res<Lang>, mut lines: Query<&mut Text, With<MasterLine>>) {
    let who = settings.active_provider().map_or("Gemini", |kind| kind.name());
    let said = format!("{} {who}", T::MasterNow.text(*lang));
    for mut text in &mut lines {
        if text.0 != said {
            text.0 = said.clone();
        }
    }
}

#[derive(Component)]
struct TestResult;

#[derive(Component)]
struct KeyInput;

#[derive(Component)]
struct KeyList;

#[derive(Component)]
struct ModelList;

/// A model's line in the list; pressing it picks the model for the buttons underneath.
#[derive(Component)]
struct ModelRow(String);

/// The words at the right of a model's line: they change by the second, the line does not.
#[derive(Component)]
struct ModelState(String);

/// What the buttons under the list do with the model that is picked.
#[derive(Component, Clone, Copy)]
enum ModelAct {
    Up,
    Down,
    Switch,
    /// Back to the game's own order, with nothing switched off.
    Reset,
}

/// The model the player last pressed in the list.
#[derive(Resource, Default)]
struct Picked(Option<String>);

/// A model as its line shows it.
#[derive(Clone, PartialEq, Debug)]
struct ModelLine {
    id: String,
    /// Its place in the chain, from one; none for a model that is not asked.
    place: Option<usize>,
    class: Class,
    tier: Tier,
}

/// The lines of the list: the chain in the order it is walked, then everything the keys
/// list that is not in it.
fn model_lines(chain: &[llm::Model], survey: &[llm::Surveyed]) -> Vec<ModelLine> {
    let class = |id: &str| survey.iter().find(|s| s.model.id == id).map_or(Class::Flash, |s| s.class);
    let mut lines: Vec<ModelLine> = chain.iter().enumerate().map(|(n, m)| ModelLine { id: m.id.clone(), place: Some(n + 1), class: class(&m.id), tier: m.tier }).collect();
    lines.extend(survey.iter().filter(|s| !chain.iter().any(|m| m.id == s.model.id)).map(|s| ModelLine { id: s.model.id.clone(), place: None, class: s.class, tier: s.model.tier }));
    lines
}

const FIELD_BORDER: f32 = 4.0;
const KEY_ROW_GAP: f32 = 2.0;
/// Keys beyond this many scroll, so the sheet keeps fitting the screen.
const VISIBLE_KEY_ROWS: f32 = 4.0;
/// Where a player gets a key. An address, so it is the same in every language.
const KEY_PAGE: &str = "aistudio.google.com/apikey";
/// Models beyond what fits in this scroll, so the buttons under the list stay on the sheet.
const MODEL_LIST_HEIGHT: f32 = 92.0;

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
            sheet.spawn(Node { column_gap: px(4), align_items: AlignItems::Center, ..default() }).with_children(|tabs| {
                // The tab one is on: pressing it does nothing.
                text_button(tabs, &art, &fonts, "[Gemini]".to_string(), ());
                for kind in llm::providers::Kind::ALL {
                    text_button(tabs, &art, &fonts, kind.name().to_string(), KeyAction::Provider(kind));
                }
            });
            sheet.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                line.spawn((Text::default(), MasterLine, fonts.body(), TextColor(color(WAX_DARK))));
                button(line, &art, &fonts, T::MasterUseGemini, true, KeyAction::UseGemini);
            });
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
                    right.spawn((scroll_list(column(0.0), MODEL_LIST_HEIGHT), ModelList));
                    right.spawn(Node { column_gap: px(3), row_gap: px(2), flex_wrap: FlexWrap::Wrap, ..default() }).with_children(|line| {
                        button(line, &art, &fonts, T::ModelUp, true, ModelAct::Up);
                        button(line, &art, &fonts, T::ModelDown, true, ModelAct::Down);
                        button(line, &art, &fonts, T::ModelSwitch, true, ModelAct::Switch);
                        button(line, &art, &fonts, T::ModelsReset, true, ModelAct::Reset);
                    });
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

/// The words at the right of a model's line and their colour.
fn state_of(line: &ModelLine, availability: Availability, now: u64, lowest: Tier, lang: Lang) -> (String, Rgba) {
    match (line.place, line.class) {
        (None, Class::Pro) => (T::ModelOffLarge.text(lang).to_string(), FADED),
        (None, Class::Unknown) => (T::ModelOffNew.text(lang).to_string(), FADED),
        (None, _) => (T::ModelOff.text(lang).to_string(), FADED),
        // Below the chosen fallback tier: in the order, but it will not be used.
        (Some(_), _) if line.tier > lowest => (model_status(availability, now, lang), PARCHMENT_DARK),
        (Some(_), _) => (model_status(availability, now, lang), if availability.free_keys > 0 { MOSS } else { WAX }),
    }
}

/// The list is built anew only when the models in it or their order change: its lines are
/// pressed, and a line redrawn under the cursor would swallow the press. What changes by
/// the second — how many keys are free, how long a rest has left — is written into the
/// lines that are there.
#[allow(clippy::too_many_arguments)]
fn redraw_models(
    mut commands: Commands,
    fonts: Res<Fonts>,
    gm: Res<Gm>,
    lang: Res<Lang>,
    settings: Res<Settings>,
    time: Res<Time>,
    mut picked: ResMut<Picked>,
    mut since: Local<f32>,
    mut shown: Local<Vec<ModelLine>>,
    list: Query<Entity, With<ModelList>>,
    fresh: Query<(), Added<ModelList>>,
    mut states: Query<(&ModelState, &mut Text, &mut TextColor)>,
    mut rows: Query<(&ModelRow, &Interaction, &mut BackgroundColor)>,
) {
    let Ok(list) = list.single() else { return };
    let now = gm.dispatcher.now();
    let survey = gm.dispatcher.survey();
    let (lines, free): (Vec<ModelLine>, Vec<Availability>) = gm.dispatcher.with_pool(|pool| {
        let lines = model_lines(pool.chain(), &survey);
        let free = lines.iter().map(|line| pool.availability(now, &line.id)).collect();
        (lines, free)
    });
    // A model that is gone cannot stay picked.
    if picked.0.as_ref().is_some_and(|id| !lines.iter().any(|line| line.id == *id)) {
        picked.0 = None;
    }
    for (row, interaction, mut back) in &mut rows {
        let wanted = if picked.0.as_deref() == Some(row.0.as_str()) {
            color(BRASS_LIGHT)
        } else if *interaction != Interaction::None {
            color(PARCHMENT_SHADE)
        } else {
            Color::NONE
        };
        if back.0 != wanted {
            back.0 = wanted;
        }
    }
    if *shown != lines || !fresh.is_empty() {
        *shown = lines.clone();
        *since = 0.0;
        commands.entity(list).despawn_children().with_children(|rows| {
            if lines.is_empty() {
                rows.spawn((Text::default(), T::ModelsUnknown, fonts.body(), TextColor(color(FADED))));
            }
            for (line, availability) in lines.iter().zip(&free) {
                let (state, state_color) = state_of(line, *availability, now, settings.lowest_tier, *lang);
                let name = if line.place.is_some() && line.tier <= settings.lowest_tier { INK } else { FADED };
                let label = line.place.map_or_else(|| format!(" · {}", line.id), |n| format!("{n:>2} {}", line.id));
                rows.spawn((
                    crate::widgets::Plate,
                    crate::widgets::PixelButton::plain(),
                    ModelRow(line.id.clone()),
                    AutoDirectionalNavigation::default(),
                    Node { width: percent(100), justify_content: JustifyContent::SpaceBetween, column_gap: px(6), padding: UiRect::horizontal(px(2)), flex_shrink: 0.0, ..default() },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|row| {
                    row.spawn((Text::new(label), TextLayout::no_wrap(), fonts.body(), TextColor(color(name)), Pickable::IGNORE));
                    row.spawn((ModelState(line.id.clone()), Text::new(state), TextLayout::no_wrap(), fonts.body(), TextColor(color(state_color)), Pickable::IGNORE));
                });
            }
        });
        return;
    }
    *since += time.delta_secs();
    if *since < 1.0 && !lang.is_changed() && !settings.is_changed() {
        return;
    }
    *since = 0.0;
    for (state, mut text, mut ink) in &mut states {
        let Some((line, availability)) = lines.iter().zip(&free).find(|(line, _)| line.id == state.0) else { continue };
        let (said, said_color) = state_of(line, *availability, now, settings.lowest_tier, *lang);
        if text.0 != said {
            text.0 = said;
        }
        if ink.0 != color(said_color) {
            ink.0 = color(said_color);
        }
    }
}

/// A line of the list pressed: that model is the one the buttons underneath act on.
fn on_pick(used: On<Activate>, rows: Query<&ModelRow>, mut picked: ResMut<Picked>) {
    if let Ok(row) = rows.get(used.entity) {
        picked.0 = Some(row.0.clone());
    }
}

/// The picked model moved, switched, or the whole order given back to the game. The chain
/// follows at once, and the choice is kept with the settings.
fn on_model_act(used: On<Activate>, acts: Query<&ModelAct>, gm: Res<Gm>, picked: Res<Picked>, mut settings: ResMut<Settings>, store: Res<crate::store::Store>) {
    let Ok(act) = acts.get(used.entity) else { return };
    let chain: Vec<llm::Model> = gm.dispatcher.with_pool(|pool| pool.chain().to_vec());
    let choice = match (act, &picked.0) {
        (ModelAct::Reset, _) => llm::Choice::default(),
        (ModelAct::Up, Some(id)) => settings.gemini.moved(&chain, id, true),
        (ModelAct::Down, Some(id)) => settings.gemini.moved(&chain, id, false),
        (ModelAct::Switch, Some(id)) => settings.gemini.switched(&chain, id),
        (_, None) => return,
    };
    if choice == settings.gemini {
        return;
    }
    gm.dispatcher.set_choice(choice.clone());
    settings.gemini = choice;
    if let Err(e) = store.save_settings(&settings) {
        error!("could not save settings: {e}");
    }
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
    (mut settings, store, mut desk, mut next): (ResMut<Settings>, Res<crate::store::Store>, ResMut<crate::provider::Desk>, ResMut<NextState<Screen>>),
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
        KeyAction::Provider(kind) => {
            desk.open_on = Some(kind);
            next.set(Screen::Provider);
        }
        KeyAction::UseGemini => {
            settings.set_master(None);
            if let Err(e) = store.save_settings(&settings) {
                error!("could not save settings: {e}");
            }
        }
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
        app.init_resource::<Picked>().add_systems(OnEnter(Screen::Keys), spawn_keys)
            .add_systems(Update, (submit_on_enter, redraw_keys, redraw_models, show_test, master_line).chain().run_if(in_state(Screen::Keys)))
            .add_observer(on_pick)
            .add_observer(on_model_act)
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

    #[test]
    fn the_list_shows_the_chain_in_its_order_and_then_what_is_left_out() {
        let listed = |ids: &[&str]| -> Vec<llm::ListedModel> { ids.iter().map(|id| serde_json::from_value(serde_json::json!({ "name": format!("models/{id}"), "supportedGenerationMethods": ["generateContent"] })).unwrap()).collect() };
        let survey = llm::survey(&[], &listed(&["gemini-4-hydrogen", "gemini-3.8-flash", "gemini-3.5-flash-lite", "gemini-4-argon", "gemini-5-xenon"]));
        let choice = llm::Choice { order: vec!["gemini-3.5-flash-lite".into(), "gemini-4-hydrogen".into()], off: vec!["gemini-3.8-flash".into()] };
        let lines = model_lines(&llm::arrange(&survey, &choice), &survey);
        let brief: Vec<(&str, Option<usize>)> = lines.iter().map(|l| (l.id.as_str(), l.place)).collect();
        assert_eq!(brief, [("gemini-3.5-flash-lite", Some(1)), ("gemini-4-hydrogen", Some(2)), ("gemini-3.8-flash", None), ("gemini-4-argon", None), ("gemini-5-xenon", None)]);
        // What is left out says why; what is asked says how many keys are free.
        let none = Availability::default();
        let said = |n: usize| state_of(&lines[n], none, 0, Tier::Gemma, Lang::En).0;
        assert_eq!((said(2).as_str(), said(3).as_str(), said(4).as_str()), ("off", "Pro · off", "new · off"));
        let free = Availability { free_keys: 1, live_keys: 2, next_free: None };
        assert_eq!(state_of(&lines[1], free, 0, Tier::Gemma, Lang::En), ("1/2".to_string(), MOSS));
        // In the order but below the tier the player allows: shown as not in use.
        assert_eq!(state_of(&lines[0], free, 0, Tier::Flash, Lang::En).1, PARCHMENT_DARK);
    }
}
