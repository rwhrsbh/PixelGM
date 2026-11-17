//! The screen for game masters other than Gemini: NVIDIA NIM, OpenRouter, Cloudflare Workers
//! AI, and any service of the player's own that speaks the OpenAI protocol. Each has its keys
//! (they take turns), its models for the telling in the order they are tried, and its
//! speaking models for the narrator's voice. The models a provider offers can be fetched and
//! picked from.

use crate::i18n::{Lang, T};
use crate::menu::{ReturnTo, Screen};
use crate::store::{Settings, Store};
use crate::theme::*;
use crate::ui::egui::{self, RichText, Ui, vec2};
use crate::ui::kit::{self, Sheet, button, hint};
use crate::ui::theme::c;
use bevy::prelude::*;
use llm::providers::{Kind, Rotation};
use std::sync::{Arc, Mutex};

/// The parts every master's screen is made of, in the same order on each, so that what is
/// looked for is always in the same place: whether it runs the game, its keys, its models,
/// and how it answers.
pub mod parts {
    use crate::i18n::{Lang, T};
    use crate::theme::*;
    use crate::ui::egui::{self, Key, RichText, Ui};
    use crate::ui::kit::{self, button, hint};
    use crate::ui::theme::c;

    /// Whether this master runs the game, or a button to make it. Gives true on the press.
    pub fn status(ui: &mut Ui, runs: bool, lang: Lang) -> bool {
        let mut pressed = false;
        ui.horizontal(|row| {
            if runs {
                row.label(RichText::new(T::RunsGame.text(lang)).color(c(MOSS)));
            } else {
                pressed = row.add(button(T::MakeMaster.text(lang)).primary()).on_hover_text(T::MakeMasterHint.text(lang)).clicked();
                hint(row, T::MakeMasterHint.text(lang));
            }
        });
        pressed
    }

    /// The title of a part of the screen.
    pub fn section(ui: &mut Ui, title: T, lang: Lang) {
        ui.add_space(6.0);
        kit::title(ui, title.text(lang), crate::ui::theme::SMALL_TITLE);
    }

    /// The field a key is pasted in, and its button. Gives true when it is to be added.
    pub fn key_field(ui: &mut Ui, typed: &mut String, lang: Lang) -> bool {
        let mut add = false;
        ui.horizontal(|row| {
            let field = row.add(egui::TextEdit::singleline(typed).desired_width((row.available_width() - 140.0).max(120.0)).char_limit(2000));
            let ready = !typed.trim().is_empty();
            let entered = field.lost_focus() && row.input(|i| i.key_pressed(Key::Enter)) && ready;
            add = row.add(button(T::Add.text(lang)).enabled(ready, T::KeyTypeFirst.text(lang)).key("Enter")).clicked() || entered;
        });
        add
    }

    /// What a press on a key's line asks for.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum KeyPress {
        Remove,
        Recheck,
    }

    /// A key's line: its end, how it is, and its buttons.
    pub fn key_row(ui: &mut Ui, shown: &str, state: Option<(&str, Rgba)>, recheck: bool, lang: Lang) -> Option<KeyPress> {
        let mut press = None;
        ui.horizontal(|row| {
            row.label(shown);
            if let Some((said, tint)) = state {
                row.label(RichText::new(said).color(c(tint)));
            }
            row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |row| {
                if row.add(button(T::Remove.text(lang))).clicked() {
                    press = Some(KeyPress::Remove);
                }
                if recheck && row.add(button(T::Recheck.text(lang))).clicked() {
                    press = Some(KeyPress::Recheck);
                }
            });
        });
        press
    }

    /// What a press on a model's line asks for.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum ModelPress {
        Up,
        Down,
        /// Off the list (a provider's), or not asked (Gemini's).
        Drop,
    }

    /// A model's line: its place and name, how it is, and the buttons that move it or take
    /// it off. `switch` names the last button when it switches rather than drops: whether
    /// the model is asked now.
    #[allow(clippy::too_many_arguments)]
    pub fn model_row(ui: &mut Ui, place: Option<usize>, id: &str, state: Option<(&str, Rgba)>, can_up: bool, can_down: bool, switch: Option<bool>, lang: Lang) -> Option<ModelPress> {
        let mut press = None;
        ui.horizontal(|row| {
            let name = match place {
                Some(n) => format!("{n:>2}. {id}"),
                None => format!(" ·  {id}"),
            };
            row.label(RichText::new(name).color(c(if place.is_some() { INK } else { FADED })));
            if let Some((said, tint)) = state {
                row.label(RichText::new(said).color(c(tint)));
            }
            row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |row| {
                let last = match switch {
                    Some(true) => row.add(button(T::ModelOffShort.text(lang))).on_hover_text(T::ModelSwitchHint.text(lang)),
                    Some(false) => row.add(button(T::ModelOn.text(lang))).on_hover_text(T::ModelSwitchHint.text(lang)),
                    None => row.add(button("×")).on_hover_text(T::ModelDropHint.text(lang)),
                };
                if last.clicked() {
                    press = Some(ModelPress::Drop);
                }
                if row.add(button("▼").enabled(can_down, "")).on_hover_text(T::ModelDownHint.text(lang)).clicked() {
                    press = Some(ModelPress::Down);
                }
                if row.add(button("▲").enabled(can_up, "")).on_hover_text(T::ModelUpHint.text(lang)).clicked() {
                    press = Some(ModelPress::Up);
                }
            });
        });
        press
    }
}

/// What a press on the screen asks for.
#[derive(Clone, Copy)]
enum Act {
    Toggle,
    AddKey,
    DropKey(usize),
    /// The models for the telling (`false`) or the speaking ones (`true`).
    Section(bool),
    AddModel,
    Up(usize),
    Down(usize),
    Drop(usize),
    Fetch,
    /// One of the models on offer, by its place among those shown.
    Pick(usize),
    Test,
    Back,
}

/// What a thread came back with: the models on offer.
type Fetched = Arc<Mutex<Option<Result<Vec<String>, String>>>>;

/// The screen as its owner left it.
#[derive(Resource)]
pub struct Desk {
    pub tab: Kind,
    /// The provider another screen asked to be shown, once.
    pub open_on: Option<Kind>,
    voices: bool,
    /// What is typed and not yet added.
    typed_key: String,
    typed_model: String,
    /// The models the provider offers, as last fetched, and for which list.
    offered: Vec<String>,
    offered_for: Option<(Kind, bool)>,
    fetched: Fetched,
    note: Arc<Mutex<String>>,
}

impl Default for Desk {
    fn default() -> Self {
        Self { tab: Kind::Nim, open_on: None, voices: false, typed_key: String::new(), typed_model: String::new(), offered: Vec::new(), offered_for: None, fetched: Arc::default(), note: Arc::default() }
    }
}

pub struct ProviderPlugin;

impl Plugin for ProviderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Desk>()
            .add_systems(OnEnter(Screen::Provider), enter)
            .add_systems(Update, arrived.run_if(in_state(Screen::Provider)))
            .add_systems(bevy_egui::EguiPrimaryContextPass, provider_screen.in_set(crate::ui::UiSet::Screens).run_if(in_state(Screen::Provider)));
    }
}

/// The masters, one tab each, the one that runs the game marked. Gives, on a press of
/// another tab, `None` for Gemini or the provider pressed.
pub fn tabs(ui: &mut Ui, settings: &Settings, current: Option<Kind>, lang: Lang) -> Option<Option<Kind>> {
    let running = settings.active_provider();
    let mut to = None;
    ui.horizontal_wrapped(|row| {
        let name = |runs: bool, name: &str| if runs { format!("● {name}") } else { name.to_string() };
        let mut gemini = button(name(running.is_none(), "Gemini"));
        if current.is_none() {
            gemini = gemini.primary();
        }
        if row.add(gemini).on_hover_text(T::MasterMark.text(lang)).clicked() && current.is_some() {
            to = Some(None);
        }
        for kind in Kind::ALL {
            let mut tab = button(name(running == Some(kind), kind.name()));
            if current == Some(kind) {
                tab = tab.primary();
            }
            if row.add(tab).on_hover_text(T::MasterMark.text(lang)).clicked() && current != Some(kind) {
                to = Some(Some(kind));
            }
        }
    });
    to
}

/// The end of a key, to tell keys apart without showing them.
fn tail(key: &str) -> String {
    let n = key.chars().count();
    format!("…{}", key.chars().skip(n.saturating_sub(4)).collect::<String>())
}

fn enter(mut settings: ResMut<Settings>, mut desk: ResMut<Desk>) {
    desk.note.lock().unwrap().clear();
    // Opened on the provider that is the master now, if one is.
    if let Some(asked) = desk.open_on.take() {
        desk.tab = asked;
    } else if let Some(active) = settings.active_provider() {
        desk.tab = active;
    }
    settings.provider_mut(desk.tab);
}

/// The models on offer that what is typed lets through: those shown, in order.
fn narrowed(desk: &Desk, typed: &str) -> Vec<String> {
    let wanted = typed.trim().to_lowercase();
    desk.offered.iter().filter(|id| wanted.is_empty() || id.to_lowercase().contains(&wanted)).take(60).cloned().collect()
}

/// What a thread fetched has come: the models on offer.
fn arrived(mut desk: ResMut<Desk>, lang: Res<Lang>) {
    let Some(result) = desk.fetched.lock().unwrap().take() else { return };
    match result {
        Ok(models) => {
            *desk.note.lock().unwrap() = format!("{} {}", T::ProvOffered.text(*lang), models.len());
            desk.offered = models;
        }
        Err(why) => {
            *desk.note.lock().unwrap() = format!("{} {}", T::ProviderFails.text(*lang), why.chars().take(120).collect::<String>());
            desk.offered.clear();
        }
    }
}

/// The screen of another master, drawn with egui in the order every master's screen has:
/// whether it runs the game, its address and keys, its models (for the telling or for the
/// voice) in the order they are tried, how it answers.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn provider_screen(mut egui: bevy_egui::EguiContexts, (mut settings, store, lang, gm): (ResMut<Settings>, Res<Store>, Res<Lang>, Res<crate::gm::Gm>), mut desk: ResMut<Desk>, (mut next, mut return_to): (ResMut<NextState<Screen>>, ResMut<ReturnTo>)) {
    use parts::{KeyPress, ModelPress};
    let lang = *lang;
    let t = |text: T| text.text(lang);
    let tab = desk.tab;
    let voices = desk.voices;
    let note = desk.note.lock().unwrap().clone();
    let mut this = settings.provider_mut(tab).clone();
    let keys = this.plain_keys();
    let stale = desk.offered_for != Some((tab, voices));
    let shown = if stale { Vec::new() } else { narrowed(&desk, &desk.typed_model) };
    let Ok(ctx) = egui.ctx_mut() else { return };
    let Some(mut ui) = kit::screen_ui(ctx, "provider") else { return };
    let mut acts: Vec<Act> = Vec::new();
    // What the footer's buttons asked for, after what the body's did.
    let mut pressed: Vec<Act> = Vec::new();
    let mut to_tab = None;
    let mut edited = false;
    let mut answer = settings.clone();
    let mut answer_changed = false;
    let desk = &mut *desk;
    let sheet = Sheet { id: "provider", title: t(T::AiTitle), about: t(T::ProviderAbout), max: vec2(480.0, 470.0), top: 0.0, fixed: true };
    kit::sheet(
        &mut ui,
        sheet,
        |ui| {
            to_tab = tabs(ui, &settings, Some(tab), lang);
            if parts::status(ui, this.on, lang) {
                acts.push(Act::Toggle);
            }
            parts::section(ui, T::SectionKeys, lang);
            hint(ui, t(if tab == Kind::OpenCodeGo { T::GoKeysHelp } else { T::ProvKeysHelp }));
            // What only this provider needs.
            let address = match tab {
                Kind::Cloudflare => Some((T::ProvAccount, &mut this.account, None)),
                Kind::Custom => Some((T::ProviderUrl, &mut this.url, None)),
                Kind::OpenCodeGo => Some((T::ProviderUrl, &mut this.url, Some(llm::providers::OPENCODE_GO))),
                _ => None,
            };
            if let Some((label, value, usual)) = address {
                ui.horizontal(|row| {
                    row.label(t(label));
                    let mut field = egui::TextEdit::singleline(value).desired_width(300.0).char_limit(300);
                    if let Some(usual) = usual {
                        field = field.hint_text(usual);
                    }
                    edited |= row.add(field).changed();
                });
                if tab == Kind::OpenCodeGo {
                    hint(ui, t(T::GoUrlHint));
                }
            }
            if keys.is_empty() {
                kit::warning(ui, t(T::NoKeys));
            }
            for (n, key) in keys.iter().enumerate().take(8) {
                if parts::key_row(ui, &tail(key), None, false, lang) == Some(KeyPress::Remove) {
                    acts.push(Act::DropKey(n));
                }
            }
            if parts::key_field(ui, &mut desk.typed_key, lang) {
                acts.push(Act::AddKey);
            }
            if tab == Kind::Nim {
                ui.add(egui::Label::new(RichText::new(t(T::ProvNimSlow)).color(c(WAX_DARK))).wrap());
            }
            parts::section(ui, T::SectionModels, lang);
            // The telling's models, or the voice's where this provider has any.
            if !matches!(tab, Kind::Nim | Kind::OpenCodeGo) {
                ui.horizontal(|row| {
                    for (section, label) in [(false, T::ProvChat), (true, T::ProvVoice)] {
                        let mut b = button(t(label));
                        if section == voices {
                            b = b.primary();
                        }
                        if row.add(b).clicked() && section != voices {
                            acts.push(Act::Section(section));
                        }
                    }
                    if voices {
                        row.label(t(T::ProvVoiceName));
                        edited |= row.add(egui::TextEdit::singleline(&mut this.voice).desired_width(90.0).char_limit(100)).changed();
                    }
                });
            }
            let chosen = if voices { &this.voices } else { &this.models };
            if chosen.is_empty() {
                hint(ui, t(T::ProvNone));
            }
            for (n, model) in chosen.iter().enumerate() {
                match parts::model_row(ui, Some(n + 1), model, None, n > 0, n + 1 < chosen.len(), None, lang) {
                    Some(ModelPress::Up) => acts.push(Act::Up(n)),
                    Some(ModelPress::Down) => acts.push(Act::Down(n)),
                    Some(ModelPress::Drop) => acts.push(Act::Drop(n)),
                    None => {}
                }
            }
            ui.horizontal(|row| {
                row.add(egui::TextEdit::singleline(&mut desk.typed_model).desired_width(220.0).char_limit(300));
                if row.add(button(t(T::ProvAdd)).enabled(!desk.typed_model.trim().is_empty(), t(T::ModelTypeFirst))).clicked() {
                    acts.push(Act::AddModel);
                }
                if row.add(button(t(T::ProvFetch))).clicked() {
                    acts.push(Act::Fetch);
                }
            });
            // Those on offer, narrowed by what is typed above; a press chooses one.
            egui::ScrollArea::vertical().id_salt("offered").max_height(64.0).show(ui, |ui| {
                ui.horizontal_wrapped(|row| {
                    if shown.is_empty() {
                        hint(row, t(if !stale && !desk.offered.is_empty() { T::ProvNoMatch } else { T::ProvFetchHint }));
                    }
                    for (n, id) in shown.iter().enumerate() {
                        if row.add(button(id.as_str())).clicked() {
                            acts.push(Act::Pick(n));
                        }
                    }
                });
            });
            parts::section(ui, T::SectionAnswer, lang);
            answer_changed = crate::settings::answer_settings(ui, &mut answer, false);
            parts::section(ui, T::SectionMemory, lang);
            let limit = crate::master::context_limit(&answer, &gm);
            answer_changed |= crate::settings::memory_section(ui, &mut answer, &limit);
            // What the last fetch or test came to.
            if !note.is_empty() {
                ui.add_space(4.0);
                ui.add(egui::Label::new(RichText::new(&note).color(c(WAX_DARK))).wrap());
            }
        },
        |footer| {
            if footer.add(button(t(T::Back)).key("Esc")).clicked() {
                pressed.push(Act::Back);
            }
            if footer.add(button(t(T::TestGm))).on_hover_text(t(T::TestGmHint)).clicked() {
                pressed.push(Act::Test);
            }
        },
    );
    if answer_changed {
        settings.thinking = answer.thinking;
        settings.first_token = answer.first_token;
        settings.lowest_tier = answer.lowest_tier;
        settings.context_smallest = answer.context_smallest;
    }
    if edited {
        let kept = settings.provider_mut(tab);
        kept.account = this.account.clone();
        kept.url = this.url.clone();
        kept.voice = this.voice.clone();
    }
    match to_tab {
        Some(None) => next.set(Screen::Keys),
        Some(Some(kind)) => {
            desk.tab = kind;
            desk.voices = false;
            desk.typed_key.clear();
            desk.typed_model.clear();
            desk.note.lock().unwrap().clear();
        }
        None => {}
    }
    acts.extend(pressed);
    let save = edited || answer_changed || !acts.is_empty();
    for act in acts {
        do_act(act, &mut settings, desk, &shown, tab, voices, lang, &mut next, &mut return_to);
    }
    if save && let Err(e) = store.save_settings(&settings) {
        error!("could not save settings: {e}");
    }
}

#[allow(clippy::too_many_arguments)]
fn do_act(act: Act, settings: &mut Settings, desk: &mut Desk, shown: &[String], tab: Kind, voices: bool, lang: Lang, next: &mut NextState<Screen>, return_to: &mut ReturnTo) {
    let patience = settings.patience();
    let this = settings.provider_mut(tab);
    fn list(this: &mut crate::store::ProviderSetting, voices: bool) -> &mut Vec<String> {
        if voices { &mut this.voices } else { &mut this.models }
    }
    match act {
        Act::Toggle => settings.set_master(Some(tab)),
        Act::AddKey => {
            // Several keys may be pasted at once, parted by spaces, commas or new lines.
            let pasted = std::mem::take(&mut desk.typed_key);
            for key in pasted.split([' ', ',', ';', '\n']).filter(|k| !k.trim().is_empty()) {
                this.add_key(key);
            }
        }
        Act::DropKey(n) => {
            // The place among the keys that can be read, which is what is shown.
            let plain = this.plain_keys();
            if let Some(gone) = plain.get(n) {
                let at = this.keys.iter().position(|hex| crate::secret::from_hex(hex).and_then(|blob| crate::secret::unprotect(&blob).ok()).and_then(|p| String::from_utf8(p).ok()).as_deref() == Some(gone.as_str()));
                if let Some(at) = at {
                    this.keys.remove(at);
                }
            }
        }
        Act::Section(to) => {
            desk.voices = to;
            desk.typed_model.clear();
        }
        Act::AddModel => {
            let model = std::mem::take(&mut desk.typed_model).trim().to_string();
            let models = list(this, voices);
            if !model.is_empty() && !models.contains(&model) {
                models.push(model);
            }
        }
        Act::Up(n) => {
            let models = list(this, voices);
            if n > 0 && n < models.len() {
                models.swap(n, n - 1);
            }
        }
        Act::Down(n) => {
            let models = list(this, voices);
            if n + 1 < models.len() {
                models.swap(n, n + 1);
            }
        }
        Act::Drop(n) => {
            let models = list(this, voices);
            if n < models.len() {
                models.remove(n);
            }
        }
        Act::Pick(n) => {
            if let Some(model) = shown.get(n) {
                let models = list(this, voices);
                if !models.contains(model) {
                    models.push(model.clone());
                }
            }
        }
        Act::Fetch => {
            let provider = this.provider(llm::ThinkingSetting::Off);
            let slot = desk.fetched.clone();
            desk.offered_for = Some((tab, voices));
            desk.offered.clear();
            *desk.note.lock().unwrap() = T::ProvFetching.text(lang).to_string();
            std::thread::spawn(move || {
                let listed = if voices { provider.list_voices() } else { provider.list_models() };
                *slot.lock().unwrap() = Some(listed);
            });
        }
        Act::Test => {
            let provider = this.provider(llm::ThinkingSetting::Off);
            let note = desk.note.clone();
            *note.lock().unwrap() = T::TestRunning.text(lang).to_string();
            // The test waits as long as the master would: the player's setting.
            std::thread::spawn(move || {
                // Asked afresh, with nothing resting, so that the word is about now.
                let rotation = Rotation::default();
                rotation.set_patience(patience);
                let said = if voices {
                    match rotation.speak(std::slice::from_ref(&provider), "The road goes ever on.", false, &llm::providers::Network) {
                        Ok((sound, who)) => format!("{} {who}, {} KB", T::ProviderWorks.text(lang), sound.len() / 1024),
                        Err(e) => format!("{} {}", T::ProviderFails.text(lang), e.chars().take(160).collect::<String>()),
                    }
                } else {
                    match rotation.complete(std::slice::from_ref(&provider), "Answer with one JSON object and nothing else.", "Reply with {\"ok\": true}.", &llm::providers::Network) {
                        Ok((text, who)) => format!("{} {who} → {}", T::ProviderWorks.text(lang), text.chars().take(40).collect::<String>()),
                        Err(e) => format!("{} {}", T::ProviderFails.text(lang), e.chars().take(160).collect::<String>()),
                    }
                };
                *note.lock().unwrap() = said;
            });
        }
        Act::Back => next.set(return_to.back()),
    }
}
