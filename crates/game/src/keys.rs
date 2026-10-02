//! The key screen: enter Gemini keys, see which ones work, and watch how much of each model
//! is left.

use crate::art::Art;
use crate::gm::{Gm, Health, KeyBook, masked};
use crate::i18n::{Lang, T};
use crate::menu::{Nav, Screen, screen_root, spawn_panel};
use crate::store::Settings;
use crate::theme::*;
use crate::widgets::{PixelButton, backing, button};
use bevy::input_focus::{AutoFocus, InputFocus};
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use llm::{Availability, KeyId, Tier};

#[derive(Component, Clone, Copy)]
enum KeyAction {
    Add,
    Remove(KeyId),
    Recheck(KeyId),
}

#[derive(Component)]
struct KeyInput;

#[derive(Component)]
struct KeyList;

#[derive(Component)]
struct ModelList;

const FIELD_BORDER: f32 = 4.0;
/// Where a player gets a key. An address, so it is the same in every language.
const KEY_PAGE: &str = "aistudio.google.com/apikey";

/// A wait rounded up, in the two largest units that matter.
pub fn wait_text(ms: u64, lang: Lang) -> String {
    let secs = ms.div_ceil(1000);
    let (hours, minutes) = (secs / 3600, secs % 3600 / 60);
    if hours > 0 {
        format!("{hours} {} {minutes} {}", T::Hours.text(lang), T::Minutes.text(lang))
    } else if minutes > 0 {
        format!("{minutes} {}", T::Minutes.text(lang))
    } else {
        format!("{secs} {}", T::Seconds.text(lang))
    }
}

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
                                Node { width: percent(100), ..default() },
                                EditableText { allow_newlines: false, max_characters: Some(200), ..default() },
                                TextLayout::no_wrap(),
                                fonts.body(),
                                ink,
                                TextCursorStyle { color: color(INK), selection_color: color(BRASS_LIGHT), ..default() },
                            ));
                        });
                        line.spawn(button(&art, &fonts, T::Add, true, KeyAction::Add));
                    });
                    left.spawn((Text::default(), T::KeysInputHint, fonts.body(), TextColor(color(PARCHMENT_DARK))));
                    left.spawn((column(2.0), KeyList));
                });
                columns.spawn(Node { flex_grow: 1.0, ..column(3.0) }).with_children(|right| {
                    right.spawn((Text::default(), T::ModelsTitle, fonts.body(), TextColor(color(WAX_DARK))));
                    right.spawn((Text::default(), T::ModelsLegend, fonts.body(), TextColor(color(PARCHMENT_DARK))));
                    right.spawn((column(1.0), ModelList));
                });
            });
            sheet.spawn(row(0.0)).with_child(button(&art, &fonts, T::Back, true, Nav::Go(Screen::Main)));
        });
    });
}

fn health_text(health: &Health) -> (T, Rgba) {
    match health {
        Health::Checking => (T::KeyChecking, PARCHMENT_DARK),
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
                    line.spawn(button(&art, &fonts, T::Recheck, true, KeyAction::Recheck(entry.id)));
                }
                line.spawn(button(&art, &fonts, T::Remove, true, KeyAction::Remove(entry.id)));
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
            rows.spawn((Text::default(), T::ModelsUnknown, fonts.body(), TextColor(color(PARCHMENT_DARK))));
        }
        for (id, tier, availability) in models {
            // Models below the chosen fallback tier are listed, but dimmed: they will not be used.
            let allowed = tier <= settings.lowest_tier;
            let name = if allowed { INK } else { PARCHMENT_DARK };
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

fn on_click(
    click: On<Pointer<Click>>,
    buttons: Query<(&KeyAction, &PixelButton)>,
    gm: Res<Gm>,
    mut book: ResMut<KeyBook>,
    mut input: Query<&mut EditableText, With<KeyInput>>,
) {
    let Ok((action, _)) = buttons.get(click.entity) else { return };
    match *action {
        KeyAction::Add => {
            if let Ok(mut input) = input.single_mut() {
                add_from_input(&gm, &mut book, &mut input);
            }
        }
        KeyAction::Remove(id) => gm.remove_key(&mut book, id),
        KeyAction::Recheck(id) => gm.recheck(&mut book, id),
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
            .add_systems(Update, (submit_on_enter, redraw_keys, redraw_models).chain().run_if(in_state(Screen::Keys)))
            .add_observer(on_click);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_are_rounded_up_and_use_the_largest_units() {
        assert_eq!(wait_text(29_594_000, Lang::Ru), "8 ч 13 мин");
        assert_eq!(wait_text(125_000, Lang::En), "2 min");
        assert_eq!(wait_text(1, Lang::En), "1 s");
        assert_eq!(wait_text(59_001, Lang::Ru), "1 мин");
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
