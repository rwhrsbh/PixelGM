//! A shop, as in Diablo 2: the keeper's wares laid out on a grid, each thing its own picture
//! at its own size, sorted onto tabs; one's own bag on the right. A click on a ware buys it;
//! a right click on a thing of one's own sells it. What the cursor is over is told below.

use crate::art::Art;
use crate::audio::{Play, Sfx};
use crate::coop::{Intent, Session, player_name};
use crate::i18n::{Lang, T};
use crate::icons::CELL;
use crate::menu::Screen;
use crate::play::{Game, Phase, my_hero};
use crate::store::Settings;
use crate::theme::*;
use crate::widgets::{Activate, PixelButton, button, panel_backing, text_button};
use bevy::prelude::*;
use world::bag::{BAG_H, BAG_W, Gear, gear_kind};
use world::shop::ShopTrouble;
use world::story::Bag;

/// The keeper being traded with, while the bag is open for it.
#[derive(Resource, Default)]
pub struct Shopping(pub Option<String>);

/// What the last deal came to, for the line under the wares.
#[derive(Resource, Default)]
pub struct Deal(pub String);

/// The shelf being looked at.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shelf {
    #[default]
    Weapons,
    Armour,
    Supplies,
    Other,
}

impl Shelf {
    const ALL: [Shelf; 4] = [Shelf::Weapons, Shelf::Armour, Shelf::Supplies, Shelf::Other];

    fn label(self) -> T {
        match self {
            Shelf::Weapons => T::ShelfWeapons,
            Shelf::Armour => T::ShelfArmour,
            Shelf::Supplies => T::ShelfSupplies,
            Shelf::Other => T::ShelfOther,
        }
    }

    /// The shelf a thing lies on.
    pub fn of(item: &world::Item) -> Shelf {
        match item.gear.as_deref().and_then(gear_kind) {
            Some(Gear::Weapon) => Shelf::Weapons,
            Some(Gear::Armor | Gear::Shield) => Shelf::Armour,
            None if matches!(item.kind.as_str(), "potion" | "food" | "light" | "throwable") || !item.heal.trim().is_empty() || !item.damage.trim().is_empty() || item.light > 0 => Shelf::Supplies,
            None => Shelf::Other,
        }
    }
}

#[derive(Component)]
struct ShopPanel;

/// The words under the wares: what the cursor is over.
#[derive(Component)]
struct WareInfo;

#[derive(Component, Clone, Copy)]
enum ShopAct {
    Buy(usize),
    Shelf(Shelf),
    Haggle,
    Leave,
}

pub fn trouble_words(trouble: ShopTrouble, lang: Lang) -> String {
    match trouble {
        ShopTrouble::Closed => T::ShopClosed,
        ShopTrouble::NoGold => T::NoGold,
        ShopTrouble::NoRoom => T::NoRoom,
        ShopTrouble::Nothing | ShopTrouble::Unwanted => T::ShopUnwanted,
    }
    .text(lang)
    .to_string()
}

/// Where the wares of a shelf lie on its grid: each thing takes the first place it fits, as
/// in a bag. What finds no place is not shown; the place in the keeper's list comes along.
fn laid_out(wares: &[(&world::Item, u32)], shelf: Shelf) -> Vec<(usize, (u8, u8), (u8, u8))> {
    let on_shelf: Vec<usize> = (0..wares.len()).filter(|n| Shelf::of(wares[*n].0) == shelf).collect();
    let mut grid = Bag { inventory: on_shelf.iter().map(|n| world::Item { at: None, ..wares[*n].0.clone() }).collect(), ..Default::default() };
    grid.pack();
    on_shelf.iter().zip(&grid.inventory).filter_map(|(n, item)| item.at.map(|at| (*n, at, item.size()))).collect()
}

const GRID_LEFT: f32 = 22.0;
const GRID_TOP: f32 = 46.0;

#[allow(clippy::too_many_arguments)]
fn build(
    mut commands: Commands,
    phase: Res<Phase>,
    game: Res<Game>,
    mut shopping: ResMut<Shopping>,
    mut shelf: ResMut<Shelf>,
    deal: Res<Deal>,
    lang: Res<Lang>,
    art: Res<Art>,
    fonts: Res<Fonts>,
    session: Res<Session>,
    settings: Res<Settings>,
    old: Query<Entity, With<ShopPanel>>,
    mut images: ResMut<Assets<Image>>,
    mut shown: Local<String>,
) {
    // The shop is open for as long as the bag is.
    if *phase != Phase::Bag && shopping.0.is_some() {
        shopping.0 = None;
    }
    let world = &game.world;
    let me = player_name(&settings);
    let hero = my_hero(world, &me, matches!(*session, Session::Solo)).unwrap_or(0);
    let keeper = shopping.0.as_deref().and_then(|id| world.here().creatures.iter().find(|c| c.id == id));
    let wares = shopping.0.as_deref().map(|id| world.wares(id)).unwrap_or_default();
    let gold = world.story.bags.get(hero).map_or(0, |b| b.gold);
    // A shelf with nothing on it is passed over for the first that has something.
    if keeper.is_some() && !wares.iter().any(|(w, _)| Shelf::of(w) == *shelf) {
        if let Some(full) = Shelf::ALL.into_iter().find(|s| wares.iter().any(|(w, _)| Shelf::of(w) == *s)) {
            *shelf = full;
        }
    }
    let print = match keeper {
        Some(k) => format!("{}|{gold}|{}|{}|{:?}|{:?}|{:?}", k.id, k.haggled, deal.0, *lang, *shelf, wares.iter().map(|(w, p)| (&w.name, w.count, *p)).collect::<Vec<_>>()),
        None => String::new(),
    };
    if *shown == print && (print.is_empty() || !old.is_empty()) {
        return;
    }
    *shown = print;
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let Some(keeper) = keeper else { return };
    let ink = TextColor(color(INK));
    let cell = CELL as f32;
    let root = Node { position_type: PositionType::Absolute, left: px(2), top: px(2), width: px(236), height: px(252), ..default() };
    commands.spawn((ShopPanel, DespawnOnExit(Screen::Play), root, GlobalZIndex(11))).with_children(|panel| {
        panel.spawn(panel_backing(&art));
        let at = |left: f32, top: f32, width: f32, height: f32| Node { position_type: PositionType::Absolute, left: px(left), top: px(top), width: px(width), height: px(height), ..default() };
        panel.spawn((at(10.0, 6.0, 150.0, 12.0), Text::new(keeper.name.clone()), fonts.title(1), ink, TextLayout::no_wrap()));
        panel.spawn((at(166.0, 8.0, 64.0, 10.0), Text::new(format!("{} {gold}", T::GoldCount.text(*lang))), fonts.body(), TextColor(color(WAX_DARK)), TextLayout::no_wrap()));
        // The shelves: one tab each, those with nothing on them dimmed.
        panel.spawn(Node { column_gap: px(2), ..at(10.0, 24.0, 220.0, 18.0) }).with_children(|tabs| {
            for tab in Shelf::ALL {
                let count = wares.iter().filter(|(w, _)| Shelf::of(w) == tab).count();
                let label = if tab == *shelf { format!("[{}]", tab.label().text(*lang)) } else { tab.label().text(*lang).to_string() };
                if count > 0 {
                    text_button(tabs, &art, &fonts, label, ShopAct::Shelf(tab));
                } else {
                    tabs.spawn((Node { padding: UiRect::axes(px(5), px(3)), ..default() }, Text::new(label), fonts.body(), TextColor(color(FADED))));
                }
            }
        });
        // The grid, and the wares of this shelf on it, each at its own size.
        panel.spawn((at(GRID_LEFT, GRID_TOP, BAG_W as f32 * cell, BAG_H as f32 * cell), BackgroundColor(color(WOOD_DEEP)))).with_children(|grid| {
            for y in 0..BAG_H {
                for x in 0..BAG_W {
                    grid.spawn((Node { position_type: PositionType::Absolute, left: px(x as f32 * cell + 1.0), top: px(y as f32 * cell + 1.0), width: px(cell - 2.0), height: px(cell - 2.0), ..default() }, BackgroundColor(color(WOOD_DARK)), Pickable::IGNORE));
                }
            }
            for (n, (x, y), (w, h)) in laid_out(&wares, *shelf) {
                let (ware, price) = wares[n];
                let picture = images.add(crate::bag_ui::picture(ware, world).into_image());
                let afford = gold >= price;
                grid.spawn((
                    Button,
                    PixelButton::plain(),
                    crate::widgets::ReplyLine,
                    ShopAct::Buy(n),
                    Node { position_type: PositionType::Absolute, left: px(x as f32 * cell), top: px(y as f32 * cell), width: px(w as f32 * cell), height: px(h as f32 * cell), ..default() },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|slot| {
                    slot.spawn((Node { width: percent(100), height: percent(100), ..default() }, ImageNode { color: if afford { Color::WHITE } else { Color::srgba(1.0, 0.6, 0.6, 0.75) }, ..ImageNode::new(picture) }, Pickable::IGNORE));
                    if ware.count > 1 {
                        slot.spawn((Node { position_type: PositionType::Absolute, right: px(1), bottom: px(-1), ..default() }, Text::new(ware.count.to_string()), fonts.body(), TextColor(color(PARCHMENT)), Pickable::IGNORE));
                    }
                });
            }
        });
        // What the cursor is over, or what the last deal came to.
        let below = GRID_TOP + BAG_H as f32 * cell + 3.0;
        panel.spawn((WareInfo, Node { overflow: Overflow::clip(), ..at(10.0, below, 218.0, 44.0) }, Text::new(if deal.0.is_empty() { T::ShopHint.text(*lang).to_string() } else { deal.0.clone() }), fonts.body(), TextColor(color(if deal.0.is_empty() { FADED } else { MOSS }))));
        panel.spawn(Node { column_gap: px(4), ..at(10.0, 228.0, 220.0, 18.0) }).with_children(|line| {
            button(line, &art, &fonts, T::Haggle, !keeper.haggled && world.shop_open(), ShopAct::Haggle);
            button(line, &art, &fonts, T::Leave, true, ShopAct::Leave);
            if !world.shop_open() {
                line.spawn((Text::default(), T::ShopClosed, fonts.body(), TextColor(color(WAX))));
            }
        });
    });
}

/// The ware under the cursor is told below the grid: its name, what it does, what it costs.
fn tell(game: Res<Game>, shopping: Res<Shopping>, deal: Res<Deal>, lang: Res<Lang>, wares: Query<(&ShopAct, &Interaction)>, mut info: Query<(&mut Text, &mut TextColor), With<WareInfo>>) {
    let Some(keeper) = shopping.0.as_deref() else { return };
    let Ok((mut text, mut tint)) = info.single_mut() else { return };
    let over = wares.iter().find_map(|(act, interaction)| match (act, interaction) {
        (ShopAct::Buy(n), Interaction::Hovered | Interaction::Pressed) => Some(*n),
        _ => None,
    });
    let listed = game.world.wares(keeper);
    let (words, wanted) = match over.and_then(|n| listed.get(n)) {
        Some((ware, price)) => {
            let lines: Vec<String> = crate::bag_ui::item_lines(ware, *lang).into_iter().map(|(line, _)| line).filter(|line| !line.starts_with(T::Price.text(*lang)) && *line != T::BagUseHint.text(*lang)).collect();
            (format!("{}\n{} {price}", lines.join(" · "), T::Price.text(*lang)), color(INK))
        }
        None if deal.0.is_empty() => (T::ShopHint.text(*lang).to_string(), color(FADED)),
        None => (deal.0.clone(), color(MOSS)),
    };
    if text.0 != words {
        text.0 = words;
        tint.0 = wanted;
    }
}

#[allow(clippy::too_many_arguments)]
fn on_activate(
    used: On<Activate>,
    acts: Query<&ShopAct>,
    mut game: Option<ResMut<Game>>,
    mut shopping: ResMut<Shopping>,
    mut shelf: ResMut<Shelf>,
    mut deal: ResMut<Deal>,
    mut phase: ResMut<Phase>,
    session: Res<Session>,
    settings: Res<Settings>,
    lang: Res<Lang>,
    mut sound: MessageWriter<Play>,
) {
    let (Ok(act), Some(game)) = (acts.get(used.entity), game.as_mut()) else { return };
    let Some(keeper) = shopping.0.clone() else { return };
    let me = player_name(&settings);
    let hero = my_hero(&game.world, &me, matches!(*session, Session::Solo)).unwrap_or(0);
    match *act {
        ShopAct::Shelf(tab) => {
            *shelf = tab;
            sound.write(Play(Sfx::Page));
        }
        ShopAct::Buy(n) => {
            if session.is_client() {
                session.intend(Intent::Buy { keeper: keeper.clone(), ware: n });
            }
            match game.world.buy(hero, &keeper, n) {
                Ok(name) => {
                    deal.0 = format!("{} {name}", T::Bought.text(*lang));
                    sound.write(Play(Sfx::Item));
                }
                Err(trouble) => {
                    deal.0 = trouble_words(trouble, *lang);
                    sound.write(Play(Sfx::Error));
                }
            }
        }
        ShopAct::Haggle => {
            if session.is_client() {
                session.intend(Intent::Haggle { keeper: keeper.clone() });
            }
            match game.world.haggle(hero, &keeper, &mut rules::Rng::from_clock()) {
                Some(true) => {
                    deal.0 = T::HaggleWon.text(*lang).to_string();
                    sound.write(Play(Sfx::Confirm));
                }
                Some(false) => {
                    deal.0 = T::HaggleLost.text(*lang).to_string();
                    sound.write(Play(Sfx::Error));
                }
                None => {}
            }
        }
        ShopAct::Leave => {
            shopping.0 = None;
            deal.0.clear();
            *phase = Phase::Walking;
        }
    }
}

pub struct ShopPlugin;

impl Plugin for ShopPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shopping>()
            .init_resource::<Deal>()
            .init_resource::<Shelf>()
            .add_systems(Update, (build, tell).chain().run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>))
            .add_observer(on_activate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wares_lie_on_their_shelves_each_in_a_place_of_its_own() {
        let sword = world::Item { name: "Sword".into(), count: 1, gear: Some("longsword".into()), ..Default::default() };
        let mail = world::Item { name: "Mail".into(), count: 1, gear: Some("chain-mail".into()), ..Default::default() };
        let potion = world::Item { name: "Potion".into(), count: 3, kind: "potion".into(), heal: "2d4".into(), ..Default::default() };
        let idol = world::Item { name: "Idol".into(), count: 1, ..Default::default() };
        assert_eq!([Shelf::of(&sword), Shelf::of(&mail), Shelf::of(&potion), Shelf::of(&idol)], [Shelf::Weapons, Shelf::Armour, Shelf::Supplies, Shelf::Other]);
        let wares = [(&sword, 15), (&potion, 50), (&sword, 15), (&mail, 75)];
        let weapons = laid_out(&wares, Shelf::Weapons);
        assert_eq!(weapons.iter().map(|w| w.0).collect::<Vec<_>>(), [0, 2]);
        // Two swords do not lie on top of one another, and each keeps its size.
        assert_ne!(weapons[0].1, weapons[1].1);
        assert_eq!(weapons[0].2, sword.size());
        assert_eq!(laid_out(&wares, Shelf::Armour)[0].0, 3);
        assert!(laid_out(&wares, Shelf::Other).is_empty());
    }
}
