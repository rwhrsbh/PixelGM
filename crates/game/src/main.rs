mod art;
mod gm;
mod i18n;
mod keys;
mod menu;
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
    .add_plugins((
        screen::ScreenPlugin,
        i18n::I18nPlugin,
        widgets::WidgetsPlugin,
        gm::GmPlugin,
        menu::MenuPlugin,
        keys::KeysPlugin,
        settings::SettingsPlugin,
        shot::ShotPlugin(args),
    ));
    app.run()
}
