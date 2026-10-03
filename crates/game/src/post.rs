//! The look of the whole picture: a colour grade the player picks in the settings, a glow
//! around what burns and shines, and for the old-screen look a curved glass, colour fringes
//! and scan lines. The interface is drawn after all of it and stays as it is.

use crate::i18n::T;
use crate::screen::MainCamera;
use crate::store::Settings;
use bevy::camera::Hdr;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::{Bloom, BloomPrefilter};
use bevy::post_process::effect_stack::{ChromaticAberration, LensDistortion};
use bevy::prelude::*;
use bevy::render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Grade {
    /// The colours as drawn, nothing over them.
    Clean,
    #[default]
    Warm,
    Cold,
    Noir,
    Sepia,
    Vivid,
    Dream,
    /// An old cathode-ray screen.
    Crt,
}

impl Grade {
    pub const ALL: [Grade; 8] = [Grade::Clean, Grade::Warm, Grade::Cold, Grade::Noir, Grade::Sepia, Grade::Vivid, Grade::Dream, Grade::Crt];

    pub fn label(self) -> T {
        match self {
            Grade::Clean => T::GradeClean,
            Grade::Warm => T::GradeWarm,
            Grade::Cold => T::GradeCold,
            Grade::Noir => T::GradeNoir,
            Grade::Sepia => T::GradeSepia,
            Grade::Vivid => T::GradeVivid,
            Grade::Dream => T::GradeDream,
            Grade::Crt => T::GradeCrt,
        }
    }

    pub fn next(self) -> Self {
        let at = Self::ALL.iter().position(|g| *g == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }

    /// Whether the old-screen lines are laid over the world.
    pub fn scan_lines(self) -> bool {
        self == Grade::Crt
    }

    fn grading(self) -> ColorGrading {
        let section = |saturation: f32, contrast: f32| ColorGradingSection { saturation, contrast, ..default() };
        let (global, sections) = match self {
            Grade::Clean => (ColorGradingGlobal::default(), section(1.0, 1.0)),
            Grade::Warm => (ColorGradingGlobal { temperature: 0.016, post_saturation: 1.08, exposure: 0.03, ..default() }, section(1.05, 1.06)),
            Grade::Cold => (ColorGradingGlobal { temperature: -0.022, tint: 0.0, post_saturation: 0.9, ..default() }, section(0.95, 1.08)),
            Grade::Noir => (ColorGradingGlobal { post_saturation: 0.08, exposure: -0.05, ..default() }, section(0.2, 1.3)),
            Grade::Sepia => (ColorGradingGlobal { temperature: 0.2, tint: -0.05, post_saturation: 0.55, ..default() }, section(0.35, 1.08)),
            Grade::Vivid => (ColorGradingGlobal { post_saturation: 1.18, exposure: 0.02, ..default() }, section(1.08, 1.1)),
            Grade::Dream => (ColorGradingGlobal { temperature: 0.02, tint: -0.02, post_saturation: 1.05, exposure: 0.03, ..default() }, section(1.0, 0.97)),
            Grade::Crt => (ColorGradingGlobal { post_saturation: 1.0, ..default() }, section(1.0, 1.1)),
        };
        ColorGrading::with_identical_sections(global, sections)
    }

    /// How strongly bright things glow, and from how bright on.
    fn bloom(self) -> Option<(f32, f32)> {
        match self {
            Grade::Clean => None,
            Grade::Noir => Some((0.08, 1.0)),
            Grade::Dream => Some((0.26, 0.7)),
            Grade::Crt => Some((0.22, 0.75)),
            Grade::Vivid => Some((0.2, 0.9)),
            _ => Some((0.14, 0.95)),
        }
    }
}

pub struct PostPlugin;

impl Plugin for PostPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, apply).add_systems(Update, apply.run_if(resource_changed::<Settings>));
    }
}

/// Puts the chosen look on the camera.
fn apply(mut commands: Commands, settings: Res<Settings>, cameras: Query<Entity, With<MainCamera>>, mut last: Local<Option<Grade>>) {
    if *last == Some(settings.grade) {
        return;
    }
    *last = Some(settings.grade);
    let grade = settings.grade;
    for camera in &cameras {
        let mut camera = commands.entity(camera);
        // The picture is drawn with room above white, so that what burns can glow.
        camera.insert((Hdr, grade.grading(), if grade == Grade::Clean { Tonemapping::None } else { Tonemapping::TonyMcMapface }));
        match grade.bloom() {
            Some((intensity, threshold)) => {
                camera.insert(Bloom { intensity, prefilter: BloomPrefilter { threshold, threshold_softness: 0.3 }, ..Bloom::NATURAL });
            }
            None => {
                camera.remove::<Bloom>();
            }
        }
        if grade == Grade::Crt {
            camera.insert((ChromaticAberration { intensity: 0.006, max_samples: 6, ..default() }, LensDistortion { intensity: 0.12, scale: 1.04, ..default() }));
        } else {
            camera.remove::<(ChromaticAberration, LensDistortion)>();
        }
    }
}

/// Lines across the picture, every other row of the canvas darker, for the old-screen look.
pub fn scan_lines(w: u32, h: u32) -> crate::art::Pixels {
    let mut lines = crate::art::Pixels::new(w, h, crate::theme::CLEAR);
    for y in (0..h as i32).step_by(2) {
        for x in 0..w as i32 {
            lines.set(x, y, [0, 0, 0, 46]);
        }
    }
    lines
}
