//! The screen for a game master of one's own: any service that speaks the OpenAI protocol —
//! OpenAI, OpenRouter, or a model running on this machine in Ollama or LM Studio — in place of
//! Gemini. An address, a model and a key, a switch, and a test.

use crate::art::Art;
use crate::i18n::{Lang, T};
use crate::menu::{Nav, Screen, screen_root, spawn_panel};
use crate::store::{Settings, Store};
use crate::theme::*;
use crate::widgets::{Activate, BUTTON_HEIGHT, backing, button};
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigation;
use std::sync::{Arc, Mutex};

#[derive(Component, Clone, Copy, PartialEq)]
enum Field {
    Url,
    Model,
    Key,
}

#[derive(Component, Clone, Copy)]
enum Act {
    Toggle,
    Save,
    Test,
}

#[derive(Component)]
struct Toggle;

#[derive(Component)]
struct Outcome;

/// What the last test came to, filled in by the thread that ran it.
#[derive(Resource, Default)]
struct Tested(Arc<Mutex<Option<String>>>);

pub struct ProviderPlugin;

impl Plugin for ProviderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tested>()
            .add_systems(OnEnter(Screen::Provider), spawn)
            .add_systems(Update, (show_toggle, show_test, escape).run_if(in_state(Screen::Provider)))
            .add_observer(on_activate);
    }
}

fn field(parent: &mut ChildSpawnerCommands, art: &Art, fonts: &Fonts, which: Field, value: &str, secret: bool) {
    parent.spawn(Node { width: px(300), height: px(BUTTON_HEIGHT), padding: UiRect::axes(px(5), px(0)), align_items: AlignItems::Center, overflow: Overflow::clip(), ..default() }).with_children(|slot| {
        slot.spawn(backing(art.field.clone(), 4.0));
        let text = EditableText { allow_newlines: false, max_characters: Some(300), ..EditableText::new(if secret { "" } else { value }) };
        slot.spawn((which, AutoDirectionalNavigation::default(), Node { width: percent(100), ..default() }, text, TextLayout::no_wrap(), fonts.body(), TextColor(color(INK)), TextCursorStyle { color: color(INK), selection_color: color(BRASS_LIGHT), ..default() }));
    });
}

fn spawn(mut commands: Commands, art: Res<Art>, fonts: Res<Fonts>, settings: Res<Settings>, tested: Res<Tested>) {
    *tested.0.lock().unwrap() = None;
    let ink = TextColor(color(INK));
    let faded = TextColor(color(FADED));
    commands.spawn(screen_root(Screen::Provider)).with_children(|root| {
        let sheet = Node { margin: UiRect::top(px(10)), width: px(440), flex_direction: FlexDirection::Column, row_gap: px(5), ..default() };
        spawn_panel(root, &art, sheet, |sheet| {
            sheet.spawn((Text::default(), T::Provider, fonts.title(1), ink));
            sheet.spawn((Text::default(), T::ProviderHelp, fonts.body(), faded));
            for (label, which, value, secret) in [(T::ProviderUrl, Field::Url, settings.custom_url.clone(), false), (T::ProviderModel, Field::Model, settings.custom_model.clone(), false), (T::ProviderKey, Field::Key, String::new(), true)] {
                sheet.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                    line.spawn((Node { width: px(110), flex_shrink: 0.0, ..default() }, Text::default(), label, fonts.body(), ink));
                    field(line, &art, &fonts, which, &value, secret);
                });
            }
            let kept = if settings.custom_key.is_empty() { T::ProviderNoKey } else { T::ProviderKeyKept };
            sheet.spawn((Text::default(), kept, fonts.body(), faded));
            sheet.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                line.spawn((Text::default(), T::ProviderUse, fonts.body(), ink));
                line.spawn(Node::default()).with_children(|slot| button(slot, &art, &fonts, if settings.custom_on { T::Yes } else { T::No }, true, (Act::Toggle, Toggle)));
            });
            sheet.spawn(Node { column_gap: px(6), ..default() }).with_children(|line| {
                button(line, &art, &fonts, T::Back, true, Nav::Go(Screen::Settings));
                button(line, &art, &fonts, T::ProviderSave, true, Act::Save);
                button(line, &art, &fonts, T::TestGm, true, Act::Test);
            });
            sheet.spawn((Outcome, Text::default(), fonts.body(), ink));
        });
    });
}

fn read_fields(fields: &Query<(&Field, &EditableText)>, settings: &mut Settings) {
    for (which, text) in fields {
        let value = text.value().to_string();
        match which {
            Field::Url => settings.custom_url = value.trim().to_string(),
            Field::Model => settings.custom_model = value.trim().to_string(),
            // An empty key field keeps the key already sealed.
            Field::Key if !value.trim().is_empty() => settings.set_custom_key(&value),
            Field::Key => {}
        }
    }
}

fn on_activate(used: On<Activate>, acts: Query<&Act>, fields: Query<(&Field, &EditableText)>, mut settings: ResMut<Settings>, store: Res<Store>, tested: Res<Tested>, lang: Res<Lang>) {
    let Ok(act) = acts.get(used.entity) else { return };
    read_fields(&fields, &mut settings);
    match act {
        Act::Toggle => settings.custom_on = !settings.custom_on,
        Act::Save => {}
        Act::Test => {
            let slot = tested.0.clone();
            *slot.lock().unwrap() = Some(T::TestRunning.text(*lang).to_string());
            let custom = llm::openai::Custom { url: settings.custom_url.clone(), model: settings.custom_model.clone(), key: settings.custom().map(|c| c.key).unwrap_or_default() };
            let lang = *lang;
            std::thread::spawn(move || {
                let said = match custom.complete("Answer with one JSON object and nothing else.", "Reply with {\"ok\": true}.") {
                    Ok(text) => format!("{} {}", T::ProviderWorks.text(lang), text.chars().take(60).collect::<String>()),
                    Err(e) => format!("{} {}", T::ProviderFails.text(lang), e.chars().take(160).collect::<String>()),
                };
                *slot.lock().unwrap() = Some(said);
            });
        }
    }
    if let Err(e) = store.save_settings(&settings) {
        error!("could not save settings: {e}");
    }
}

fn show_toggle(settings: Res<Settings>, toggles: Query<&Children, With<Toggle>>, mut labels: Query<&mut T>) {
    for children in &toggles {
        for child in children {
            if let Ok(mut label) = labels.get_mut(*child) {
                let wanted = if settings.custom_on { T::Yes } else { T::No };
                if *label != wanted {
                    *label = wanted;
                }
            }
        }
    }
}

fn show_test(tested: Res<Tested>, mut outcome: Query<&mut Text, With<Outcome>>) {
    let said = tested.0.lock().unwrap().clone().unwrap_or_default();
    for mut text in &mut outcome {
        if text.0 != said {
            text.0 = said.clone();
        }
    }
}

fn escape(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<Screen>>) {
    if keys.just_pressed(KeyCode::Escape) {
        next.set(Screen::Settings);
    }
}
