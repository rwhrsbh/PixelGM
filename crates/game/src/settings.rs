//! The settings screen, and the systems that make a changed setting take effect.

use crate::gm::Gm;
use crate::i18n::{Lang, T};
use crate::master::Master;
use crate::menu::{ReturnTo, Screen};
use crate::store::{Initiative, Settings, Store};
use crate::ui::egui::{Ui, vec2};
use crate::ui::kit::{self, Sheet, button, hint};
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};
use llm::{ThinkingSetting, Tier};

/// One row of the screen: a setting whose value changes by clicking through its choices.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Choice {
    Language,
    /// The screen the interface is drawn for.
    Screen,
    UiSize,
    Window,
    Size,
    Fill,
    OnlineMusic,
    Thinking,
    FirstToken,
    Fallback,
    Predraw,
    Initiative,
    /// How long a player has for a turn in a fight at the table this player hosts.
    TurnTimer,
    Music,
    Sound,
    Voice,
}

impl Choice {
    fn label(self) -> T {
        match self {
            Choice::Language => T::SetLanguage,
            Choice::Screen => T::SetScreen,
            Choice::UiSize => T::SetUiSize,
            Choice::Window => T::SetWindow,
            Choice::Size => T::SetSize,
            Choice::Fill => T::SetFill,
            Choice::OnlineMusic => T::SetOnlineMusic,
            Choice::Thinking => T::SetThinking,
            Choice::FirstToken => T::SetFirstToken,
            Choice::Fallback => T::SetFallback,
            Choice::Predraw => T::SetPredraw,
            Choice::Initiative => T::SetInitiative,
            Choice::TurnTimer => T::SetTurnTimer,
            Choice::Music => T::SetMusic,
            Choice::Sound => T::SetSound,
            Choice::Voice => T::SetVoice,
        }
    }

    fn hint(self) -> T {
        match self {
            Choice::Language => T::LanguageHint,
            Choice::Screen => T::ScreenHint,
            Choice::UiSize => T::UiSizeHint,
            Choice::Window => T::WindowHint,
            Choice::Size => T::SizeHint,
            Choice::Music => T::MusicHint,
            Choice::Sound => T::SoundHint,
            Choice::Thinking => T::ThinkingHint,
            Choice::FirstToken => T::FirstTokenHint,
            Choice::Fallback => T::FallbackHint,
            Choice::Predraw => T::PredrawHint,
            Choice::Initiative => T::InitiativeHint,
            Choice::TurnTimer => T::TurnTimerHint,
            Choice::Voice => T::VoiceHint,
            Choice::Fill => T::FillHint,
            Choice::OnlineMusic => T::OnlineMusicHint,
        }
    }

    /// The word the button shows, when the setting is a word rather than a number.
    fn value(self, s: &Settings) -> Option<T> {
        Some(match self {
            // Seconds, shown by the button's own text instead.
            Choice::FirstToken | Choice::TurnTimer => return None,
            Choice::Language => T::LanguageName,
            // The names of screens are the same in every language: shown by `shown`.
            Choice::Screen if s.screen > 0 => return None,
            Choice::Screen => T::ScreenAuto,
            Choice::UiSize => [T::UiSmaller, T::UiNormal, T::UiLarger][s.ui_size.min(2) as usize],
            Choice::Window => {
                if s.fullscreen {
                    T::Fullscreen
                } else {
                    T::Windowed
                }
            }
            Choice::Thinking => match s.thinking {
                ThinkingSetting::Off => T::ThinkingOff,
                ThinkingSetting::Low => T::ThinkingLow,
                ThinkingSetting::Medium => T::ThinkingMedium,
                ThinkingSetting::High => T::ThinkingHigh,
            },
            Choice::Fallback => match s.lowest_tier {
                Tier::Flash => T::FallbackFlash,
                Tier::FlashLite => T::FallbackLite,
                Tier::Gemma => T::FallbackGemma,
            },
            Choice::Predraw => {
                if s.predraw {
                    T::Yes
                } else {
                    T::No
                }
            }
            Choice::Initiative => match s.initiative {
                Initiative::Off => T::InitiativeOff,
                Initiative::Rare => T::InitiativeRare,
                Initiative::Often => T::InitiativeOften,
            },
            Choice::Size => [T::SizeAuto, T::Size960, T::Size1280, T::Size1440, T::Size1600, T::Size1920, T::Size2560][s.window_size.min(6) as usize],
            Choice::Fill => {
                if s.fill {
                    T::FillScreen
                } else {
                    T::FillSharp
                }
            }
            Choice::OnlineMusic => {
                if s.online_music {
                    T::Yes
                } else {
                    T::No
                }
            }
            Choice::Music => volume_name(s.music),
            Choice::Sound => volume_name(s.sound),
            Choice::Voice => {
                if s.voice {
                    T::Yes
                } else {
                    T::No
                }
            }
        })
    }

    fn advance(self, s: &mut Settings) {
        match self {
            Choice::Language => {
                s.lang = match s.lang {
                    Lang::Ru => Lang::En,
                    Lang::En => Lang::Ru,
                }
            }
            Choice::Screen => s.screen = (s.screen.min(3) + 1) % 4,
            Choice::UiSize => s.ui_size = (s.ui_size.min(2) + 1) % 3,
            Choice::Window => s.fullscreen = !s.fullscreen,
            Choice::Size => s.window_size = (s.window_size + 1) % (crate::screen::SIZES.len() as u8 + 1),
            Choice::Fill => s.fill = !s.fill,
            Choice::OnlineMusic => s.online_music = !s.online_music,
            Choice::Thinking => {
                s.thinking = match s.thinking {
                    ThinkingSetting::Off => ThinkingSetting::Low,
                    ThinkingSetting::Low => ThinkingSetting::Medium,
                    ThinkingSetting::Medium => ThinkingSetting::High,
                    ThinkingSetting::High => ThinkingSetting::Off,
                }
            }
            Choice::Fallback => {
                s.lowest_tier = match s.lowest_tier {
                    Tier::Flash => Tier::FlashLite,
                    Tier::FlashLite => Tier::Gemma,
                    Tier::Gemma => Tier::Flash,
                }
            }
            Choice::FirstToken => s.first_token = next_first_token(s.first_token),
            Choice::Predraw => s.predraw = !s.predraw,
            Choice::TurnTimer => s.turn_timer = match s.turn_timer {
                0 => 30,
                30 => 60,
                60 => 120,
                _ => 0,
            },
            Choice::Initiative => {
                s.initiative = match s.initiative {
                    Initiative::Off => Initiative::Rare,
                    Initiative::Rare => Initiative::Often,
                    Initiative::Often => Initiative::Off,
                }
            }
            Choice::Music => s.music = (s.music.min(4) + 1) % 5,
            Choice::Sound => s.sound = (s.sound.min(4) + 1) % 5,
            Choice::Voice => s.voice = !s.voice,
        }
    }
}

fn volume_name(level: u8) -> T {
    [T::Volume0, T::Volume1, T::Volume2, T::Volume3, T::Volume4][level.min(4) as usize]
}

/// The waits offered for a first word, in seconds: the setting never drops below ten and never
/// rises above five minutes, and a click takes the next offer.
const FIRST_TOKEN_SECONDS: [u16; 10] = [10, 20, 30, 45, 60, 90, 120, 180, 240, 300];

fn next_first_token(current: u16) -> u16 {
    // A wait below the least the setting offers is brought up to it rather than skipped over.
    if current < FIRST_TOKEN_SECONDS[0] {
        return FIRST_TOKEN_SECONDS[0];
    }
    match FIRST_TOKEN_SECONDS.iter().position(|seconds| *seconds == current) {
        Some(at) => FIRST_TOKEN_SECONDS[(at + 1) % FIRST_TOKEN_SECONDS.len()],
        // A wait from a settings file that sits between two offers: the next one up.
        None => FIRST_TOKEN_SECONDS.iter().copied().find(|seconds| *seconds > current).unwrap_or(FIRST_TOKEN_SECONDS[0]),
    }
}

fn first_token_text(settings: &Settings) -> String {
    format!("{} {}", settings.first_token.clamp(10, 300), T::Seconds.text(settings.lang))
}

/// The tabs of the screen, and the settings on each.
const TABS: [(T, &[Choice]); 3] = [
    (T::TabGame, &[Choice::Language, Choice::Initiative, Choice::Predraw, Choice::TurnTimer]),
    (T::TabScreen, &[Choice::Screen, Choice::UiSize, Choice::Window, Choice::Size, Choice::Fill]),
    (T::TabSound, &[Choice::Music, Choice::OnlineMusic, Choice::Sound, Choice::Voice]),
];

/// How the models answer: on the master's screen, beside the models themselves. Gemini
/// alone has tiers to fall back through.
const ANSWER: [Choice; 3] = [Choice::Thinking, Choice::FirstToken, Choice::Fallback];

/// The settings of how the master's models answer, a row and its line each. Gives whether
/// one changed.
pub fn answer_settings(ui: &mut Ui, settings: &mut Settings, gemini: bool) -> bool {
    let lang = settings.lang;
    let mut changed = false;
    for choice in ANSWER.iter().filter(|c| gemini || **c != Choice::Fallback) {
        let values = choice.cycle(settings, lang);
        let names: Vec<&str> = values.iter().map(|(v, _)| v.as_str()).collect();
        let mut index = 0;
        if kit::choice(ui, choice.label().text(lang), &names, &mut index).changed() && index != 0 {
            *settings = values[index].1.clone();
            changed = true;
        }
        hint(ui, choice.hint().text(lang));
        ui.add_space(3.0);
    }
    changed
}

/// How much the master can read and how full the story keeps it, with the switch between
/// counting by the smallest model and by the first, and what the last answer cost. `true`
/// when the switch was turned.
pub fn memory_section(ui: &mut Ui, settings: &mut Settings, limit: &crate::master::ContextLimit) -> bool {
    let lang = settings.lang;
    let t = |text: T| text.text(lang);
    let by = if settings.context_smallest { t(T::ContextBySmallest) } else { t(T::ContextByFirst) };
    let model = if limit.model.is_empty() { String::new() } else { format!(" ({})", limit.model) };
    ui.label(format!("{}: {} {} — {by}{model}", t(T::ContextIs), limit.tokens / 1000, t(T::ContextTokens)));
    if limit.guessed {
        kit::warning(ui, t(T::ContextGuessed));
    }
    let mut on = settings.context_smallest;
    let changed = kit::toggle(ui, t(T::ContextSmallest), &mut on).changed();
    if changed {
        settings.context_smallest = on;
    }
    hint(ui, t(T::ContextSmallestHint));
    hint(ui, t(T::ContextCompacts));
    if let Some((model, spent)) = llm::context::last_spent() {
        ui.add_space(3.0);
        ui.label(format!(
            "{} ({model}): {} {}, {} {} ({}%), {} {}{}",
            t(T::LastSpent),
            t(T::SpentIn),
            spent.prompt,
            t(T::SpentCached),
            spent.cached,
            spent.cache_share(),
            t(T::SpentOut),
            spent.output,
            if spent.thinking > 0 { format!(", {} {}", t(T::SpentThinking), spent.thinking) } else { String::new() }
        ));
    }
    changed
}

/// What the player did on the settings screen besides changing a setting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsAct {
    Back,
    Graphics,
    Master,
}

impl Choice {
    /// What the setting shows for these settings. A language is named in itself.
    fn shown(self, s: &Settings, lang: Lang) -> String {
        if self == Choice::TurnTimer {
            return if s.turn_timer == 0 { T::TimerOff.text(lang).to_string() } else { format!("{} {}", s.turn_timer, T::Seconds.text(lang)) };
        }
        if self == Choice::Screen && s.screen > 0 {
            return ["Full HD", "2K", "4K"][s.screen.min(3) as usize - 1].to_string();
        }
        match self.value(s) {
            Some(T::LanguageName) => T::LanguageName.text(s.lang).to_string(),
            Some(value) => value.text(lang).to_string(),
            None => first_token_text(s),
        }
    }

    /// Every value of the setting, from the one set now on, each with the settings it gives.
    fn cycle(self, s: &Settings, lang: Lang) -> Vec<(String, Settings)> {
        let mut out = vec![(self.shown(s, lang), s.clone())];
        let mut next = s.clone();
        for _ in 0..16 {
            self.advance(&mut next);
            let shown = self.shown(&next, lang);
            if shown == out[0].0 {
                break;
            }
            out.push((shown, next.clone()));
        }
        out
    }
}

/// The settings on tabs: each a name with `◀ value ▶` and, under it, what it does. Gives
/// whether a setting changed, and whether a button of the footer was pressed.
pub fn settings_ui(ui: &mut Ui, settings: &mut Settings, tab: &mut usize) -> (bool, Option<SettingsAct>) {
    let lang = settings.lang;
    let t = |text: T| text.text(lang);
    let mut changed = false;
    let mut act = None;
    let sheet = Sheet { id: "settings", title: t(T::Settings), about: t(T::SettingsAbout), max: vec2(380.0, 320.0), top: 0.0, fixed: true };
    kit::sheet(
        ui,
        sheet,
        |ui| {
            ui.horizontal_wrapped(|row| {
                for (n, (name, _)) in TABS.iter().enumerate() {
                    let mut tab_button = button(t(*name));
                    if n == *tab {
                        tab_button = tab_button.primary();
                    }
                    if row.add(tab_button).clicked() {
                        *tab = n;
                    }
                }
            });
            ui.add_space(4.0);
            for choice in TABS[*tab % TABS.len()].1 {
                let values = choice.cycle(settings, lang);
                let names: Vec<&str> = values.iter().map(|(v, _)| v.as_str()).collect();
                let mut index = 0;
                let row = kit::choice(ui, choice.label().text(lang), &names, &mut index);
                if row.changed() && index != 0 {
                    *settings = values[index].1.clone();
                    changed = true;
                }
                hint(ui, choice.hint().text(lang));
                ui.add_space(3.0);
            }
        },
        |footer| {
            if footer.add(button(t(T::Back)).key("Esc")).clicked() {
                act = Some(SettingsAct::Back);
            }
            if footer.add(button(t(T::Graphics))).clicked() {
                act = Some(SettingsAct::Graphics);
            }
            if footer.add(button(t(T::MasterSettings))).clicked() {
                act = Some(SettingsAct::Master);
            }
        },
    );
    (changed, act)
}

/// The settings screen, drawn with egui; a change is saved at once.
fn settings_screen(mut egui: bevy_egui::EguiContexts, mut settings: ResMut<Settings>, store: Res<Store>, mut tab: Local<usize>, mut return_to: ResMut<ReturnTo>, mut next: ResMut<NextState<Screen>>) {
    let Ok(ctx) = egui.ctx_mut() else { return };
    let Some(mut ui) = kit::screen_ui(ctx, "settings") else { return };
    let mut edited = settings.clone();
    let (changed, act) = settings_ui(&mut ui, &mut edited, &mut tab);
    if changed {
        *settings = edited;
        if let Err(e) = store.save_settings(&settings) {
            error!("could not save settings: {e}");
        }
    }
    match act {
        Some(SettingsAct::Back) => next.set(return_to.back()),
        Some(SettingsAct::Graphics) => next.set(Screen::Graphics),
        // The master's screen comes back here.
        Some(SettingsAct::Master) => {
            return_to.push(Screen::Settings);
            next.set(if settings.active_provider().is_some() { Screen::Provider } else { Screen::Keys });
        }
        None => {}
    }
}

/// How long a silent model is given is the player's to say, and both masters listen: Gemini
/// through its transport, the other providers through their rotation.
fn apply_patience(settings: Res<Settings>, gm: Res<Gm>, master: Res<Master>) {
    if !settings.is_changed() {
        return;
    }
    gm.set_patience(settings.patience());
    master.rotation.set_patience(settings.patience());
}

fn apply_language(settings: Res<Settings>, mut lang: ResMut<Lang>) {
    if *lang != settings.lang {
        *lang = settings.lang;
    }
}

fn apply_window_mode(settings: Res<Settings>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    // A window kept off the desktop for a test stays the size it was given.
    if !window.visible {
        return;
    }
    let wanted = if settings.fullscreen { WindowMode::BorderlessFullscreen(MonitorSelection::Current) } else { WindowMode::Windowed };
    if window.mode != wanted {
        info!("the window goes {}", if settings.fullscreen { "fullscreen" } else { "windowed" });
        window.mode = wanted;
    }
}

/// Switching the keyboard layout with Alt+Shift can leave the game thinking Alt or Shift is
/// still held, and then typing does nothing until the window loses focus. What the system says
/// is held is taken over what the game believes.
#[cfg(windows)]
fn unstick_keys(mut keyboard: ResMut<ButtonInput<KeyCode>>, windows: Query<&Window, With<PrimaryWindow>>, mut released: MessageWriter<bevy::input::keyboard::KeyboardInput>, focused: Query<Entity, With<PrimaryWindow>>) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    // A window kept off the desktop is driven by a test script, whose keys are not the keyboard's.
    if !windows.iter().any(|w| w.focused && w.visible) {
        return;
    }
    let Ok(window) = focused.single() else { return };
    for (code, vk, key) in [
        (KeyCode::AltLeft, 0xA4, bevy::input::keyboard::Key::Alt),
        (KeyCode::AltRight, 0xA5, bevy::input::keyboard::Key::Alt),
        (KeyCode::ShiftLeft, 0xA0, bevy::input::keyboard::Key::Shift),
        (KeyCode::ShiftRight, 0xA1, bevy::input::keyboard::Key::Shift),
        (KeyCode::ControlLeft, 0xA2, bevy::input::keyboard::Key::Control),
        (KeyCode::ControlRight, 0xA3, bevy::input::keyboard::Key::Control),
        // Win+Space switches the layout too.
        (KeyCode::SuperLeft, 0x5B, bevy::input::keyboard::Key::Super),
        (KeyCode::SuperRight, 0x5C, bevy::input::keyboard::Key::Super),
    ] {
        // SAFETY: GetAsyncKeyState only reads the state of a key.
        let down = unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000 != 0;
        if keyboard.pressed(code) && !down {
            keyboard.release(code);
            released.write(bevy::input::keyboard::KeyboardInput { key_code: code, logical_key: key, state: bevy::input::ButtonState::Released, text: None, repeat: false, window });
        }
    }
}

#[cfg(not(windows))]
fn unstick_keys() {}

/// F11 and Alt+Enter switch between the window and the whole screen from anywhere.
fn fullscreen_shortcut(keyboard: Res<ButtonInput<KeyCode>>, mut settings: ResMut<Settings>, store: Res<Store>) {
    let alt = keyboard.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    if keyboard.just_pressed(KeyCode::F11) || (alt && keyboard.just_pressed(KeyCode::Enter)) {
        settings.fullscreen = !settings.fullscreen;
        if let Err(e) = store.save_settings(&settings) {
            error!("could not save settings: {e}");
        }
    }
}

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, unstick_keys)
            .add_systems(Update, (fullscreen_shortcut, apply_patience, apply_language, apply_window_mode).chain())
            .add_systems(bevy_egui::EguiPrimaryContextPass, settings_screen.in_set(crate::ui::UiSet::Screens).run_if(in_state(Screen::Settings)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every setting on the screen, from all the tabs.
    fn all() -> Vec<Choice> {
        TABS.iter().flat_map(|(_, choices)| choices.iter().copied()).chain(ANSWER).collect()
    }

    #[test]
    fn every_choice_cycles_back_to_where_it_started() {
        for choice in all() {
            let start = Settings::default();
            let mut s = start.clone();
            let mut steps = 0;
            loop {
                choice.advance(&mut s);
                steps += 1;
                if s == start {
                    break;
                }
                assert!(steps <= 10, "{choice:?} never returns");
            }
            assert!(steps >= 2, "{choice:?} has only one value");
        }
    }

    #[test]
    fn advancing_a_choice_changes_only_what_it_shows() {
        for choice in all() {
            let mut s = Settings::default();
            let before = s.clone();
            choice.advance(&mut s);
            let changed = [before.lang != s.lang, before.fullscreen != s.fullscreen, before.thinking != s.thinking, before.first_token != s.first_token, before.lowest_tier != s.lowest_tier, before.predraw != s.predraw, before.initiative != s.initiative, before.music != s.music, before.sound != s.sound, before.voice != s.voice, before.grade != s.grade, before.window_size != s.window_size, before.fill != s.fill, before.online_music != s.online_music, before.screen != s.screen, before.ui_size != s.ui_size, before.turn_timer != s.turn_timer];
            assert_eq!(changed.iter().filter(|c| **c).count(), 1, "{choice:?}");
        }
    }

    #[test]
    fn the_wait_for_a_first_token_stays_between_ten_seconds_and_five_minutes() {
        let mut s = Settings::default();
        for _ in 0..40 {
            Choice::FirstToken.advance(&mut s);
            assert!((10..=300).contains(&s.first_token), "{} s", s.first_token);
        }
        // A settings file may hold a wait that is not one of the offers: it is rounded up.
        assert_eq!(next_first_token(15), 20);
        assert_eq!(next_first_token(300), 10);
        assert_eq!(next_first_token(7), 10);
    }

    #[test]
    fn every_setting_is_on_a_tab_or_the_masters_screen_and_says_what_it_does() {
        let shown = all();
        for choice in [Choice::TurnTimer, Choice::Language, Choice::Screen, Choice::UiSize, Choice::Window, Choice::Size, Choice::Fill, Choice::Music, Choice::OnlineMusic, Choice::Sound, Choice::Voice, Choice::Thinking, Choice::FirstToken, Choice::Fallback, Choice::Predraw, Choice::Initiative] {
            assert!(shown.contains(&choice), "{choice:?} is on no tab");
            assert!(!choice.hint().text(Lang::Ru).is_empty(), "{choice:?}");
        }
    }

    #[test]
    fn a_tab_and_a_click_on_a_value_change_that_setting_with_the_first_click() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        let state = std::cell::RefCell::new((Settings { lang: Lang::En, ..Settings::default() }, 0usize, false));
        let mut harness = Harness::builder().with_size(vec2(480.0, 400.0)).build_ui(|ui| {
            crate::ui::theme::install(ui.ctx());
            if crate::ui::kit::ready(ui.ctx()) {
                let (settings, tab, changed) = &mut *state.borrow_mut();
                *changed |= settings_ui(ui, settings, tab).0;
            }
        });
        harness.run();
        harness.get_by_label("Sound").click();
        harness.run();
        assert_eq!(state.borrow().1, 2);
        let before = state.borrow().0.music;
        harness.get_by_label_contains("Music: ").click();
        harness.run();
        let after = state.borrow().0.music;
        assert!(state.borrow().2 && after != before, "{before} -> {after}");
    }
}
