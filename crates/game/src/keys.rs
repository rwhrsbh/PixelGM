//! The key screen: enter Gemini keys, see which ones work, and watch how much of each model
//! is left.

use crate::gm::{Gm, GmTest, Health, KeyBook, masked, trouble_text};
use crate::i18n::{Lang, T, wait_text};
use crate::menu::{ReturnTo, Screen};
use crate::store::Settings;
use crate::theme::*;
use crate::ui::egui::{self, Key, RichText, Sense, vec2};
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

/// What a press on the Gemini screen asks for.
#[derive(Clone, Copy)]
enum KeyAction {
    Add,
    Remove(KeyId),
    Recheck(KeyId),
    TestGm,
    /// Gemini runs the game again.
    UseGemini,
}

/// What the buttons under the list do with the model that is picked.
#[derive(Clone, Copy)]
enum ModelAct {
    Up,
    Down,
    Switch,
    /// Back to the game's own order, with nothing switched off.
    Reset,
}

/// The Gemini screen, drawn with egui: the master's tabs, the keys at the left and the
/// models at the right, each with what it is for; a test and Back at the bottom.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn keys_screen(
    mut egui: bevy_egui::EguiContexts,
    (gm, mut book, mut test, lang): (Res<Gm>, ResMut<KeyBook>, ResMut<GmTest>, Res<Lang>),
    (mut settings, store, mut desk, mut next, mut return_to): (ResMut<Settings>, Res<crate::store::Store>, ResMut<crate::provider::Desk>, ResMut<NextState<Screen>>, ResMut<ReturnTo>),
    mut picked: Local<Option<String>>,
    mut typed: Local<String>,
) {
    let lang = *lang;
    let t = |text: T| text.text(lang);
    let now = gm.dispatcher.now();
    let survey = gm.dispatcher.survey();
    let (lines, free, chain): (Vec<ModelLine>, Vec<Availability>, Vec<llm::Model>) = gm.dispatcher.with_pool(|pool| {
        let lines = model_lines(pool.chain(), &survey);
        let free = lines.iter().map(|line| pool.availability(now, &line.id)).collect();
        (lines, free, pool.chain().to_vec())
    });
    // A model that is gone cannot stay picked.
    if picked.as_ref().is_some_and(|id| !lines.iter().any(|line| line.id == *id)) {
        *picked = None;
    }
    let Ok(ctx) = egui.ctx_mut() else { return };
    let Some(mut ui) = kit::screen_ui(ctx, "keys") else { return };
    let mut to_tab = None;
    let mut act = None;
    let mut model_act = None;
    let mut back = false;
    let mut testing = false;
    let (said, said_tint) = test_text(&test, now, lang);
    let sheet = Sheet { id: "keys", title: t(T::AiTitle), about: t(T::KeysAbout), max: vec2(480.0, 380.0), top: 0.0, fixed: true };
    kit::sheet(
        &mut ui,
        sheet,
        |ui| {
            to_tab = crate::provider::tabs(ui, &settings, None, lang);
            ui.horizontal(|row| {
                let who = settings.active_provider().map_or("Gemini", |kind| kind.name());
                row.label(RichText::new(format!("{} {who}", t(T::MasterNow))).color(c(WAX_DARK)));
                if row.add(button(t(T::MasterUseGemini)).enabled(settings.active_provider().is_some(), t(T::GeminiAlready))).clicked() {
                    act = Some(KeyAction::UseGemini);
                }
            });
            ui.add_space(4.0);
            ui.columns(2, |cols| {
                // The keys: how to get one, the field to paste it in, and those there are.
                let left = &mut cols[0];
                left.add(egui::Label::new(t(T::KeysHelp)).wrap());
                left.label(RichText::new(KEY_PAGE).color(c(WAX_DARK)));
                left.horizontal(|row| {
                    let field = row.add(egui::TextEdit::singleline(&mut *typed).desired_width((row.available_width() - 74.0).max(60.0)).char_limit(200).hint_text("AIza…"));
                    let entered = field.lost_focus() && row.input(|i| i.key_pressed(Key::Enter));
                    let ready = !typed.trim().is_empty();
                    if row.add(button(t(T::Add)).enabled(ready, t(T::KeyTypeFirst))).clicked() || (entered && ready) {
                        act = Some(KeyAction::Add);
                    }
                });
                hint(left, t(T::KeysInputHint));
                egui::ScrollArea::vertical().id_salt("key_list").max_height(90.0).show(left, |ui| {
                    if book.entries.is_empty() {
                        warning(ui, t(T::NoKeys));
                    }
                    for entry in &book.entries {
                        let (status, tint) = health_text(&entry.health);
                        ui.horizontal(|row| {
                            row.label(masked(&entry.secret));
                            row.label(RichText::new(t(status)).color(c(tint)));
                            if entry.health == Health::Unreachable && row.add(button(t(T::Recheck))).clicked() {
                                act = Some(KeyAction::Recheck(entry.id));
                            }
                            if row.add(button(t(T::Remove))).clicked() {
                                act = Some(KeyAction::Remove(entry.id));
                            }
                        });
                    }
                });
                // The models: the order they are asked in, and what each has left.
                let right = &mut cols[1];
                right.label(RichText::new(t(T::ModelsTitle)).color(c(WAX_DARK)));
                hint(right, t(T::ModelsLegend));
                egui::ScrollArea::vertical().id_salt("model_list").max_height(120.0).show(right, |ui| {
                    if lines.is_empty() {
                        hint(ui, t(T::ModelsUnknown));
                    }
                    for (line, availability) in lines.iter().zip(&free) {
                        let (state, state_tint) = state_of(line, *availability, now, settings.lowest_tier, lang);
                        let name_tint = if line.place.is_some() && line.tier <= settings.lowest_tier { INK } else { FADED };
                        let label = line.place.map_or_else(|| format!(" · {}", line.id), |n| format!("{n:>2} {}", line.id));
                        let chosen = picked.as_deref() == Some(line.id.as_str());
                        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 12.0), Sense::click());
                        if chosen {
                            ui.painter().rect_filled(rect, 0.0, c(BRASS_LIGHT));
                        } else if response.hovered() {
                            ui.painter().rect_filled(rect, 0.0, c(PARCHMENT_SHADE));
                        }
                        ui.painter().text(rect.left_center() + vec2(2.0, 0.0), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(crate::ui::theme::BODY), c(name_tint));
                        ui.painter().text(rect.right_center() - vec2(2.0, 0.0), egui::Align2::RIGHT_CENTER, state, egui::FontId::proportional(crate::ui::theme::BODY), c(state_tint));
                        if response.clicked() {
                            *picked = Some(line.id.clone());
                        }
                    }
                });
                right.horizontal_wrapped(|row| {
                    let ready = picked.is_some();
                    let why = t(T::PickModelFirst);
                    for (label, then) in [(T::ModelUp, ModelAct::Up), (T::ModelDown, ModelAct::Down), (T::ModelSwitch, ModelAct::Switch)] {
                        if row.add(button(t(label)).enabled(ready, why)).clicked() {
                            model_act = Some(then);
                        }
                    }
                    if row.add(button(t(T::ModelsReset))).clicked() {
                        model_act = Some(ModelAct::Reset);
                    }
                });
            });
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
    if testing {
        act = Some(KeyAction::TestGm);
    }
    match act {
        Some(KeyAction::Add) => {
            // A duplicate stays in the field so the player can see what was refused.
            if gm.add_key(&mut book, &typed) {
                typed.clear();
            }
        }
        Some(KeyAction::Remove(id)) => gm.remove_key(&mut book, id),
        Some(KeyAction::Recheck(id)) => gm.recheck(&mut book, id),
        Some(KeyAction::TestGm) => gm.test(&mut test, &settings),
        Some(KeyAction::UseGemini) => {
            settings.set_master(None);
            if let Err(e) = store.save_settings(&settings) {
                error!("could not save settings: {e}");
            }
        }
        None => {}
    }
    // The picked model moved, switched, or the whole order given back to the game. The chain
    // follows at once, and the choice is kept with the settings.
    if let Some(model_act) = model_act {
        let choice = match (model_act, picked.as_ref()) {
            (ModelAct::Reset, _) => Some(llm::Choice::default()),
            (ModelAct::Up, Some(id)) => Some(settings.gemini.moved(&chain, id, true)),
            (ModelAct::Down, Some(id)) => Some(settings.gemini.moved(&chain, id, false)),
            (ModelAct::Switch, Some(id)) => Some(settings.gemini.switched(&chain, id)),
            (_, None) => None,
        };
        if let Some(choice) = choice.filter(|choice| *choice != settings.gemini) {
            gm.dispatcher.set_choice(choice.clone());
            settings.gemini = choice;
            if let Err(e) = store.save_settings(&settings) {
                error!("could not save settings: {e}");
            }
        }
    }
    match to_tab {
        Some(Some(kind)) => {
            desk.open_on = Some(kind);
            next.set(Screen::Provider);
        }
        Some(None) | None => {}
    }
    if back {
        next.set(return_to.0.take().unwrap_or(Screen::Main));
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
