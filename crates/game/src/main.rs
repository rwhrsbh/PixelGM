mod art;
mod audio;
mod bag_ui;
mod icons;
mod coop;
mod fight;
mod fx;
mod gm;
mod hero;
mod i18n;
mod keys;
mod logfile;
mod master;
mod menu;
mod nav;
mod newgame;
mod play;
mod post;
mod motion;
mod music;
mod provider;
mod party_hud;
mod graphics;
mod weather;
mod saves;
mod scene;
mod doll;
mod screen;
mod secret;
mod settings;
mod shot;
mod store;
mod theme;
mod translate;
mod voice;
mod shop_ui;
mod view;
mod widgets;

use bevy::prelude::*;
use bevy::window::WindowResolution;

fn main() -> AppExit {
    let args = shot::Args::parse(std::env::args().skip(1));
    // Before any window exists: Steam's overlay is brought in even when the game was started
    // by hand. A test instance kept off the desktop has no use for it.
    let overlay = (!args.hidden && std::env::var_os("PIXELGM_HOME").is_none()).then(net::bring_overlay);
    // A hidden test window is one canvas; a real one is sized to the screen once it opens.
    let (width, height) = args.size.unwrap_or(if args.hidden { (960, 540) } else { (1920, 1080) });
    let store = store::Store::at(store::home());
    let settings = store.load_settings();
    let mut app = App::new();
    // A command meant for something already gone (a figure redrawn the same frame it was
    // told to move) is written down, not a crash.
    app.set_error_handler(bevy::ecs::error::warn);
    app.add_plugins(
        DefaultPlugins
            .set(ImagePlugin::default_nearest())
            .set(bevy::log::LogPlugin { custom_layer: logfile::layer, ..default() })
            .set(WindowPlugin { primary_window: Some(Window { title: "PixelGM".into(), resolution: WindowResolution::new(width, height), visible: !args.hidden, ..default() }), ..default() }),
    )
    .insert_resource(store)
    .insert_resource(settings)
    .init_resource::<theme::Fonts>()
    .init_resource::<art::Art>()
    .insert_resource(master::Master::new(args.fake_gm))
    // An instance with a hidden window is a test: it makes no noise.
    .insert_resource(audio::Muted(args.hidden && !args.sound))
    .add_plugins((
        screen::ScreenPlugin,
        i18n::I18nPlugin,
        widgets::WidgetsPlugin,
        nav::NavPlugin,
        gm::GmPlugin,
        menu::MenuPlugin,
        keys::KeysPlugin,
        settings::SettingsPlugin,
        newgame::NewGamePlugin,
        saves::SavesPlugin,
        coop::CoopPlugin,
        play::PlayPlugin,
        hero::HeroPlugin,
        fight::FightPlugin,
    ))
    .add_plugins((view::ViewPlugin, shop_ui::ShopPlugin, translate::TranslatePlugin, bag_ui::BagPlugin, post::PostPlugin, motion::MotionPlugin, music::MusicPlugin, provider::ProviderPlugin, party_hud::PartyHudPlugin, graphics::GraphicsPlugin, weather::WeatherPlugin, audio::AudioPlugin, fx::FxPlugin, voice::VoicePlugin, shot::ShotPlugin(args)));
    // How bringing the overlay in went is said once there is a log to say it in.
    if let Some(overlay) = overlay {
        app.add_systems(Startup, move || match &overlay {
            Ok(how) => info!("Steam overlay: {how}"),
            Err(why) => info!("Steam overlay not brought in: {why}"),
        });
    }
    app.run()
}
