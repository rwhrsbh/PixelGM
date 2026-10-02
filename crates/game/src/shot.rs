//! Developer flags for driving and checking the game without a person at the keyboard.
//!
//! - `--do "<step>;<step>;…"` plays a script through the real input path: the steps become
//!   the same window messages a mouse and keyboard produce. Steps: `wait:<seconds>`,
//!   `move:<x>,<y>` and `click:<x>,<y>` (canvas pixels), `type:<text>`, `key:<name>`,
//!   `shot:<file.png>`, `hold:<key>,<seconds>`, `warp:<cell x>,<cell y>`, `quit`.
//! - `--shot <file.png>` is short for "let the picture settle, save it, quit".
//! - `--screen <main|keys|settings>` opens a screen, `--size <W>x<H>` sets the window size,
//!   `--lang <ru|en>` the language, `--keys-file <file>` adds the keys listed in a file,
//!   `--hidden true` keeps the window off the desktop, `--fake-gm true` lets a rule-based
//!   stand-in answer for the game master.

use crate::gm::{Gm, KeyBook};
use crate::i18n::Lang;
use crate::menu::Screen;
use crate::screen::fit;
use crate::store::Settings;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput, NativeKeyCode};
use bevy::input::mouse::MouseButtonInput;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::{CursorMoved, PrimaryWindow, WindowEvent};

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Wait(f32),
    Frames(u32),
    Move(Vec2),
    Mouse(ButtonState),
    Type(String),
    Key(KeyCode, Key, ButtonState),
    Shot(String),
    /// Put the party in a cell of the current location, to test what lies beyond a walk.
    Warp(usize, usize),
    Quit,
}

/// A cell the script asked the party to be moved to; the play screen carries it out.
#[derive(Resource, Default)]
pub struct Warp(pub Option<(usize, usize)>);

#[derive(Clone, Default)]
pub struct Args {
    pub screen: Option<Screen>,
    pub size: Option<(u32, u32)>,
    pub lang: Option<Lang>,
    pub keys_file: Option<String>,
    pub hidden: bool,
    /// Let a rule-based stand-in answer for the game master: no network, no quota.
    pub fake_gm: bool,
    pub script: Vec<Step>,
}

/// Frames for layout, fonts and textures to reach the screen, and for a screenshot to be
/// written before the next step.
const SETTLE_FRAMES: u32 = 40;
/// Frames between the parts of a click, so hover and press are seen as separate moments.
const CLICK_FRAMES: u32 = 3;

fn named_key(name: &str) -> Option<(KeyCode, Key)> {
    Some(match name.to_ascii_lowercase().as_str() {
        "enter" => (KeyCode::Enter, Key::Enter),
        "escape" | "esc" => (KeyCode::Escape, Key::Escape),
        "tab" => (KeyCode::Tab, Key::Tab),
        "backspace" => (KeyCode::Backspace, Key::Backspace),
        "f11" => (KeyCode::F11, Key::F11),
        "up" => (KeyCode::ArrowUp, Key::ArrowUp),
        "down" => (KeyCode::ArrowDown, Key::ArrowDown),
        "left" => (KeyCode::ArrowLeft, Key::ArrowLeft),
        "w" => (KeyCode::KeyW, Key::Character("w".into())),
        "a" => (KeyCode::KeyA, Key::Character("a".into())),
        "s" => (KeyCode::KeyS, Key::Character("s".into())),
        "d" => (KeyCode::KeyD, Key::Character("d".into())),
        "right" => (KeyCode::ArrowRight, Key::ArrowRight),
        _ => return None,
    })
}

fn point(text: &str) -> Option<Vec2> {
    let (x, y) = text.split_once(',')?;
    Some(Vec2::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// Unknown or malformed steps are dropped: a typo in a test script must not crash the game.
pub fn parse_script(script: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    for part in script.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let (verb, arg) = part.split_once(':').unwrap_or((part, ""));
        match verb {
            "wait" => steps.extend(arg.parse().ok().map(Step::Wait)),
            "move" => steps.extend(point(arg).map(Step::Move)),
            "click" => {
                if let Some(at) = point(arg) {
                    steps.extend([
                        Step::Move(at),
                        Step::Frames(CLICK_FRAMES),
                        Step::Mouse(ButtonState::Pressed),
                        Step::Frames(CLICK_FRAMES),
                        Step::Mouse(ButtonState::Released),
                        Step::Frames(CLICK_FRAMES),
                    ]);
                }
            }
            "type" => steps.push(Step::Type(arg.to_string())),
            // Hold a key for a while: `hold:d,1.5` walks right for a second and a half.
            "hold" => {
                if let Some(((code, key), seconds)) = arg.split_once(',').and_then(|(name, time)| Some((named_key(name.trim())?, time.trim().parse().ok()?))) {
                    steps.extend([Step::Key(code, key.clone(), ButtonState::Pressed), Step::Wait(seconds), Step::Key(code, key, ButtonState::Released), Step::Frames(1)]);
                }
            }
            "key" => {
                if let Some((code, key)) = named_key(arg) {
                    steps.extend([Step::Key(code, key.clone(), ButtonState::Pressed), Step::Frames(1), Step::Key(code, key, ButtonState::Released), Step::Frames(1)]);
                }
            }
            "shot" => steps.extend([Step::Shot(arg.to_string()), Step::Frames(SETTLE_FRAMES)]),
            "warp" => steps.extend(point(arg).map(|p| Step::Warp(p.x as usize, p.y as usize))),
            "quit" => steps.push(Step::Quit),
            _ => {}
        }
    }
    steps
}

impl Args {
    pub fn parse(mut args: impl Iterator<Item = String>) -> Self {
        let mut out = Self::default();
        while let Some(flag) = args.next() {
            let value = args.next().unwrap_or_default();
            match flag.as_str() {
                "--shot" => out.script.extend([Step::Frames(SETTLE_FRAMES), Step::Shot(value), Step::Frames(SETTLE_FRAMES), Step::Quit]),
                "--do" => out.script.extend(parse_script(&value)),
                "--screen" => {
                    out.screen = match value.as_str() {
                        "main" => Some(Screen::Main),
                        "keys" => Some(Screen::Keys),
                        "settings" => Some(Screen::Settings),
                        "newgame" => Some(Screen::NewGame),
                        "load" => Some(Screen::Load),
                        _ => None,
                    }
                }
                "--size" => out.size = value.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))),
                "--lang" => {
                    out.lang = match value.as_str() {
                        "ru" => Some(Lang::Ru),
                        "en" => Some(Lang::En),
                        _ => None,
                    }
                }
                "--keys-file" => out.keys_file = Some(value),
                "--hidden" => out.hidden = value == "true",
                "--fake-gm" => out.fake_gm = value == "true",
                _ => {}
            }
        }
        out
    }
}

fn apply_start(args: Res<StartArgs>, mut next: ResMut<NextState<Screen>>, mut settings: ResMut<Settings>, gm: Res<Gm>, mut book: ResMut<KeyBook>) {
    if let Some(screen) = args.0.screen {
        next.set(screen);
    }
    // Changed in memory only: a test run must not rewrite the saved language.
    if let Some(lang) = args.0.lang {
        settings.lang = lang;
    }
    if let Some(file) = &args.0.keys_file {
        for key in std::fs::read_to_string(file).unwrap_or_default().lines() {
            gm.add_key(&mut book, key);
        }
    }
}

#[derive(Default)]
struct Progress {
    next: usize,
    wait: f32,
    frames: u32,
}

fn key_message(window: Entity, code: KeyCode, key: Key, state: ButtonState, text: Option<&str>) -> KeyboardInput {
    KeyboardInput { key_code: code, logical_key: key, state, text: text.map(Into::into), repeat: false, window }
}

/// Plays the script one step at a time, feeding the same messages the window system would.
#[allow(clippy::too_many_arguments)]
fn play_script(
    args: Res<StartArgs>,
    time: Res<Time>,
    mut progress: Local<Progress>,
    mut commands: Commands,
    mut window: Single<(Entity, &mut Window), With<PrimaryWindow>>,
    mut window_events: MessageWriter<WindowEvent>,
    mut moves: MessageWriter<CursorMoved>,
    mut buttons: MessageWriter<MouseButtonInput>,
    mut keys: MessageWriter<KeyboardInput>,
    mut exit: MessageWriter<AppExit>,
    mut warp: ResMut<Warp>,
) {
    if progress.frames > 0 {
        progress.frames -= 1;
        return;
    }
    if progress.wait > 0.0 {
        progress.wait -= time.delta_secs();
        return;
    }
    let Some(step) = args.0.script.get(progress.next).cloned() else { return };
    progress.next += 1;
    let entity = window.0;
    let mut key = |message: KeyboardInput| {
        window_events.write(WindowEvent::KeyboardInput(message.clone()));
        keys.write(message);
    };
    match step {
        Step::Wait(seconds) => progress.wait = seconds,
        Step::Frames(n) => progress.frames = n,
        Step::Move(at) => {
            let (scale, origin, _) = fit(window.1.physical_size());
            let physical = origin.as_vec2() + at * scale as f32;
            window.1.set_physical_cursor_position(Some(physical.as_dvec2()));
            let message = CursorMoved { window: entity, position: physical / window.1.scale_factor(), delta: None };
            window_events.write(WindowEvent::CursorMoved(message.clone()));
            moves.write(message);
        }
        Step::Mouse(state) => {
            let message = MouseButtonInput { button: MouseButton::Left, state, window: entity };
            window_events.write(WindowEvent::MouseButtonInput(message));
            buttons.write(message);
        }
        Step::Type(text) => {
            for ch in text.chars() {
                let s = ch.to_string();
                let code = KeyCode::Unidentified(NativeKeyCode::Unidentified);
                key(key_message(entity, code, Key::Character(s.as_str().into()), ButtonState::Pressed, Some(&s)));
                key(key_message(entity, code, Key::Character(s.as_str().into()), ButtonState::Released, None));
            }
        }
        Step::Key(code, logical, state) => key(key_message(entity, code, logical, state, None)),
        Step::Shot(path) => {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        }
        Step::Warp(x, y) => warp.0 = Some((x, y)),
        Step::Quit => {
            exit.write(AppExit::Success);
        }
    }
}

#[derive(Resource)]
struct StartArgs(Args);

pub struct ShotPlugin(pub Args);

impl Plugin for ShotPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(StartArgs(self.0.clone())).init_resource::<Warp>().add_systems(Startup, apply_start).add_systems(First, play_script);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Args {
        Args::parse(line.split_whitespace().map(str::to_string))
    }

    #[test]
    fn flags_are_read_in_any_order() {
        let a = parse("--size 1920x1080 --lang en --screen keys --keys-file k.txt --hidden true");
        assert_eq!(a.size, Some((1920, 1080)));
        assert_eq!(a.lang, Some(Lang::En));
        assert_eq!(a.screen, Some(Screen::Keys));
        assert_eq!(a.keys_file.as_deref(), Some("k.txt"));
        assert!(a.hidden);
    }

    #[test]
    fn nonsense_values_are_ignored_rather_than_fatal() {
        let a = parse("--size big --lang de --screen nowhere --what");
        assert!(a.size.is_none() && a.lang.is_none() && a.screen.is_none() && a.script.is_empty());
    }

    #[test]
    fn shot_flag_settles_saves_and_quits() {
        let a = parse("--shot out.png");
        assert_eq!(a.script, [Step::Frames(SETTLE_FRAMES), Step::Shot("out.png".into()), Step::Frames(SETTLE_FRAMES), Step::Quit]);
    }

    #[test]
    fn click_is_move_press_release_with_pauses_between() {
        let steps = parse_script("click:10,20.5");
        assert_eq!(steps[0], Step::Move(Vec2::new(10.0, 20.5)));
        assert_eq!(steps[2], Step::Mouse(ButtonState::Pressed));
        assert_eq!(steps[4], Step::Mouse(ButtonState::Released));
        assert_eq!(steps.len(), 6);
    }

    #[test]
    fn script_keeps_text_verbatim_and_drops_what_it_cannot_read() {
        let steps = parse_script("type:AIza key: x; wait:1.5; key:Enter; jump:3; click:oops; key:Hyper; quit");
        assert_eq!(steps[0], Step::Type("AIza key: x".into()));
        assert_eq!(steps[1], Step::Wait(1.5));
        assert_eq!(steps[2], Step::Key(KeyCode::Enter, Key::Enter, ButtonState::Pressed));
        assert_eq!(steps.last(), Some(&Step::Quit));
        assert_eq!(steps.len(), 7);
    }
}
