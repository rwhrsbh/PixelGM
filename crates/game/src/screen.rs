//! The game is drawn on a fixed 480×270 canvas and blown up by a whole number, so every
//! pixel of art and text stays square whatever the window size.

use bevy::camera::Viewport;
use bevy::prelude::*;
use bevy::ui_render::UiAntiAlias;
use bevy::window::PrimaryWindow;

pub const CANVAS: UVec2 = UVec2::new(480, 270);

#[derive(Component)]
pub struct MainCamera;

pub struct ScreenPlugin;

impl Plugin for ScreenPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::BLACK)).add_systems(Startup, spawn_camera).add_systems(PreUpdate, fit_canvas);
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((Camera2d, MainCamera, Msaa::Off, UiAntiAlias::Off));
}

/// Largest whole-number scale that fits the window, and the centred rectangle it covers.
pub fn fit(window: UVec2) -> (u32, UVec2, UVec2) {
    let scale = (window.x / CANVAS.x).min(window.y / CANVAS.y).max(1);
    let size = (CANVAS * scale).min(window);
    (scale, (window - size) / 2, size)
}

fn fit_canvas(
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&mut Camera, &mut Projection), With<MainCamera>>,
    mut ui_scale: ResMut<UiScale>,
) {
    let physical = window.physical_size();
    if physical.x == 0 || physical.y == 0 {
        return;
    }
    let (scale, position, size) = fit(physical);
    let (mut camera, mut projection) = camera.into_inner();
    let viewport = camera.viewport.as_ref().map(|v| (v.physical_position, v.physical_size));
    if viewport != Some((position, size)) {
        camera.viewport = Some(Viewport { physical_position: position, physical_size: size, ..default() });
    }
    if let Projection::Orthographic(ortho) = projection.as_mut() {
        let wanted = 1.0 / scale as f32;
        if ortho.scale != wanted {
            ortho.scale = wanted;
        }
    }
    // UI lengths are logical pixels; dividing by the window's own factor makes one canvas
    // pixel exactly `scale` physical pixels.
    let wanted = scale as f32 / window.scale_factor();
    if ui_scale.0 != wanted {
        ui_scale.0 = wanted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_hd_is_four_times_with_no_bars() {
        assert_eq!(fit(UVec2::new(1920, 1080)), (4, UVec2::ZERO, UVec2::new(1920, 1080)));
    }

    #[test]
    fn odd_window_gets_the_largest_whole_scale_centred() {
        assert_eq!(fit(UVec2::new(1600, 900)), (3, UVec2::new(80, 45), UVec2::new(1440, 810)));
    }

    #[test]
    fn window_smaller_than_the_canvas_is_clipped_not_scaled_to_zero() {
        assert_eq!(fit(UVec2::new(400, 300)), (1, UVec2::new(0, 15), UVec2::new(400, 270)));
    }
}
