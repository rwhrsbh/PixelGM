//! The main menu.

use crate::i18n::{Lang, T};
use crate::menu::Screen;
use crate::ui::egui::{self, Align, Layout, Ui, vec2};
use crate::ui::kit::{self, Sheet, button};
use crate::ui::theme::{BIG, c};

/// What the main menu needs to know.
pub struct MainView {
    pub lang: Lang,
    pub has_saves: bool,
    /// What the button of the game's master says: who it is.
    pub master: T,
    /// Where that button leads: the master's own screen.
    pub master_screen: Screen,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MainAct {
    Go(Screen),
    Continue,
    Quit,
}

pub fn main_menu(ui: &mut Ui, view: &MainView) -> Option<MainAct> {
    let t = |text: T| text.text(view.lang);
    let mut act = None;
    // The game's name and what it is, above the sheet.
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 56.0), Layout::top_down(Align::Center), |ui| {
        ui.add_space(14.0);
        ui.label(egui::RichText::new("PIXEL GM").font(egui::FontId::new(BIG * 2.0, crate::ui::theme::title_family())).color(c(crate::theme::BRASS_LIGHT)));
        ui.add_space(4.0);
        ui.label(egui::RichText::new(t(T::Tagline)).color(c(crate::theme::PARCHMENT_SHADE)));
    });
    let no_saves = t(T::NoSavesYet);
    let sheet = Sheet { id: "main", title: "", about: "", max: vec2(200.0, 260.0), top: 64.0, fixed: false };
    kit::bare_sheet(
        ui,
        sheet,
        |ui| {
            let mut first = None;
            for (label, then, enabled) in [
                (t(T::NewGame), MainAct::Go(Screen::NewGame), true),
                (t(T::Continue), MainAct::Continue, view.has_saves),
                (t(T::LoadGame), MainAct::Go(Screen::Load), view.has_saves),
                (t(T::Multiplayer), MainAct::Go(Screen::Lobby), true),
                (t(view.master), MainAct::Go(view.master_screen), true),
                (t(T::Settings), MainAct::Go(Screen::Settings), true),
                (t(T::Quit), MainAct::Quit, true),
            ] {
                let response = ui.add(button(label).menu().enabled(enabled, no_saves));
                first.get_or_insert(response.clone());
                if response.clicked() {
                    act = Some(then);
                }
            }
            // The keyboard starts on the first button.
            if let Some(first) = first
                && ui.memory(|m| m.focused().is_none())
            {
                first.request_focus();
            }
        },
    );
    act
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn press(label: &str, has_saves: bool) -> Option<MainAct> {
        let view = MainView { lang: Lang::En, has_saves, master: T::MasterGemini, master_screen: Screen::Keys };
        let mut got = None;
        {
            let mut harness = Harness::builder().with_size(vec2(480.0, 270.0)).build_ui(|ui| {
                crate::ui::theme::install(ui.ctx());
                if crate::ui::kit::ready(ui.ctx())
                    && let Some(act) = main_menu(ui, &view)
                {
                    got = Some(act);
                }
            });
            harness.run();
            harness.get_by_label(label).click();
            harness.run();
        }
        got
    }

    #[test]
    fn every_button_leads_where_it_says_with_the_first_click() {
        assert_eq!(press("New game", true), Some(MainAct::Go(Screen::NewGame)));
        assert_eq!(press("Continue", true), Some(MainAct::Continue));
        assert_eq!(press("Load", true), Some(MainAct::Go(Screen::Load)));
        assert_eq!(press("Multiplayer", true), Some(MainAct::Go(Screen::Lobby)));
        assert_eq!(press("Master: Gemini", true), Some(MainAct::Go(Screen::Keys)));
        assert_eq!(press("Settings", true), Some(MainAct::Go(Screen::Settings)));
        assert_eq!(press("Quit", true), Some(MainAct::Quit));
    }

    #[test]
    fn with_no_saves_continue_and_load_do_nothing() {
        assert_eq!(press("Continue", false), None);
        assert_eq!(press("Load", false), None);
    }
}
