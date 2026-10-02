mod art;
mod audio;
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
mod widgets;

use bevy::prelude::*;
use bevy::window::WindowResolution;

fn main() -> AppExit {
    let args = shot::Args::parse(std::env::args().skip(1));
    // A hidden test window is one canvas; a real one is sized to the screen once it opens.
    let (width, height) = args.size.unwrap_or(if args.hidden { (960, 540) } else { (1920, 1080) });
    let store = store::Store::at(store::home());
    let settings = store.load_settings();
    let mut app = App::new();
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
    .add_plugins((translate::TranslatePlugin, audio::AudioPlugin, fx::FxPlugin, voice::VoicePlugin, shot::ShotPlugin(args)));
    app.run()
}
