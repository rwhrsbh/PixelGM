//! The key screen: enter Gemini keys, see which ones work, and watch how much of each model
//! is left.

use crate::gm::{Gm, GmTest, Health, KeyBook, masked, trouble_text};
use crate::i18n::{Lang, T, wait_text};
use crate::menu::{ReturnTo, Screen};
use crate::store::Settings;
use crate::theme::*;
use crate::ui::egui::{self, RichText, vec2};
use crate::ui::kit::{self, Sheet, button, hint, warning};
use crate::ui::theme::c;
use bevy::prelude::*;
use llm::{Availability, Class, KeyId, Tier};

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

/// Where a player gets a key. An address, so it is the same in every language.
const KEY_PAGE: &str = "aistudio.google.com/apikey";

fn model_status(a: Availability, now: u64, lang: Lang) -> String {
    match (a.live_keys, a.free_keys, a.next_free) {
        (0, _, _) => "—".to_string(),
        (_, 0, Some(until)) => wait_text(until.saturating_sub(now), lang),
        (live, free, _) => format!("{free}/{live}"),
    }
}

fn health_text(health: &Health) -> (T, Rgba) {
    match health {
        Health::Checking => (T::KeyChecking, FADED),
        Health::Working => (T::KeyWorking, MOSS),
        Health::Rejected => (T::KeyRejected, WAX),
        Health::Unreachable => (T::KeyUnreachable, WAX),
    }
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

/// What the last test of the game master came to, in words and in the colour of the news.
fn test_text(test: &GmTest, now: u64, lang: Lang) -> (String, Rgba) {
    match test {
        GmTest::Idle => (T::TestGmHint.text(lang).to_string(), FADED),
        GmTest::Running => (T::TestRunning.text(lang).to_string(), FADED),
        GmTest::Answered { model, seconds, text } => (format!("{model} {} {seconds:.1} {}: {text}", T::TestAnswered.text(lang), T::Seconds.text(lang)), MOSS),
        GmTest::Failed(trouble) => (trouble_text(trouble, now, lang), WAX),
    }
}

/// The Gemini screen, drawn with egui in the order every master's screen has: whether it
/// runs the game, its keys, its models in the order they are asked, how it answers.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn keys_screen(
    mut egui: bevy_egui::EguiContexts,
    (gm, mut book, mut test, lang): (Res<Gm>, ResMut<KeyBook>, ResMut<GmTest>, Res<Lang>),
    (mut settings, store, mut desk, mut next, mut return_to): (ResMut<Settings>, Res<crate::store::Store>, ResMut<crate::provider::Desk>, ResMut<NextState<Screen>>, ResMut<ReturnTo>),
    mut typed: Local<String>,
) {
    use crate::provider::parts::{self, KeyPress, ModelPress};
    let lang = *lang;
    let t = |text: T| text.text(lang);
    let now = gm.dispatcher.now();
    let survey = gm.dispatcher.survey();
    let (lines, free, chain): (Vec<ModelLine>, Vec<Availability>, Vec<llm::Model>) = gm.dispatcher.with_pool(|pool| {
        let lines = model_lines(pool.chain(), &survey);
        let free = lines.iter().map(|line| pool.availability(now, &line.id)).collect();
        (lines, free, pool.chain().to_vec())
    });
    let Ok(ctx) = egui.ctx_mut() else { return };
    let Some(mut ui) = kit::screen_ui(ctx, "keys") else { return };
    let mut to_tab = None;
    let mut make_master = false;
    let mut add_key = false;
    let mut key_press: Option<(KeyId, KeyPress)> = None;
    let mut model_press: Option<(String, ModelPress)> = None;
    let mut reset = false;
    let mut answer_changed = false;
    let mut edited = settings.clone();
    let mut back = false;
    let mut testing = false;
    let (said, said_tint) = test_text(&test, now, lang);
    let runs = settings.active_provider().is_none();
    let in_chain = chain.len();
    let sheet = Sheet { id: "keys", title: t(T::AiTitle), about: t(T::KeysAbout), max: vec2(480.0, 470.0), top: 0.0, fixed: true };
    kit::sheet(
        &mut ui,
        sheet,
        |ui| {
            to_tab = crate::provider::tabs(ui, &settings, None, lang);
            make_master = parts::status(ui, runs, lang);
            parts::section(ui, T::SectionKeys, lang);
            ui.add(egui::Label::new(t(T::KeysHelp)).wrap());
            ui.label(RichText::new(KEY_PAGE).color(c(WAX_DARK)));
            if book.entries.is_empty() {
                warning(ui, t(T::NoKeys));
            }
            for entry in &book.entries {
                let (status, tint) = health_text(&entry.health);
                if let Some(press) = parts::key_row(ui, &masked(&entry.secret), Some((t(status), tint)), entry.health == Health::Unreachable, lang) {
                    key_press = Some((entry.id, press));
                }
            }
            add_key = parts::key_field(ui, &mut typed, lang);
            hint(ui, t(T::KeysInputHint));
            parts::section(ui, T::SectionModels, lang);
            hint(ui, t(T::ModelsLegend));
            if lines.is_empty() {
                hint(ui, t(T::ModelsUnknown));
            }
            for (line, availability) in lines.iter().zip(&free) {
                let (state, tint) = state_of(line, *availability, now, settings.lowest_tier, lang);
                let place = line.place;
                let up = place.is_some_and(|n| n > 1);
                let down = place.is_some_and(|n| n < in_chain);
                if let Some(press) = parts::model_row(ui, place, &line.id, Some((&state, tint)), up, down, Some(place.is_some()), lang) {
                    model_press = Some((line.id.clone(), press));
                }
            }
            if ui.add(button(t(T::ModelsReset))).clicked() {
                reset = true;
            }
            parts::section(ui, T::SectionAnswer, lang);
            answer_changed = crate::settings::answer_settings(ui, &mut edited, true);
            // What the last test of the master came to.
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(&said).color(c(said_tint))).wrap());
        },
        |footer| {
            if footer.add(button(t(T::Back)).key("Esc")).clicked() {
                back = true;
            }
            if footer.add(button(t(T::TestGm))).on_hover_text(t(T::TestGmHint)).clicked() {
                testing = true;
            }
        },
    );
    let mut save = false;
    if answer_changed {
        *settings = edited;
        save = true;
    }
    if make_master {
        settings.set_master(None);
        save = true;
    }
    // A duplicate stays in the field so the player can see what was refused.
    if add_key && gm.add_key(&mut book, &typed) {
        typed.clear();
    }
    match key_press {
        Some((id, KeyPress::Remove)) => gm.remove_key(&mut book, id),
        Some((id, KeyPress::Recheck)) => gm.recheck(&mut book, id),
        None => {}
    }
    if testing {
        gm.test(&mut test, &settings);
    }
    // A model moved or switched, or the whole order given back to the game. The chain
    // follows at once, and the choice is kept with the settings.
    let choice = match model_press {
        _ if reset => Some(llm::Choice::default()),
        Some((id, ModelPress::Up)) => Some(settings.gemini.moved(&chain, &id, true)),
        Some((id, ModelPress::Down)) => Some(settings.gemini.moved(&chain, &id, false)),
        Some((id, ModelPress::Drop)) => Some(settings.gemini.switched(&chain, &id)),
        None => None,
    };
    if let Some(choice) = choice.filter(|choice| *choice != settings.gemini) {
        gm.dispatcher.set_choice(choice.clone());
        settings.gemini = choice;
        save = true;
    }
    if save && let Err(e) = store.save_settings(&settings) {
        error!("could not save settings: {e}");
    }
    if let Some(Some(kind)) = to_tab {
        desk.open_on = Some(kind);
        next.set(Screen::Provider);
    }
    if back {
        next.set(return_to.back());
    }
}

pub struct KeysPlugin;

impl Plugin for KeysPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(bevy_egui::EguiPrimaryContextPass, keys_screen.in_set(crate::ui::UiSet::Screens).run_if(in_state(Screen::Keys)));
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
