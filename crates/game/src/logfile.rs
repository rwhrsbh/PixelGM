//! Everything the game logs also goes to a file, so that when something goes wrong there is
//! something to send. The file of the sitting before is kept beside it, and a crash writes
//! its message and where it came from before the game goes down.

use crate::store;
use bevy::log::tracing::field::{Field, Visit};
use bevy::log::tracing::{Event, Subscriber};
use bevy::log::tracing_subscriber::Layer;
use bevy::log::tracing_subscriber::layer::Context;
use bevy::log::{BoxedLayer, Level};
use bevy::prelude::*;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;

pub const LOG_NAME: &str = "pixelgm.log";
pub const PREVIOUS_LOG_NAME: &str = "pixelgm.previous.log";

pub fn log_dir() -> PathBuf {
    store::home().join("logs")
}

/// A moment as `2026-10-03 21:07:45`, in UTC, from seconds since 1970.
pub fn stamp(unix: u64) -> String {
    let (days, rest) = (unix / 86_400, unix % 86_400);
    // Days to a calendar date (Howard Hinnant's civil-from-days).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}", rest / 3_600, rest % 3_600 / 60, rest % 60)
}

struct Line(String);

impl Visit for Line {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        } else {
            let _ = write!(self.0, " {}={value:?}", field.name());
        }
    }
}

struct FileLayer(Mutex<File>);

impl<S: Subscriber> Layer<S> for FileLayer {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let meta = event.metadata();
        // The engine's own chatter below a warning would drown the game's.
        if *meta.level() > Level::INFO || (*meta.level() == Level::INFO && !meta.target().starts_with("pixelgm")) {
            return;
        }
        let mut line = Line(String::new());
        event.record(&mut line);
        if let Ok(mut file) = self.0.lock() {
            let _ = writeln!(file, "{} {:5} {}: {}", stamp(store::unix_now()), meta.level().as_str(), meta.target(), line.0);
        }
    }
}

fn open() -> Option<File> {
    let dir = log_dir();
    fs::create_dir_all(&dir).ok()?;
    let path = dir.join(LOG_NAME);
    // The sitting before is kept: a crash is often noticed only on the next start.
    if path.exists() {
        let _ = fs::rename(&path, dir.join(PREVIOUS_LOG_NAME));
    }
    let mut file = File::create(&path).ok()?;
    let _ = writeln!(file, "{} START PixelGM {} on {}", stamp(store::unix_now()), env!("CARGO_PKG_VERSION"), std::env::consts::OS);
    Some(file)
}

/// The layer handed to the engine's logging: `None` when the file cannot be made, and the
/// game then logs to the terminal only.
pub fn layer(_app: &mut App) -> Option<BoxedLayer> {
    let file = open()?;
    // A crash goes into the same file, with where it happened.
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Ok(mut file) = OpenOptions::new().append(true).open(log_dir().join(LOG_NAME)) {
            let _ = writeln!(file, "{} PANIC {info}\n{}", stamp(store::unix_now()), std::backtrace::Backtrace::force_capture());
        }
        before(info);
    }));
    Some(Box::new(FileLayer(Mutex::new(file))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moments_read_as_dates() {
        assert_eq!(stamp(0), "1970-01-01 00:00:00");
        assert_eq!(stamp(951_782_400), "2000-02-29 00:00:00");
        assert_eq!(stamp(1_700_000_000), "2023-11-14 22:13:20");
    }
}
