//! The graphics screen: every effect over the picture can be switched on and off, and a
//! small scene beside the list shows at once what each choice looks like — a camp with a
//! fire and water, a hero walking on the spot, at the hour and in the weather picked for it.

use crate::art::Art;
use crate::i18n::{Lang, T};
use crate::menu::{Backdrop, Nav, Screen};
use crate::post::Grade;
use crate::store::{Settings, Store};
use crate::theme::*;
use crate::widgets::{Activate, button, panel_backing, text_button};
use bevy::prelude::*;
use bevy::sprite::Anchor;

/// The preview's own choices: they change nothing in the game.
#[derive(Resource, Default)]
struct Preview {
    /// Day, dusk, night or dark.
    hour: usize,
    weather: usize,
    built: Option<(Grade, crate::post::Gfx, usize, usize, Lang)>,
    scene: Option<Painted>,
}

/// The preview's place, painted once.
struct Painted {
    ground: Vec<Handle<Image>>,
    standing: Vec<(Handle<Image>, (f32, f32))>,
    fire: (Handle<Image>, (f32, f32)),
    cat: (Handle<Image>, (f32, f32)),
    hero: Vec<Handle<Image>>,
    warm: Handle<Image>,
    vignette: Handle<Image>,
    lines: Handle<Image>,
    grid: Handle<Image>,
    dark: [Handle<Image>; 4],
    /// The pond, on the canvas.
    water: Vec<Rect>,
}

const HOURS: [(&str, &str, &str); 4] = [("day", "день", "day"), ("dusk", "сумерки", "dusk"), ("night", "ночь", "night"), ("dark", "подземелье", "underground")];

/// The part of the place shown, in the place's canvas pixels, and where on the canvas it goes.
const CROP: Rect = Rect { min: Vec2::new(96.0, 128.0), max: Vec2::new(576.0, 512.0) };
const AT: Vec2 = Vec2::new(12.0, 12.0);

#[derive(Component)]
struct PreviewThing;

#[derive(Component)]
struct GfxPanel;

#[derive(Component)]
struct WalkingHero(Vec<Handle<Image>>);

#[derive(Component, Clone, Copy)]
enum Pick {
    Grade,
    Bloom,
    Vignette,
    ScanLines,
    Glass,
    Weather,
    Night,
    CellHint,
    Hour,
    Sky,
}

pub struct GraphicsPlugin;

impl Plugin for GraphicsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Preview>()
            .add_systems(OnEnter(Screen::Graphics), enter)
            .add_systems(OnExit(Screen::Graphics), leave)
            .add_systems(Update, (build, walk, escape).run_if(in_state(Screen::Graphics)))
            .add_observer(on_activate);
    }
}

fn enter(mut backdrop: Query<&mut Visibility, With<Backdrop>>, mut preview: ResMut<Preview>) {
    // The table under the menus would hide the scene.
    for mut v in &mut backdrop {
        *v = Visibility::Hidden;
    }
    preview.built = None;
}

fn leave(mut commands: Commands, mut backdrop: Query<&mut Visibility, With<Backdrop>>, things: Query<Entity, Or<(With<PreviewThing>, With<GfxPanel>)>>) {
    for mut v in &mut backdrop {
        *v = Visibility::Inherited;
    }
    for thing in &things {
        commands.entity(thing).despawn();
    }
}

fn escape(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<Screen>>) {
    if keys.just_pressed(KeyCode::Escape) {
        next.set(Screen::Settings);
    }
}

fn paint(images: &mut Assets<Image>) -> Painted {
    let draft: world::WorldDraft = serde_json::from_str(&world::fake::world("preview")).expect("the test world reads");
    let (world, _) = world::build_world(&draft, "preview", "English").expect("the test world builds");
    let here = world.here();
    let scene = crate::scene::paint(here, &world);
    let tile = world::TILE as f32;
    let foot = |x: usize, y: usize| ((x as f32 + 0.5) * tile, (y as f32 + 1.0) * tile);
    let sprite = |id: &str, images: &mut Assets<Image>| images.add(crate::scene::sprite(&world.sprites[id], &world.palette).into_image());
    let fire = here.props.first().map(|p| foot(p.x, p.y)).unwrap_or((160.0, 208.0));
    let cat = here.creatures.first().map(|c| foot(c.x, c.y)).unwrap_or((200.0, 192.0));
    let data = rules::srd();
    let fighter = data.class("fighter").expect("fighters exist");
    let skills: Vec<String> = fighter.skill_options.iter().take(fighter.skill_picks).cloned().collect();
    let mut hero = rules::Character::create("Preview", "", "human", "fighter", [15, 14, 13, 12, 10, 8], &skills, &[]).expect("a fighter can be made");
    hero.look = rules::Look { build: 1, skin: 2, hair: 3, hair_color: 4, beard: 0, eyes: 1, cloth: 0, trim: 3, helmet: false };
    let lamp = crate::scene::Lamp { x: fire.0, y: fire.1 - tile / 2.0, reach: 4.2 };
    let hero_at = crate::scene::Lamp { x: fire.0 - 2.0 * tile, y: fire.1 - tile / 2.0, reach: 2.4 };
    let dark = std::array::from_fn(|n| {
        let mut place = here.clone();
        place.light = HOURS[n].0.to_string();
        images.add(crate::scene::darkness(crate::scene::Hour::of(&place), &[lamp, hero_at], 0).into_image())
    });
    let (w, h) = (CROP.width() as i32, CROP.height() as i32);
    let cell = (world::TILE * crate::scene::PX as u32) as f32;
    let water = scene.water.iter().map(|(x, y)| Rect::new(*x as f32 * cell, *y as f32 * cell, (*x + 1) as f32 * cell, (*y + 1) as f32 * cell)).map(|r| Rect { min: r.min - CROP.min + AT, max: r.max - CROP.min + AT }).collect();
    Painted {
        water,
        ground: scene.ground.into_iter().map(|g| images.add(g.into_image())).collect(),
        standing: scene.standing.into_iter().map(|s| (images.add(s.picture.into_image()), s.foot)).collect(),
        fire: (sprite("campfire", images), fire),
        cat: (sprite("cat", images), cat),
        hero: crate::doll::frames(&hero).into_iter().map(|f| images.add(f.into_image())).collect(),
        warm: images.add(crate::scene::warm_pool(120).into_image()),
        vignette: images.add(crate::scene::vignette(w, h).into_image()),
        lines: images.add(crate::post::scan_lines(w as u32, h as u32).into_image()),
        grid: images.add(crate::scene::walk_grid(here, &world).into_image()),
        dark,
    }
}

/// From the place's own pixels to the canvas, with the crop in the top left corner.
fn place(x: f32, y: f32, z: f32) -> Vec3 {
    let canvas = AT + Vec2::new(x * crate::scene::PX as f32, y * crate::scene::PX as f32) - CROP.min;
    Vec3::new(canvas.x - crate::screen::CANVAS.x as f32 / 2.0, crate::screen::CANVAS.y as f32 / 2.0 - canvas.y, z)
}

fn corner(z: f32) -> Vec3 {
    Vec3::new(AT.x - crate::screen::CANVAS.x as f32 / 2.0, crate::screen::CANVAS.y as f32 / 2.0 - AT.y, z)
}

#[allow(clippy::too_many_arguments)]
fn build(
    mut commands: Commands,
    settings: Res<Settings>,
    lang: Res<Lang>,
    art: Res<Art>,
    fonts: Res<Fonts>,
    mut preview: ResMut<Preview>,
    mut images: ResMut<Assets<Image>>,
    old: Query<Entity, Or<(With<PreviewThing>, With<GfxPanel>)>>,
) {
    let key = (settings.grade, settings.gfx, preview.hour, preview.weather, *lang);
    if preview.built == Some(key) {
        return;
    }
    preview.built = Some(key);
    if preview.scene.is_none() {
        preview.scene = Some(paint(&mut images));
    }
    for thing in &old {
        commands.entity(thing).despawn();
    }
    let gfx = settings.gfx;
    let ru = *lang == Lang::Ru;
    let p = preview.scene.as_ref().expect("painted above");
    // The scene: a crop of the place, and what stands in it.
    commands.spawn((PreviewThing, Sprite { rect: Some(CROP), ..Sprite::from_image(p.ground[0].clone()) }, Anchor::TOP_LEFT, Transform::from_translation(corner(0.0))));
    let inside = |(x, y): (f32, f32)| {
        let c = Vec2::new(x, y) * crate::scene::PX as f32;
        c.x > CROP.min.x + 8.0 && c.x < CROP.max.x - 8.0 && c.y > CROP.min.y + 24.0 && c.y < CROP.max.y
    };
    for (image, at) in p.standing.iter().filter(|(_, at)| inside(*at)) {
        commands.spawn((PreviewThing, Sprite::from_image(image.clone()), Anchor::BOTTOM_CENTER, Transform::from_translation(place(at.0, at.1, 1.0 + at.1 * 0.001))));
    }
    let glow = Color::linear_rgb(1.9, 1.6, 1.25);
    commands.spawn((PreviewThing, Sprite { color: glow, ..Sprite::from_image(p.fire.0.clone()) }, Anchor::BOTTOM_CENTER, Transform::from_translation(place(p.fire.1.0, p.fire.1.1, 1.2))));
    commands.spawn((PreviewThing, Sprite::from_image(p.cat.0.clone()), Anchor::BOTTOM_CENTER, Transform::from_translation(place(p.cat.1.0, p.cat.1.1, 1.2))));
    let tile = world::TILE as f32;
    commands.spawn((PreviewThing, WalkingHero(p.hero.clone()), Sprite::from_image(p.hero[0].clone()), Anchor::BOTTOM_CENTER, Transform::from_translation(place(p.fire.1.0 - 2.0 * tile, p.fire.1.1, 1.2))));
    // The dark, the lamp's warmth, the walk grid, the vignette and the lines, as in the game.
    let size = CROP.size();
    if preview.hour > 0 && gfx.night {
        let block = crate::scene::DARK_BLOCK as f32;
        let rect = Rect { min: CROP.min / block, max: CROP.max / block };
        commands.spawn((PreviewThing, Sprite { rect: Some(rect), custom_size: Some(size), ..Sprite::from_image(p.dark[preview.hour].clone()) }, Anchor::TOP_LEFT, Transform::from_translation(corner(1.6))));
        commands.spawn((PreviewThing, Sprite { color: Color::linear_rgb(1.6, 1.4, 1.1), ..Sprite::from_image(p.warm.clone()) }, Transform::from_translation(place(p.fire.1.0, p.fire.1.1 - tile / 2.0, 1.62))));
    }
    if gfx.cell_hint {
        commands.spawn((PreviewThing, Sprite { rect: Some(CROP), color: Color::srgba(1.0, 1.0, 1.0, 0.5), ..Sprite::from_image(p.grid.clone()) }, Anchor::TOP_LEFT, Transform::from_translation(corner(1.65))));
    }
    if gfx.vignette {
        commands.spawn((PreviewThing, Sprite::from_image(p.vignette.clone()), Anchor::TOP_LEFT, Transform::from_translation(corner(1.7))));
    }
    if gfx.scan_lines {
        commands.spawn((PreviewThing, Sprite::from_image(p.lines.clone()), Anchor::TOP_LEFT, Transform::from_translation(corner(1.72))));
    }
    crate::weather::preview(&mut commands, preview.weather, gfx.weather, Rect { min: AT, max: AT + size }, p.water.clone(), PreviewThing);

    // The list of choices.
    let on = |b: bool| if b { "[x]" } else { "[ ]" }.to_string();
    let hour = |n: usize| if ru { HOURS[n].1 } else { HOURS[n].2 }.to_string();
    let rows: Vec<(T, String, Pick)> = vec![
        (T::SetGrade, settings.grade.label().text(*lang).to_string(), Pick::Grade),
        (T::GfxBloom, on(gfx.bloom), Pick::Bloom),
        (T::GfxVignette, on(gfx.vignette), Pick::Vignette),
        (T::GfxScanLines, on(gfx.scan_lines), Pick::ScanLines),
        (T::GfxGlass, on(gfx.glass), Pick::Glass),
        (T::GfxWeather, on(gfx.weather), Pick::Weather),
        (T::GfxNight, on(gfx.night), Pick::Night),
        (T::GfxCellHint, on(gfx.cell_hint), Pick::CellHint),
        (T::GfxPreviewHour, hour(preview.hour), Pick::Hour),
        (T::GfxPreviewSky, crate::weather::Sky::ALL[preview.weather % crate::weather::Sky::ALL.len()].label().text(*lang).to_string(), Pick::Sky),
    ];
    commands.spawn((GfxPanel, Node { position_type: PositionType::Absolute, left: px(252), top: px(6), width: px(222), padding: UiRect::all(px(10)), flex_direction: FlexDirection::Column, row_gap: px(3), ..default() })).with_children(|panel| {
        panel.spawn(panel_backing(&art));
        panel.spawn((Text::new(T::Graphics.text(*lang)), fonts.title(1), TextColor(color(INK))));
        for (n, (label, value, pick)) in rows.into_iter().enumerate() {
            // The last two rows choose only what the preview shows.
            if n == 8 {
                panel.spawn((Text::new(T::GfxPreviewOnly.text(*lang)), fonts.body(), TextColor(color(FADED))));
            }
            panel.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..default() }).with_children(|line| {
                line.spawn((Text::new(label.text(*lang)), fonts.body(), TextColor(color(INK)), TextLayout::no_wrap()));
                text_button(line, &art, &fonts, value, pick);
            });
        }
        panel.spawn(Node { margin: UiRect::top(px(4)), ..default() }).with_children(|slot| button(slot, &art, &fonts, T::Back, true, Nav::Go(Screen::Settings)));
    });
    commands.spawn((GfxPanel, Node { position_type: PositionType::Absolute, left: px(8), top: px(208), width: px(236), ..default() })).with_children(|n| {
        n.spawn((Text::new(T::GradeHint.text(*lang)), fonts.body(), TextColor(color(PARCHMENT))));
    });
}

/// The hero in the preview walks on the spot.
fn walk(time: Res<Time>, mut heroes: Query<(&WalkingHero, &mut Sprite)>, mut clock: Local<f32>) {
    *clock += time.delta_secs();
    let step = (*clock * 8.0) as usize % crate::doll::STEPS;
    for (frames, mut sprite) in &mut heroes {
        let frame = &frames.0[crate::doll::frame_index(crate::doll::Facing::Right, step)];
        if sprite.image != *frame {
            sprite.image = frame.clone();
        }
    }
}

fn on_activate(used: On<Activate>, picks: Query<&Pick>, mut settings: ResMut<Settings>, mut preview: ResMut<Preview>, store: Res<Store>) {
    let Ok(pick) = picks.get(used.entity) else { return };
    let g = &mut settings.gfx;
    match pick {
        Pick::Grade => settings.grade = settings.grade.next(),
        Pick::Bloom => g.bloom = !g.bloom,
        Pick::Vignette => g.vignette = !g.vignette,
        Pick::ScanLines => g.scan_lines = !g.scan_lines,
        Pick::Glass => g.glass = !g.glass,
        Pick::Weather => g.weather = !g.weather,
        Pick::Night => g.night = !g.night,
        Pick::CellHint => g.cell_hint = !g.cell_hint,
        Pick::Hour => {
            preview.hour = (preview.hour + 1) % HOURS.len();
            return;
        }
        Pick::Sky => {
            preview.weather = (preview.weather + 1) % crate::weather::Sky::ALL.len();
            return;
        }
    }
    if let Err(e) = store.save_settings(&settings) {
        error!("could not save settings: {e}");
    }
}
