//! Where the game looks: the part of the screen the map is seen through, and the camera that
//! keeps the hero in the middle of it. The frame of the interface — the party on the right,
//! the telling below — lies around that part and never over it.

use crate::fx::Fx;
use crate::menu::Screen;
use crate::play::{Game, HUD_HEIGHT, Phase, on_screen};
use crate::screen::{CANVAS, MainCamera, UI_PIXEL, fit, set_zoom};
use crate::store::{Settings, Store};
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use world::TILE;

/// The width of the column of party portraits, in pixels of the interface.
pub const COLUMN: f32 = 46.0;
/// The heights the window of the telling can take.
pub const HEIGHTS: [f32; 3] = [80.0, 130.0, 190.0];
/// How much larger than the canvas the map is drawn; the first is "the whole map".
pub const ZOOMS: [f32; 4] = [0.0, 1.0, 1.5, 2.0];

/// What the camera shows.
#[derive(Resource)]
pub struct View {
    /// The point of the world in the middle of the screen.
    pub center: Vec2,
    pub zoom: f32,
    /// How much of the bottom of the screen the frame takes, in pixels of the interface,
    /// the strip of keys included.
    pub bottom: f32,
    /// The whole place is being looked over: M switches it.
    pub overview: bool,
}

impl Default for View {
    fn default() -> Self {
        Self { center: Vec2::ZERO, zoom: 1.0, bottom: HUD_HEIGHT, overview: false }
    }
}

impl View {
    /// The size of the part of the canvas the map is seen through.
    pub fn region(&self) -> Vec2 {
        Vec2::new(CANVAS.x as f32 - COLUMN * UI_PIXEL, CANVAS.y as f32 - self.bottom * UI_PIXEL)
    }

    /// The point of the map, in canvas pixels from the map's top-left corner, under a point
    /// of the canvas; nothing when that point lies on the frame.
    pub fn map_point(&self, canvas: Vec2) -> Option<Vec2> {
        let region = self.region();
        if canvas.x < 0.0 || canvas.y < 0.0 || canvas.x >= region.x || canvas.y >= region.y {
            return None;
        }
        let half = CANVAS.as_vec2() / 2.0;
        let world = self.center + Vec2::new(canvas.x - half.x, half.y - canvas.y) / self.zoom;
        Some(Vec2::new(world.x + half.x, half.y - world.y))
    }
}

/// The point of the map under the mouse, in canvas pixels from the map's top-left corner.
pub fn map_under_cursor(window: &Window, view: &View) -> Option<Vec2> {
    let physical = window.physical_cursor_position()?;
    let (scale, origin, _) = fit(window.physical_size());
    view.map_point((physical - origin.as_vec2()) / scale)
}

/// A thing drawn over the whole screen rather than on the map: it stays put while the
/// camera moves. Its place is given as on a canvas the camera does not move over.
#[derive(Component)]
pub struct ScreenFixed(pub Vec2);

/// A sheet lying in the slot under the map; the frame keeps as much of the screen as it is tall.
#[derive(Component)]
pub struct Slot;

fn measure(mut view: ResMut<View>, slots: Query<&ComputedNode, With<Slot>>, phase: Res<Phase>) {
    let tallest = slots.iter().map(|node| node.size().y * node.inverse_scale_factor()).fold(0.0f32, f32::max);
    let _ = &phase;
    let bottom = HUD_HEIGHT + tallest.round();
    if view.bottom != bottom {
        view.bottom = bottom;
    }
}

/// The hero, or in a fight the hero's fighter, as a point of the world.
fn focus(game: &Game, mine: Option<usize>) -> Vec2 {
    let world = &game.world;
    let fighter = world.fight.as_ref().filter(|_| world.fight_here()).and_then(|fight| {
        let id = format!("hero_{}", mine.unwrap_or(0));
        fight.combatants.iter().find(|c| c.id == id && !c.dead)
    });
    match fighter {
        Some(c) => {
            let tile = TILE as f32;
            on_screen((c.x as f32 + 0.5) * tile, (c.y as f32 + 0.5) * tile)
        }
        None => on_screen(world.player.x, world.player.y),
    }
}

fn follow(time: Res<Time>, game: Res<Game>, settings: Res<Settings>, session: Res<crate::coop::Session>, mut view: ResMut<View>, mut was: Local<Option<String>>) {
    let px = crate::scene::PX as f32;
    let (map_w, map_h) = game.world.here().size();
    let map = Vec2::new((map_w as u32 * TILE) as f32 * px, (map_h as u32 * TILE) as f32 * px);
    let region = view.region();
    let zoom = match if view.overview { 0.0 } else { ZOOMS.get(settings.zoom as usize).copied().unwrap_or(1.5) } {
        z if z > 0.0 => z,
        _ => (region.x / map.x).min(region.y / map.y),
    };
    let half = CANVAS.as_vec2() / 2.0;
    // Where the middle of the seen part lies from the middle of the screen.
    let shift = Vec2::new(region.x / 2.0 - half.x, half.y - region.y / 2.0);
    let seen = region / (2.0 * zoom);
    let (left, top) = (-half.x, half.y);
    let fit = |want: f32, low: f32, high: f32, reach: f32| if high - low <= reach * 2.0 { (low + high) / 2.0 } else { want.clamp(low + reach, high - reach) };
    let me = crate::coop::player_name(&settings);
    let hero = focus(&game, crate::play::my_hero(&game.world, &me, matches!(*session, crate::coop::Session::Solo)));
    let middle = Vec2::new(fit(hero.x, left, left + map.x, seen.x), fit(hero.y, top - map.y, top, seen.y));
    let target = middle - shift / zoom;
    // A new place, or a new zoom, is looked at straight away; a walk is followed smoothly.
    let place = Some(game.world.current.clone());
    let jump = *was != place || view.zoom != zoom || view.center.distance(target) > 240.0;
    *was = place;
    let next = if jump { target } else { view.center.lerp(target, 1.0 - (-time.delta_secs() * 9.0).exp()) };
    // On whole pixels of the picture, so that nothing shimmers.
    let grain = 1.0 / zoom.max(0.25);
    let next = (next / grain).round() * grain;
    if view.center != next || view.zoom != zoom {
        view.center = next;
        view.zoom = zoom;
    }
    set_zoom(zoom);
}

fn rest(mut view: ResMut<View>) {
    *view = View::default();
    set_zoom(1.0);
}

/// The camera stands where the view says, shaken when the ground is.
fn place_camera(view: Res<View>, fx: Res<Fx>, mut camera: Single<&mut Transform, With<MainCamera>>, mut fixed: Query<(&ScreenFixed, &mut Transform), Without<MainCamera>>) {
    let at = view.center + fx.offset / view.zoom;
    if camera.translation.truncate() != at {
        camera.translation.x = at.x;
        camera.translation.y = at.y;
    }
    for (home, mut transform) in &mut fixed {
        let wanted = at + home.0 / view.zoom;
        let scale = Vec3::splat(1.0 / view.zoom);
        if transform.translation.truncate() != wanted || transform.scale != scale {
            transform.translation.x = wanted.x;
            transform.translation.y = wanted.y;
            transform.scale = scale;
        }
    }
}

/// M looks over the whole place and back.
fn overview(keyboard: Res<ButtonInput<KeyCode>>, phase: Res<Phase>, mut view: ResMut<View>) {
    if keyboard.just_pressed(KeyCode::KeyM) && matches!(*phase, Phase::Walking | Phase::Fighting | Phase::Reading { .. }) {
        view.overview = !view.overview;
    }
}

/// The wheel over the map brings it nearer or takes it further.
fn wheel(mut turns: MessageReader<MouseWheel>, window: Single<&Window, With<PrimaryWindow>>, view: Res<View>, phase: Res<Phase>, mut settings: ResMut<Settings>, store: Res<Store>) {
    let turned: f32 = turns.read().map(|t| t.y).sum();
    if turned == 0.0 || !matches!(*phase, Phase::Walking | Phase::Reading { .. } | Phase::Fighting | Phase::Asking { .. } | Phase::Looking { .. }) || map_under_cursor(&window, &view).is_none() {
        return;
    }
    let now = settings.zoom as i32;
    let next = (now + if turned > 0.0 { 1 } else { -1 }).clamp(0, ZOOMS.len() as i32 - 1);
    if next != now {
        settings.zoom = next as u8;
        if let Err(e) = store.save_settings(&settings) {
            error!("could not save settings: {e}");
        }
    }
}

pub struct ViewPlugin;

impl Plugin for ViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<View>()
            .add_systems(Update, (overview, wheel, measure, follow).chain().run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>))
            .add_systems(PostUpdate, place_camera)
            .add_systems(OnExit(Screen::Play), rest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_of_the_screen_is_a_point_of_the_map_and_the_frame_is_not() {
        let view = View { center: Vec2::new(100.0, -50.0), zoom: 2.0, bottom: 60.0, overview: false };
        // The middle of the screen shows the point the camera stands on.
        let middle = view.map_point(Vec2::new(480.0, 270.0)).unwrap();
        assert_eq!(middle, Vec2::new(580.0, 320.0));
        // Twice as near, a step on the screen is half a step on the map.
        assert_eq!(view.map_point(Vec2::new(500.0, 270.0)).unwrap().x, 590.0);
        assert_eq!(view.map_point(Vec2::new(900.0, 100.0)), None, "under the party column");
        assert_eq!(view.map_point(Vec2::new(100.0, 500.0)), None, "under the telling");
    }
}
