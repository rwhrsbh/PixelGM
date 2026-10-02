//! Picking up a saved game: "Continue" takes the newest save, the load screen lists them all.

use crate::art::Art;
use crate::i18n::{Lang, T, wait_text};
use crate::menu::{Nav, Screen, screen_root, spawn_panel};
use crate::play;
use crate::store::{SaveEntry, SaveMeta, Store, unix_now};
use crate::theme::*;
use crate::widgets::{Activate, BUTTON_HEIGHT, button, scroll_list, text_button};
use bevy::prelude::*;
use std::path::PathBuf;

/// A button that loads the save at this path.
#[derive(Component, Clone)]
struct LoadSave(PathBuf);

/// Rows of saves that fit on the sheet before the list scrolls.
const VISIBLE_SAVES: f32 = 8.0;
const ROW_GAP: f32 = 2.0;

/// One line for the list: world, place, kind of save, age.
fn describe(meta: &SaveMeta, now: u64, lang: Lang) -> String {
    let kind = if meta.manual { T::ManualSave } else { T::AutoSave };
    let age = wait_text(now.saturating_sub(meta.saved_at) * 1000, lang);
    format!("{} — {} · {} · {} {}", meta.title, meta.place, kind.text(lang), age, T::Ago.text(lang))
}

fn spawn_list(mut commands: Commands, art: Res<Art>, fonts: Res<Fonts>, store: Res<Store>, lang: Res<Lang>) {
    let saves = store.list_saves();
    let now = unix_now();
    commands.spawn(screen_root(Screen::Load)).with_children(|root| {
        let sheet = Node { margin: UiRect::top(px(24)), width: px(400), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() };
        spawn_panel(root, &art, sheet, |sheet| {
            sheet.spawn((Text::default(), T::LoadGame, fonts.title(1), TextColor(color(INK))));
            if saves.is_empty() {
                sheet.spawn((Text::default(), T::NoSaves, fonts.body(), TextColor(color(PARCHMENT_DARK))));
            }
            let list = Node { flex_direction: FlexDirection::Column, row_gap: px(ROW_GAP), ..default() };
            sheet.spawn(scroll_list(list, VISIBLE_SAVES * (BUTTON_HEIGHT + ROW_GAP))).with_children(|rows| {
                for SaveEntry { path, meta } in saves {
                    text_button(rows, &art, &fonts, describe(&meta, now, *lang), LoadSave(path));
                }
            });
            sheet.spawn(Node::default()).with_children(|slot| button(slot, &art, &fonts, T::Back, true, Nav::Back));
        });
    });
}

fn load(path: &PathBuf, store: &Store, commands: &mut Commands, next: &mut NextState<Screen>) {
    match store.load_game(path) {
        Ok((meta, world)) => play::begin(commands, next, world, meta.world_id),
        Err(e) => error!("could not load {}: {e}", path.display()),
    }
}

fn on_activate(used: On<Activate>, saves: Query<&LoadSave>, navs: Query<&Nav>, store: Res<Store>, mut commands: Commands, mut next: ResMut<NextState<Screen>>) {
    if let Ok(LoadSave(path)) = saves.get(used.entity) {
        load(path, &store, &mut commands, &mut next);
    } else if let Ok(Nav::Continue) = navs.get(used.entity)
        && let Some(newest) = store.list_saves().first()
    {
        load(&newest.path, &store, &mut commands, &mut next);
    }
}

pub struct SavesPlugin;

impl Plugin for SavesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::Load), spawn_list).add_observer(on_activate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_save_is_described_by_world_place_kind_and_age() {
        let meta = SaveMeta { world_id: "w".into(), title: "Hollow".into(), place: "Camp".into(), saved_at: 1_000, manual: true };
        assert_eq!(describe(&meta, 1_000 + 3 * 3600 + 120, Lang::En), "Hollow — Camp · manual · 3 h 2 min ago");
        let auto = SaveMeta { manual: false, ..meta };
        assert_eq!(describe(&auto, 1_030, Lang::Ru), "Hollow — Camp · авто · 30 с назад");
        // A clock that went backwards must not underflow.
        assert_eq!(describe(&auto, 900, Lang::En), "Hollow — Camp · auto · 0 s ago");
    }
}
