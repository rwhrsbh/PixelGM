//! The settings screen, and the systems that make a changed setting take effect.

use crate::art::Art;
use crate::i18n::{Lang, T};
use crate::menu::{Nav, Screen, screen_root, spawn_panel};
use crate::store::{Initiative, Settings, Store};
use crate::theme::*;
use crate::widgets::{Activate, button, scroll_list};
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};
use llm::{ThinkingSetting, Tier};

/// One row of the screen: a setting whose value changes by clicking through its choices.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum Choice {
    Language,
    Window,
    Thinking,
    Fallback,
    Predraw,
    Initiative,
    Music,
    Sound,
    Voice,
    Grade,
}

impl Choice {
    fn label(self) -> T {
        match self {
            Choice::Language => T::SetLanguage,
            Choice::Window => T::SetWindow,
            Choice::Thinking => T::SetThinking,
            Choice::Fallback => T::SetFallback,
            Choice::Predraw => T::SetPredraw,
            Choice::Initiative => T::SetInitiative,
            Choice::Music => T::SetMusic,
            Choice::Sound => T::SetSound,
            Choice::Voice => T::SetVoice,
            Choice::Grade => T::SetGrade,
        }
    }

    fn hint(self) -> Option<T> {
        match self {
            Choice::Thinking => Some(T::ThinkingHint),
            Choice::Fallback => Some(T::FallbackHint),
            Choice::Predraw => Some(T::PredrawHint),
            Choice::Initiative => Some(T::InitiativeHint),
            Choice::Voice => Some(T::VoiceHint),
            Choice::Grade => Some(T::GradeHint),
            _ => None,
        }
    }

    fn value(self, s: &Settings) -> T {
        match self {
            Choice::Language => T::LanguageName,
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
            Choice::Grade => s.grade.label(),
            Choice::Music => volume_name(s.music),
            Choice::Sound => volume_name(s.sound),
            Choice::Voice => {
                if s.voice {
                    T::Yes
                } else {
                    T::No
                }
            }
        }
    }

    fn advance(self, s: &mut Settings) {
        match self {
            Choice::Language => {
                s.lang = match s.lang {
                    Lang::Ru => Lang::En,
                    Lang::En => Lang::Ru,
                }
            }
            Choice::Window => s.fullscreen = !s.fullscreen,
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
            Choice::Predraw => s.predraw = !s.predraw,
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
            Choice::Grade => s.grade = s.grade.next(),
        }
    }
}

fn volume_name(level: u8) -> T {
    [T::Volume0, T::Volume1, T::Volume2, T::Volume3, T::Volume4][level.min(4) as usize]
}

const CHOICES: [Choice; 10] = [Choice::Language, Choice::Window, Choice::Grade, Choice::Music, Choice::Sound, Choice::Voice, Choice::Thinking, Choice::Fallback, Choice::Predraw, Choice::Initiative];

fn spawn_settings(mut commands: Commands, art: Res<Art>, fonts: Res<Fonts>, settings: Res<Settings>) {
    commands.spawn(screen_root(Screen::Settings)).with_children(|root| {
        let sheet = Node { margin: UiRect::top(px(8)), width: px(440), flex_direction: FlexDirection::Column, row_gap: px(5), ..default() };
        spawn_panel(root, &art, sheet, |sheet| {
            sheet.spawn((Text::default(), T::Settings, fonts.title(1), TextColor(color(INK))));
            // More settings than fit on the table at once: the list scrolls.
            sheet.spawn(scroll_list(Node { flex_direction: FlexDirection::Column, row_gap: px(5), ..default() }, 196.0)).with_children(|sheet| {
            for choice in CHOICES {
                sheet.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }).with_children(|block| {
                    block.spawn(Node { align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|line| {
                        line.spawn((Text::default(), choice.label(), fonts.body(), TextColor(color(INK))));
                        line.spawn(Node { width: px(150), flex_direction: FlexDirection::Column, ..default() })
                            .with_children(|slot| button(slot, &art, &fonts, choice.value(&settings), true, choice));
                    });
                    if let Some(hint) = choice.hint() {
                        block.spawn((Text::default(), hint, fonts.body(), TextColor(color(FADED))));
                    }
                });
            }
            });
            sheet.spawn(Node { margin: UiRect::top(px(4)), ..default() }).with_children(|slot| button(slot, &art, &fonts, T::Back, true, Nav::Back));
        });
    });
}

fn on_activate(used: On<Activate>, buttons: Query<&Choice>, mut settings: ResMut<Settings>, store: Res<Store>) {
    let Ok(choice) = buttons.get(used.entity) else { return };
    choice.advance(&mut settings);
    if let Err(e) = store.save_settings(&settings) {
        error!("could not save settings: {e}");
    }
}

/// Each value button shows its setting's current value: the label is a child of the button.
fn show_values(settings: Res<Settings>, buttons: Query<(&Choice, &Children)>, mut labels: Query<&mut T>) {
    if !settings.is_changed() {
        return;
    }
    for (choice, children) in &buttons {
        for child in children {
            if let Ok(mut label) = labels.get_mut(*child) {
                let wanted = choice.value(&settings);
                if *label != wanted {
                    *label = wanted;
                }
            }
        }
    }
}

fn apply_language(settings: Res<Settings>, mut lang: ResMut<Lang>) {
    if *lang != settings.lang {
        *lang = settings.lang;
    }
}

fn apply_window_mode(settings: Res<Settings>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    let wanted = if settings.fullscreen { WindowMode::BorderlessFullscreen(MonitorSelection::Current) } else { WindowMode::Windowed };
    if window.mode != wanted {
        window.mode = wanted;
    }
}

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
        app.add_systems(OnEnter(Screen::Settings), spawn_settings)
            .add_systems(Update, (fullscreen_shortcut, apply_language, apply_window_mode, show_values).chain())
            .add_observer(on_activate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_choice_cycles_back_to_where_it_started() {
        for choice in CHOICES {
            let start = Settings::default();
            let mut s = start.clone();
            let mut steps = 0;
            loop {
                choice.advance(&mut s);
                steps += 1;
                if s == start {
                    break;
                }
                assert!(steps < 10, "{choice:?} never returns");
            }
            assert!(steps >= 2, "{choice:?} has only one value");
        }
    }

    #[test]
    fn advancing_a_choice_changes_only_what_it_shows() {
        for choice in CHOICES {
            let mut s = Settings::default();
            let before = s.clone();
            choice.advance(&mut s);
            let changed = [before.lang != s.lang, before.fullscreen != s.fullscreen, before.thinking != s.thinking, before.lowest_tier != s.lowest_tier, before.predraw != s.predraw, before.initiative != s.initiative, before.music != s.music, before.sound != s.sound, before.voice != s.voice, before.grade != s.grade];
            assert_eq!(changed.iter().filter(|c| **c).count(), 1, "{choice:?}");
        }
    }
}
