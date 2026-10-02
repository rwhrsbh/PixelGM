mod art;
mod coop;
mod fight;
mod gm;
mod hero;
mod i18n;
mod keys;
mod master;
mod menu;
mod nav;
mod newgame;
mod play;
mod saves;
mod screen;
mod secret;
mod settings;
mod shot;
mod store;
mod theme;
mod widgets;

use bevy::prelude::*;
use bevy::window::WindowResolution;

fn main() -> AppExit {
    let args = shot::Args::parse(std::env::args().skip(1));
    let (width, height) = args.size.unwrap_or((1440, 810));
    let store = store::Store::at(store::home());
    let settings = store.load_settings();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(ImagePlugin::default_nearest())
            .set(WindowPlugin { primary_window: Some(Window { title: "PixelGM".into(), resolution: WindowResolution::new(width, height), visible: !args.hidden, ..default() }), ..default() }),
    )
    .insert_resource(store)
    .insert_resource(settings)
    .init_resource::<theme::Fonts>()
    .init_resource::<art::Art>()
    .insert_resource(master::Master::new(args.fake_gm))
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
        shot::ShotPlugin(args),
    ));
    app.run()
}
