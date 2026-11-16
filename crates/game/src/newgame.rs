//! Starting a game: the host says what world they want, in three steps, and waits while the
//! game master invents and draws it.

use crate::coop::Session;
use crate::gm::{Gm, Health, KeyBook, Trouble, trouble_text};
use crate::i18n::{Lang, T};
use crate::master::{Answer, Ask, Master};
use crate::menu::{ReturnTo, Screen};
use crate::play;
use crate::store::{Settings, Store};
use crate::ui::egui::{self, Ui, vec2};
use crate::ui::kit::{self, Sheet, button, choice, hint, toggle, warning};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};
use world::Difficulty;

/// The world being made: what was asked for, how far the form has come, how long the
/// master has taken, and what went wrong.
#[derive(Resource, Default)]
struct Making {
    request: String,
    /// Which of the form's three steps is shown.
    step: usize,
    seconds: f32,
    trouble: Option<Trouble>,
    /// Answers still to come to a making that was called off: they are not the world asked for now.
    stale: u32,
    /// The table's content switches; both are on unless the host turns them off here.
    no_violence: bool,
    no_sex: bool,
    /// How the table plays: by the book or with inventions, with caps or without, and how
    /// ability scores are made.
    table: world::TableRules,
}

const STEPS: usize = 3;
/// The most a wish may say.
const WISH_MOST: usize = 1500;

/// What the player did on the form.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FormAct {
    Back,
    Create,
    Keys,
}

/// What the player did while the world is being made.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum WaitAct {
    Cancel,
    Retry,
    Keys,
}

/// The form: the wish; then how the game goes; then its rules. Each setting shows what it
/// is set to and moves either way; the buttons stay pinned to the sheet's bottom.
fn form(ui: &mut Ui, making: &mut Making, can_ask: bool, lang: Lang) -> Option<FormAct> {
    let t = |text: T| text.text(lang);
    let mut act = None;
    let steps = [t(T::NwStepWorld), t(T::NwStepGame), t(T::NwStepRules)];
    let title = format!("{} · {}/{STEPS} {}", t(T::NewWorldTitle), making.step + 1, steps[making.step.min(STEPS - 1)]);
    let sheet = Sheet { id: "new_world", title: &title, max: vec2(460.0, 230.0), top: 24.0, fixed: true };
    let last = making.step + 1 == STEPS;
    kit::sheet(
        ui,
        sheet,
        |ui| match making.step {
            0 => {
                ui.label(t(T::NewWorldAsk));
                let field = ui.add(egui::TextEdit::multiline(&mut making.request).desired_rows(5).desired_width(f32::INFINITY).char_limit(WISH_MOST).hint_text("…"));
                if ui.memory(|m| m.focused().is_none()) {
                    field.request_focus();
                }
                hint(ui, format!("{} / {WISH_MOST}", making.request.chars().count()));
                ui.horizontal_wrapped(|ui| {
                    ui.label(t(T::NwSuggest));
                    for idea in [T::NwIdeaDark, T::NwIdeaSea, T::NwIdeaSpace, T::NwIdeaWaste, T::NwIdeaNoir] {
                        if ui.add(button(t(idea))).clicked() {
                            making.request = t(idea).to_string();
                        }
                    }
                });
            }
            1 => {
                let mut mode = usize::from(making.table.sandbox);
                if choice(ui, t(T::NwMode), &[t(T::NwStory), t(T::NwSandbox)], &mut mode).changed() {
                    making.table.sandbox = mode == 1;
                }
                hint(ui, t(T::NwModeHint));
                let mut difficulty = making.table.difficulty as usize;
                if choice(ui, t(T::NwDifficulty), &[t(T::NwCasual), t(T::NwHonest), t(T::NwHardcore)], &mut difficulty).changed() {
                    making.table.difficulty = [Difficulty::Casual, Difficulty::Honest, Difficulty::Hardcore][difficulty];
                }
                hint(ui, t([T::NwCasualHint, T::NwHonestHint, T::NwHardcoreHint][making.table.difficulty as usize]));
                toggle(ui, t(T::NwLive), &mut making.table.live);
                ui.add_space(4.0);
                let mut violence = usize::from(making.no_violence);
                if choice(ui, t(T::NwViolence), &[t(T::NwGraphic), t(T::NwOffPage)], &mut violence).changed() {
                    making.no_violence = violence == 1;
                }
                let mut sex = usize::from(making.no_sex);
                if choice(ui, t(T::NwSex), &[t(T::NwExplicit), t(T::NwOffPage)], &mut sex).changed() {
                    making.no_sex = sex == 1;
                }
            }
            _ => {
                let mut book = usize::from(making.table.book_only);
                if choice(ui, t(T::NwRules), &[t(T::NwMixed), t(T::NwBook)], &mut book).changed() {
                    making.table.book_only = book == 1;
                }
                let mut limits = !making.table.no_limits;
                if toggle(ui, t(T::NwLimits), &mut limits).changed() {
                    making.table.no_limits = !limits;
                }
                let mut method = making.table.method as usize;
                if choice(ui, t(T::NwMethod), &[t(T::MethodPointBuy), t(T::MethodArray), t(T::MethodRoll)], &mut method).changed() {
                    making.table.method = [rules::AbilityMethod::PointBuy, rules::AbilityMethod::StandardArray, rules::AbilityMethod::Roll][method];
                }
                let mut edition = usize::from(making.table.srd_2024);
                if choice(ui, t(T::NwEdition), &["SRD 5.1", t(T::NwSrd52)], &mut edition).changed() {
                    making.table.srd_2024 = edition == 1;
                }
                if !can_ask {
                    ui.add_space(4.0);
                    warning(ui, t(T::NeedKey));
                }
            }
        },
        |footer| {
            let back = if making.step == 0 { t(T::NwCancel) } else { t(T::Back) };
            if footer.add(button(back).key("Esc")).clicked() {
                act = Some(FormAct::Back);
            }
            if last && !can_ask && footer.add(button(t(T::Keys))).clicked() {
                act = Some(FormAct::Keys);
            }
            let forward = if last { button(t(T::CreateWorld)).primary().enabled(can_ask, t(T::NeedKey)) } else { button(t(T::NwNext)).primary() };
            // Ctrl+Enter from the wish's field: Enter there is a new line.
            let enter = footer.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.ctrl);
            if footer.add(forward.key("Ctrl+Enter")).clicked() || enter && (!last || can_ask) {
                act = Some(FormAct::Create);
            }
        },
    );
    if act.is_none() && kit::back_pressed(ui) {
        act = Some(FormAct::Back);
    }
    // Back and forward within the form are the form's own business.
    match act {
        Some(FormAct::Back) if making.step > 0 => {
            making.step -= 1;
            None
        }
        Some(FormAct::Create) if !last => {
            making.step += 1;
            None
        }
        other => other,
    }
}

/// While the world is made: what is happening and for how long, and what went wrong.
fn waiting(ui: &mut Ui, making: &Making, now: u64, keys_label: T, lang: Lang) -> Option<WaitAct> {
    let t = |text: T| text.text(lang);
    let mut act = None;
    let sheet = Sheet { id: "creating", title: t(T::NewWorldTitle), max: vec2(320.0, 200.0), top: 60.0, fixed: false };
    kit::sheet(
        ui,
        sheet,
        |ui| match &making.trouble {
            Some(trouble) => {
                ui.label(t(T::CreateFailed));
                warning(ui, trouble_text(trouble, now, lang));
            }
            None => {
                ui.label(format!("{}{}", t(T::Creating), dots(making.seconds)));
                hint(ui, t(T::CreatingHint));
                ui.label(format!("{} {} {}", t(T::Elapsed), making.seconds as u32, t(T::Seconds)));
            }
        },
        |footer| {
            if footer.add(button(t(T::NwCancel)).key("Esc")).clicked() {
                act = Some(WaitAct::Cancel);
            }
            if making.trouble.is_some() {
                if footer.add(button(t(keys_label))).clicked() {
                    act = Some(WaitAct::Keys);
                }
                if footer.add(button(t(T::Recheck)).primary()).clicked() {
                    act = Some(WaitAct::Retry);
                }
            }
        },
    );
    if act.is_none() && kit::back_pressed(ui) {
        act = Some(WaitAct::Cancel);
    }
    act
}

/// Whether there is anyone to draw a world: a key of the host's, or guests whose keys can
/// be asked.
fn can_ask(master: &Master, book: &KeyBook, session: &Session, settings: &Settings) -> bool {
    master.fake || !settings.providers().is_empty() || book.entries.iter().any(|e| e.health == Health::Working || e.health == Health::Checking) || !session.players().is_empty()
}

fn start_making(making: &mut Making, gm: &Gm, store: &Store, settings: &Settings, master: &Master) {
    making.seconds = 0.0;
    making.trouble = None;
    master.ask(gm, store, settings, Ask::NewWorld { request: making.request.trim().to_string(), story: !making.table.sandbox });
}

fn keys_screen(settings: &Settings) -> Screen {
    if settings.active_provider().is_some() { Screen::Provider } else { Screen::Keys }
}

#[allow(clippy::too_many_arguments)]
fn form_screen(
    mut egui: EguiContexts,
    mut making: ResMut<Making>,
    (book, master, session, settings, gm, store): (Res<KeyBook>, Res<Master>, Res<Session>, Res<Settings>, Res<Gm>, Res<Store>),
    mut return_to: ResMut<ReturnTo>,
    mut next: ResMut<NextState<Screen>>,
) {
    let Ok(ctx) = egui.ctx_mut() else { return };
    let Some(mut ui) = kit::screen_ui(ctx, "new_world") else { return };
    let can = can_ask(&master, &book, &session, &settings);
    match form(&mut ui, &mut making, can, settings.lang) {
        Some(FormAct::Back) => next.set(return_to.0.take().unwrap_or(Screen::Main)),
        Some(FormAct::Create) => {
            start_making(&mut making, &gm, &store, &settings, &master);
            next.set(Screen::Creating);
        }
        Some(FormAct::Keys) => {
            return_to.0 = Some(Screen::NewGame);
            next.set(keys_screen(&settings));
        }
        None => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn waiting_screen(
    mut egui: EguiContexts,
    mut commands: Commands,
    time: Res<Time>,
    (master, gm, settings, store): (Res<Master>, Res<Gm>, Res<Settings>, Res<Store>),
    mut making: ResMut<Making>,
    mut return_to: ResMut<ReturnTo>,
    mut next: ResMut<NextState<Screen>>,
) {
    for answer in master.take() {
        let Answer::World(result) = answer else { continue };
        // The answer to a making that was called off is not the world asked for now.
        if making.stale > 0 {
            making.stale -= 1;
            continue;
        }
        match result {
            Ok((mut world, notes)) => {
                for note in notes {
                    info!("world repaired: {note}");
                }
                world.violence = !making.no_violence;
                world.sex = !making.no_sex;
                world.table = making.table;
                let world_id = format!("{:x}", crate::store::unix_now());
                making.step = 0;
                play::begin(&mut commands, &mut next, *world, world_id);
                return;
            }
            Err(trouble) => making.trouble = Some(trouble),
        }
    }
    if making.trouble.is_none() {
        making.seconds += time.delta_secs();
    }
    let Ok(ctx) = egui.ctx_mut() else { return };
    // The dots and the seconds move on their own.
    ctx.request_repaint();
    let Some(mut ui) = kit::screen_ui(ctx, "creating") else { return };
    let keys_label = if settings.active_provider().is_some() { T::KeysProvider } else { T::Keys };
    match waiting(&mut ui, &making, gm.dispatcher.now(), keys_label, settings.lang) {
        Some(WaitAct::Cancel) => {
            if making.trouble.is_none() {
                making.stale += 1;
            }
            next.set(Screen::NewGame);
        }
        Some(WaitAct::Retry) => start_making(&mut making, &gm, &store, &settings, &master),
        Some(WaitAct::Keys) => {
            return_to.0 = Some(Screen::NewGame);
            next.set(keys_screen(&settings));
        }
        None => {}
    }
}

/// Dots that come and go, so a long wait visibly is not a freeze.
fn dots(seconds: f32) -> &'static str {
    ["", ".", "..", "..."][(seconds * 2.0) as usize % 4]
}

pub struct NewGamePlugin;

impl Plugin for NewGamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Making>().add_systems(
            EguiPrimaryContextPass,
            (form_screen.run_if(in_state(Screen::NewGame)), waiting_screen.run_if(in_state(Screen::Creating))).in_set(crate::ui::UiSet::Screens),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn dots_cycle_twice_a_second_and_start_empty() {
        assert_eq!([dots(0.0), dots(0.5), dots(1.0), dots(1.5), dots(2.0)], ["", ".", "..", "...", ""]);
    }

    /// Runs the form, pressing the controls named, one per frame. Gives the form as it was
    /// left and the last thing it asked for.
    fn on_form(can_ask: bool, presses: &[&str]) -> (Making, Option<FormAct>) {
        let mut making = Making::default();
        let mut got = None;
        {
            let mut harness = Harness::builder().with_size(vec2(480.0, 340.0)).build_ui(|ui| {
                crate::ui::theme::install(ui.ctx());
                if kit::ready(ui.ctx())
                    && let Some(act) = form(ui, &mut making, can_ask, Lang::En)
                {
                    got = Some(act);
                }
            });
            harness.run();
            for press in presses {
                harness.get_by_label_contains(press).click();
                harness.run();
            }
        }
        (making, got)
    }

    #[test]
    fn the_form_goes_step_by_step_and_back() {
        let (making, act) = on_form(true, &["Next", "Next"]);
        assert_eq!((making.step, act), (2, None));
        let (making, act) = on_form(true, &["Next", "Back"]);
        assert_eq!((making.step, act), (0, None));
        // Back from the first step leaves the form.
        let (_, act) = on_form(true, &["Cancel"]);
        assert_eq!(act, Some(FormAct::Back));
        let (_, act) = on_form(true, &["Next", "Next", "Create the world"]);
        assert_eq!(act, Some(FormAct::Create));
    }

    #[test]
    fn every_setting_says_what_it_is_set_to_once_pressed() {
        // Each press moves a setting one value on; the words on it are the new value's.
        let (making, _) = on_form(true, &["Next", "Mode: story", "Difficulty: honest", "Live world", "Violence: graphic", "Sex: explicit"]);
        assert!(making.table.sandbox && making.table.live && making.no_violence && making.no_sex);
        assert_eq!(making.table.difficulty, Difficulty::Hardcore);
        let (making, _) = on_form(true, &["Next", "Next", "Rules: the book and", "Power held", "Ability scores: Point buy", "Rules edition: SRD 5.1"]);
        assert!(making.table.book_only && making.table.no_limits && making.table.srd_2024);
        assert_eq!(making.table.method, rules::AbilityMethod::StandardArray);
    }

    #[test]
    fn without_a_key_the_world_cannot_be_made_and_the_way_to_one_is_offered() {
        let (_, act) = on_form(false, &["Next", "Next", "Create the world"]);
        assert_eq!(act, None);
        let (_, act) = on_form(false, &["Next", "Next", "Gemini keys"]);
        assert_eq!(act, Some(FormAct::Keys));
    }

    #[test]
    fn an_idea_fills_the_wish() {
        let (making, _) = on_form(true, &["Space"]);
        assert_eq!(making.request, "Space");
    }

    #[test]
    fn the_buttons_stand_in_one_place_from_step_to_step() {
        let mut making = Making::default();
        let mut places = Vec::new();
        {
            let mut harness = Harness::builder().with_size(vec2(960.0, 540.0)).build_ui(|ui| {
                crate::ui::theme::install(ui.ctx());
                if kit::ready(ui.ctx()) {
                    form(ui, &mut making, true, Lang::En);
                }
            });
            harness.run();
            for label in ["Next", "Next", "Create the world"] {
                let next = harness.get_by_label(label);
                places.push(next.rect());
                next.click();
                harness.run();
            }
        }
        assert!(places.windows(2).all(|w| (w[0].top() - w[1].top()).abs() < 0.5), "{places:?}");
    }
}
