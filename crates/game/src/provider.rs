//! The screen for game masters other than Gemini: NVIDIA NIM, OpenRouter, Cloudflare Workers
//! AI, and any service of the player's own that speaks the OpenAI protocol. Each has its keys
//! (they take turns), its models for the telling in the order they are tried, and its
//! speaking models for the narrator's voice. The models a provider offers can be fetched and
//! picked from.

use crate::art::Art;
use crate::i18n::{Lang, T};
use crate::menu::{ReturnTo, Screen, screen_root, spawn_panel};
use crate::store::{Settings, Store};
use crate::theme::*;
use crate::widgets::{Activate, BUTTON_HEIGHT, backing, button, scroll_list, text_button};
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigation;
use llm::providers::{Kind, Rotation};
use std::sync::{Arc, Mutex};

#[derive(Component, Clone, Copy, PartialEq)]
enum Field {
    /// Cloudflare's account id, or the address of a service of one's own.
    Address,
    Key,
    Model,
    Voice,
}

#[derive(Component, Clone, Copy)]
enum Act {
    Tab(Kind),
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

#[derive(Component)]
struct Root;

/// Where the models on offer are listed.
#[derive(Component)]
struct Offered;

#[derive(Component)]
struct Outcome;

/// What a thread came back with: the models on offer.
type Fetched = Arc<Mutex<Option<Result<Vec<String>, String>>>>;

/// The screen as its owner left it.
#[derive(Resource)]
pub struct Desk {
    pub tab: Kind,
    voices: bool,
    /// What is typed and not yet added, kept over a redraw.
    typed_key: String,
    typed_model: String,
    /// The models the provider offers, as last fetched, and for which list.
    offered: Vec<String>,
    offered_for: Option<(Kind, bool)>,
    fetched: Fetched,
    note: Arc<Mutex<String>>,
    /// Raised when the screen must be drawn again.
    redraw: bool,
    /// What the list of models on offer was last drawn from.
    shown: String,
}

impl Default for Desk {
    fn default() -> Self {
        Self { tab: Kind::Nim, voices: false, typed_key: String::new(), typed_model: String::new(), offered: Vec::new(), offered_for: None, fetched: Arc::default(), note: Arc::default(), redraw: false, shown: String::new() }
    }
}

pub struct ProviderPlugin;

impl Plugin for ProviderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Desk>()
            .add_systems(OnEnter(Screen::Provider), enter)
            .add_systems(Update, (arrived, redraw, offered, show_note, escape).chain().run_if(in_state(Screen::Provider)))
            .add_observer(on_activate);
    }
}

fn field(parent: &mut ChildSpawnerCommands, art: &Art, fonts: &Fonts, which: Field, value: &str, width: f32) {
    parent.spawn(Node { width: px(width), height: px(BUTTON_HEIGHT), padding: UiRect::axes(px(4), px(0)), align_items: AlignItems::Center, overflow: Overflow::clip(), flex_shrink: 0.0, ..default() }).with_children(|slot| {
        slot.spawn(backing(art.field.clone(), 4.0));
        let text = EditableText { allow_newlines: false, max_characters: Some(300), ..EditableText::new(value) };
        slot.spawn((which, AutoDirectionalNavigation::default(), Node { width: percent(100), ..default() }, text, TextLayout::no_wrap(), fonts.body(), TextColor(color(INK)), TextCursorStyle { color: color(INK), selection_color: color(BRASS_LIGHT), ..default() }));
    });
}

/// The end of a key, to tell keys apart without showing them.
fn tail(key: &str) -> String {
    let n = key.chars().count();
    format!("…{}", key.chars().skip(n.saturating_sub(4)).collect::<String>())
}

fn enter(mut commands: Commands, art: Res<Art>, fonts: Res<Fonts>, mut settings: ResMut<Settings>, mut desk: ResMut<Desk>, lang: Res<Lang>) {
    desk.note.lock().unwrap().clear();
    desk.shown.clear();
    // Opened on the provider that is the master now, if one is.
    if let Some(active) = settings.active_provider() {
        desk.tab = active;
    }
    build(&mut commands, &art, &fonts, &mut settings, &desk, *lang);
}

fn build(commands: &mut Commands, art: &Art, fonts: &Fonts, settings: &mut Settings, desk: &Desk, lang: Lang) {
    let ink = TextColor(color(INK));
    let faded = TextColor(color(FADED));
    let on: Vec<Kind> = settings.providers.iter().filter(|p| p.on).map(|p| p.kind).collect();
    let this = settings.provider_mut(desk.tab).clone();
    let keys = this.plain_keys();
    let chosen = if desk.voices { &this.voices } else { &this.models };
    let line = || Node { column_gap: px(4), row_gap: px(2), flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, ..default() };
    commands.spawn((Root, screen_root(Screen::Provider))).with_children(|root| {
        let sheet = Node { margin: UiRect::top(px(3)), width: px(468), flex_direction: FlexDirection::Column, row_gap: px(3), ..default() };
        spawn_panel(root, art, sheet, |sheet| {
            // The providers: the one being looked at in brackets, those switched on marked.
            sheet.spawn(line()).with_children(|tabs| {
                for kind in Kind::ALL {
                    let mark = if on.contains(&kind) { "● " } else { "" };
                    let name = if kind == desk.tab { format!("[{mark}{}]", kind.name()) } else { format!("{mark}{}", kind.name()) };
                    text_button(tabs, art, fonts, name, Act::Tab(kind));
                }
            });
            sheet.spawn(line()).with_children(|row| {
                row.spawn((Text::default(), T::ProviderUse, fonts.body(), ink));
                button(row, art, fonts, if this.on { T::Yes } else { T::No }, true, Act::Toggle);
                match desk.tab {
                    Kind::Cloudflare => {
                        row.spawn((Text::default(), T::ProvAccount, fonts.body(), ink));
                        field(row, art, fonts, Field::Address, &this.account, 190.0);
                    }
                    Kind::Custom => {
                        row.spawn((Text::default(), T::ProviderUrl, fonts.body(), ink));
                        field(row, art, fonts, Field::Address, &this.url, 210.0);
                    }
                    _ => {}
                }
            });
            // The keys take turns; a press on one takes it away.
            sheet.spawn(line()).with_children(|row| {
                row.spawn((Text::new(format!("{} {}", T::ProvKeys.text(lang), keys.len())), fonts.body(), ink));
                for (n, key) in keys.iter().enumerate().take(8) {
                    text_button(row, art, fonts, format!("{} x", tail(key)), Act::DropKey(n));
                }
                field(row, art, fonts, Field::Key, &desk.typed_key, 130.0);
                button(row, art, fonts, T::ProvAddKey, true, Act::AddKey);
            });
            if desk.tab == Kind::Nim {
                sheet.spawn((Text::default(), T::ProvNimSlow, fonts.body(), TextColor(color(WAX_DARK))));
            }
            sheet.spawn(line()).with_children(|row| {
                for (voices, label) in [(false, T::ProvChat), (true, T::ProvVoice)] {
                    let name = if voices == desk.voices { format!("[{}]", label.text(lang)) } else { label.text(lang).to_string() };
                    text_button(row, art, fonts, name, Act::Section(voices));
                }
                if desk.voices {
                    row.spawn((Text::default(), T::ProvVoiceName, fonts.body(), ink));
                    field(row, art, fonts, Field::Voice, &this.voice, 90.0);
                } else {
                    row.spawn((Text::default(), T::ProvOrder, fonts.body(), faded));
                }
            });
            // Those chosen, the first asked first.
            sheet.spawn(scroll_list(Node { flex_direction: FlexDirection::Column, row_gap: px(1), ..default() }, 50.0)).with_children(|rows| {
                if chosen.is_empty() {
                    rows.spawn((Text::default(), T::ProvNone, fonts.body(), faded));
                }
                for (n, model) in chosen.iter().enumerate() {
                    rows.spawn(Node { column_gap: px(3), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                        row.spawn((Node { width: px(300), overflow: Overflow::clip(), flex_shrink: 0.0, ..default() }, Text::new(format!("{}. {model}", n + 1)), TextLayout::no_wrap(), fonts.body(), ink));
                        text_button(row, art, fonts, "▲".to_string(), Act::Up(n));
                        text_button(row, art, fonts, "▼".to_string(), Act::Down(n));
                        text_button(row, art, fonts, "x".to_string(), Act::Drop(n));
                    });
                }
            });
            sheet.spawn(line()).with_children(|row| {
                field(row, art, fonts, Field::Model, &desk.typed_model, 220.0);
                button(row, art, fonts, T::ProvAdd, true, Act::AddModel);
                button(row, art, fonts, T::ProvFetch, true, Act::Fetch);
            });
            // Those on offer, narrowed by what is typed above; a press chooses one.
            sheet.spawn((Offered, scroll_list(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(3), row_gap: px(2), ..default() }, 52.0)));
            sheet.spawn(line()).with_children(|row| {
                button(row, art, fonts, T::Back, true, Act::Back);
                button(row, art, fonts, T::TestGm, true, Act::Test);
                row.spawn((Outcome, Node { width: px(300), overflow: Overflow::clip(), ..default() }, Text::default(), TextLayout::no_wrap(), fonts.body(), ink));
            });
        });
    });
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
    desk.shown.clear();
}

fn redraw(mut commands: Commands, art: Res<Art>, fonts: Res<Fonts>, mut settings: ResMut<Settings>, mut desk: ResMut<Desk>, lang: Res<Lang>, roots: Query<Entity, With<Root>>) {
    if !std::mem::take(&mut desk.redraw) {
        return;
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    desk.shown.clear();
    build(&mut commands, &art, &fonts, &mut settings, &desk, *lang);
}

/// The list of models on offer follows what is typed, without the rest of the screen being
/// drawn again under the cursor.
fn offered(mut commands: Commands, art: Res<Art>, fonts: Res<Fonts>, mut desk: ResMut<Desk>, lists: Query<Entity, With<Offered>>, fields: Query<(&Field, &EditableText)>, lang: Res<Lang>) {
    let Ok(list) = lists.single() else { return };
    let typed = fields.iter().find(|(f, _)| **f == Field::Model).map(|(_, t)| t.value().to_string()).unwrap_or_default();
    let stale = desk.offered_for != Some((desk.tab, desk.voices));
    let print = format!("{:?}|{}|{typed}|{}|{stale}", desk.tab, desk.voices, desk.offered.len());
    if desk.shown == print {
        return;
    }
    desk.shown = print;
    let shown = if stale { Vec::new() } else { narrowed(&desk, &typed) };
    let more = !stale && desk.offered.len() > shown.len();
    commands.entity(list).despawn_children().with_children(|rows| {
        if shown.is_empty() {
            let words = if !stale && !desk.offered.is_empty() { T::ProvNoMatch } else { T::ProvFetchHint };
            rows.spawn((Text::new(words.text(*lang)), fonts.body(), TextColor(color(FADED))));
        }
        for (n, id) in shown.iter().enumerate() {
            text_button(rows, &art, &fonts, id.clone(), Act::Pick(n));
        }
        if more && shown.len() >= 60 {
            rows.spawn((Text::new("…"), fonts.body(), TextColor(color(FADED))));
        }
    });
}

fn show_note(desk: Res<Desk>, mut outcome: Query<&mut Text, With<Outcome>>) {
    let said = desk.note.lock().unwrap().clone();
    for mut text in &mut outcome {
        if text.0 != said {
            text.0 = said.clone();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn on_activate(used: On<Activate>, acts: Query<&Act>, fields: Query<(&Field, &EditableText)>, mut settings: ResMut<Settings>, store: Res<Store>, mut desk: ResMut<Desk>, lang: Res<Lang>, mut return_to: ResMut<ReturnTo>, mut next: ResMut<NextState<Screen>>) {
    let Ok(act) = acts.get(used.entity) else { return };
    let typed = |which: Field| fields.iter().find(|(f, _)| **f == which).map(|(_, t)| t.value().to_string().trim().to_string());
    // What is in the fields is kept before anything else: the screen may be drawn again.
    let tab = desk.tab;
    let voices = desk.voices;
    {
        let this = settings.provider_mut(tab);
        if let Some(address) = typed(Field::Address) {
            match tab {
                Kind::Cloudflare => this.account = address,
                Kind::Custom => this.url = address,
                _ => {}
            }
        }
        if let Some(voice) = typed(Field::Voice) {
            this.voice = voice;
        }
    }
    desk.typed_key = typed(Field::Key).unwrap_or_default();
    desk.typed_model = typed(Field::Model).unwrap_or_default();
    desk.redraw = true;
    let shown = narrowed(&desk, &desk.typed_model);
    let this = settings.provider_mut(tab);
    fn list(this: &mut crate::store::ProviderSetting, voices: bool) -> &mut Vec<String> {
        if voices { &mut this.voices } else { &mut this.models }
    }
    match *act {
        Act::Tab(kind) => {
            desk.tab = kind;
            desk.typed_key.clear();
            desk.typed_model.clear();
            desk.note.lock().unwrap().clear();
        }
        Act::Toggle => this.on = !this.on,
        Act::AddKey => {
            // Several keys may be pasted at once, parted by spaces, commas or new lines.
            let pasted = desk.typed_key.clone();
            for key in pasted.split([' ', ',', ';', '\n']).filter(|k| !k.trim().is_empty()) {
                this.add_key(key);
            }
            desk.typed_key.clear();
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
            let model = desk.typed_model.clone();
            let models = list(this, voices);
            if !model.is_empty() && !models.contains(&model) {
                models.push(model);
            }
            desk.typed_model.clear();
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
            *desk.note.lock().unwrap() = T::ProvFetching.text(*lang).to_string();
            std::thread::spawn(move || {
                let listed = if voices { provider.list_voices() } else { provider.list_models() };
                *slot.lock().unwrap() = Some(listed);
            });
        }
        Act::Test => {
            let provider = this.provider(llm::ThinkingSetting::Off);
            let note = desk.note.clone();
            *note.lock().unwrap() = T::TestRunning.text(*lang).to_string();
            let lang = *lang;
            std::thread::spawn(move || {
                // Asked afresh, with nothing resting, so that the word is about now.
                let rotation = Rotation::default();
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
            desk.redraw = false;
        }
        Act::Back => {
            desk.redraw = false;
            next.set(return_to.0.take().unwrap_or(Screen::Settings));
        }
    }
    if let Err(e) = store.save_settings(&settings) {
        error!("could not save settings: {e}");
    }
}

fn escape(keys: Res<ButtonInput<KeyCode>>, mut return_to: ResMut<ReturnTo>, mut next: ResMut<NextState<Screen>>) {
    if keys.just_pressed(KeyCode::Escape) {
        next.set(return_to.0.take().unwrap_or(Screen::Settings));
    }
}
