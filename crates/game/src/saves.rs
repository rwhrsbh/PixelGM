//! Picking up a saved game: "Continue" takes the newest save, the load screen lists them all.

use crate::i18n::{Lang, T, wait_text};
use crate::menu::{ReturnTo, Screen};
use crate::play;
use crate::store::{SaveEntry, SaveMeta, Store, unix_now};
use crate::ui::egui::vec2;
use crate::ui::kit::{self, Sheet, button, hint};
use bevy::prelude::*;
use std::path::PathBuf;

/// One line for the list: world, place, kind of save, age.
fn describe(meta: &SaveMeta, now: u64, lang: Lang) -> String {
    let kind = if meta.manual { T::ManualSave } else { T::AutoSave };
    let age = wait_text(now.saturating_sub(meta.saved_at) * 1000, lang);
    format!("{} — {} · {} · {} {}", meta.title, meta.place, kind.text(lang), age, T::Ago.text(lang))
}

/// The saves as they were when the screen opened: read from the disk once.
#[derive(Resource, Default)]
struct Listed(Vec<SaveEntry>);

fn list(store: Res<Store>, mut listed: ResMut<Listed>) {
    listed.0 = store.list_saves();
}

/// The load screen, drawn with egui: a wide button for every save, the newest first.
fn load_screen(mut egui: bevy_egui::EguiContexts, listed: Res<Listed>, store: Res<Store>, lang: Res<Lang>, mut commands: Commands, mut next: ResMut<NextState<Screen>>, mut return_to: ResMut<ReturnTo>) {
    let Ok(ctx) = egui.ctx_mut() else { return };
    let Some(mut ui) = kit::screen_ui(ctx, "load") else { return };
    let lang = *lang;
    let t = |text: T| text.text(lang);
    let now = unix_now();
    let mut chosen = None;
    let mut back = false;
    let sheet = Sheet { id: "load", title: t(T::LoadGame), about: t(T::LoadAbout), max: vec2(400.0, 300.0), top: 0.0, fixed: false };
    kit::sheet(
        &mut ui,
        sheet,
        |ui| {
            if listed.0.is_empty() {
                hint(ui, t(T::NoSaves));
            }
            for SaveEntry { path, meta } in &listed.0 {
                if ui.add(button(describe(meta, now, lang)).wide()).clicked() {
                    chosen = Some(path.clone());
                }
            }
        },
        |footer| {
            if footer.add(button(t(T::Back)).key("Esc")).clicked() {
                back = true;
            }
        },
    );
    if let Some(path) = chosen {
        load(&path, &store, &mut commands, &mut next);
    } else if back {
        next.set(return_to.back());
    }
}

fn load(path: &PathBuf, store: &Store, commands: &mut Commands, next: &mut NextState<Screen>) {
    match store.load_game(path) {
        Ok((meta, world)) => play::begin(commands, next, world, meta.world_id),
        Err(e) => error!("could not load {}: {e}", path.display()),
    }
}

/// Loads the newest save, if there is one.
pub fn continue_newest(store: &Store, commands: &mut Commands, next: &mut NextState<Screen>) {
    if let Some(newest) = store.list_saves().first() {
        load(&newest.path, store, commands, next);
    }
}

pub struct SavesPlugin;

impl Plugin for SavesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Listed>()
            .add_systems(OnEnter(Screen::Load), list)
            .add_systems(bevy_egui::EguiPrimaryContextPass, load_screen.in_set(crate::ui::UiSet::Screens).run_if(in_state(Screen::Load)));
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
