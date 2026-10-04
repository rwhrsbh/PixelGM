//! The game is drawn on a fixed 960×540 canvas: the world in pixels of that size, the
//! interface on a grid of 480×270, each of its pixels two of the canvas. The canvas is blown
//! up by a whole number where the window allows, by a half step where it does not: the
//! interface then stays exactly square, the world nearly so.

use bevy::camera::Viewport;
use bevy::prelude::*;
use bevy::ui_render::UiAntiAlias;
use bevy::window::PrimaryWindow;

pub const CANVAS: UVec2 = UVec2::new(960, 540);
/// The grid the interface is laid out on.
pub const UI: UVec2 = UVec2::new(480, 270);
/// Canvas pixels to a pixel of the interface.
pub const UI_PIXEL: f32 = 2.0;

#[derive(Component)]
pub struct MainCamera;

pub struct ScreenPlugin;

impl Plugin for ScreenPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::BLACK)).add_systems(Startup, spawn_camera).add_systems(PreUpdate, (fit_window, fit_canvas).chain());
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((Camera2d, MainCamera, Msaa::Off, UiAntiAlias::Off));
}

/// Whether the picture fills the window at any scale rather than in half steps that keep
/// the pixels even. Read by everything that turns a point on the window into the canvas.
static FILL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// How much nearer than the canvas the world is drawn, as the bits of an f32.
static ZOOM: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x3f80_0000);

pub fn set_zoom(zoom: f32) {
    ZOOM.store(zoom.max(0.1).to_bits(), std::sync::atomic::Ordering::Relaxed);
}

fn zoom() -> f32 {
    f32::from_bits(ZOOM.load(std::sync::atomic::Ordering::Relaxed))
}

pub fn set_fill(fill: bool) {
    FILL.store(fill, std::sync::atomic::Ordering::Relaxed);
}

/// Window sizes the player can pick, after "chosen by the screen".
pub const SIZES: [(u32, u32); 6] = [(960, 540), (1280, 720), (1440, 810), (1600, 900), (1920, 1080), (2560, 1440)];

/// The largest scale, in half steps, at which the canvas fits the window; and the centred
/// rectangle it covers. Filling, the scale is whatever fits exactly.
pub fn fit(window: UVec2) -> (f32, UVec2, UVec2) {
    fit_with(window, FILL.load(std::sync::atomic::Ordering::Relaxed))
}

pub fn fit_with(window: UVec2, fill: bool) -> (f32, UVec2, UVec2) {
    let fits = (window.x as f32 / CANVAS.x as f32).min(window.y as f32 / CANVAS.y as f32);
    let scale = if fill { fits.max(0.25) } else { ((fits * 2.0).floor() / 2.0).max(1.0) };
    let size = (CANVAS.as_vec2() * scale).as_uvec2().min(window);
    (scale, (window - size) / 2, size)
}

/// A new window takes the largest size that is a whole number of canvases and fits the
/// screen, so that it opens sharp. Once, at the start.
fn fit_window(mut done: Local<Option<u8>>, settings: Res<crate::store::Settings>, monitors: Query<&bevy::window::Monitor>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    set_fill(settings.fill);
    // Again whenever the player picks another size.
    if *done == Some(settings.window_size) {
        return;
    }
    // In fullscreen the window is the screen: sizing it in the same breath as it goes
    // fullscreen left it a small window in the corner. It is sized when it is a window again.
    if settings.fullscreen {
        return;
    }
    let Some(monitor) = monitors.iter().next() else { return };
    *done = Some(settings.window_size);
    if window.mode != bevy::window::WindowMode::Windowed || !window.visible {
        return;
    }
    if let Some((w, h)) = settings.window_size.checked_sub(1).and_then(|i| SIZES.get(i as usize)) {
        let (w, h) = (*w.min(&monitor.physical_width), *h.min(&monitor.physical_height));
        window.resolution.set_physical_resolution(w, h);
        return;
    }
    let room = UVec2::new(monitor.physical_width, monitor.physical_height).as_vec2() * Vec2::new(0.94, 0.88);
    let whole = (room.x / CANVAS.x as f32).min(room.y / CANVAS.y as f32).floor().max(1.0);
    let wanted = CANVAS.as_vec2() * whole;
    if window.resolution.physical_size().as_vec2() != wanted {
        window.resolution.set_physical_resolution(wanted.x as u32, wanted.y as u32);
    }
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
        let wanted = 1.0 / (scale * zoom());
        if ortho.scale != wanted {
            ortho.scale = wanted;
        }
    }
    // UI lengths are logical pixels; dividing by the window's own factor makes one pixel of
    // the interface exactly `scale × 2` physical pixels, a whole number at every scale.
    let wanted = scale * UI_PIXEL / window.scale_factor();
    if ui_scale.0 != wanted {
        ui_scale.0 = wanted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_hd_is_twice_with_no_bars() {
        assert_eq!(fit_with(UVec2::new(1920, 1080), false), (2.0, UVec2::ZERO, UVec2::new(1920, 1080)));
    }

    #[test]
    fn a_window_between_whole_scales_gets_a_half_step_centred() {
        assert_eq!(fit_with(UVec2::new(1600, 900), false), (1.5, UVec2::new(80, 45), UVec2::new(1440, 810)));
        assert_eq!(fit_with(UVec2::new(2560, 1440), false), (2.5, UVec2::new(80, 45), UVec2::new(2400, 1350)));
        // Filling, there are no bars on a screen of the canvas's shape.
        assert_eq!(fit_with(UVec2::new(1600, 900), true), (1600.0 / 960.0, UVec2::ZERO, UVec2::new(1600, 900)));
    }

    #[test]
    fn window_smaller_than_the_canvas_is_clipped_not_scaled_to_zero() {
        assert_eq!(fit_with(UVec2::new(800, 600), false), (1.0, UVec2::new(0, 30), UVec2::new(800, 540)));
    }
}
