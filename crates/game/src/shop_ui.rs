//! A shop, as in Diablo 2: the keeper's wares laid out on a grid, each thing its own picture
//! at its own size, sorted onto tabs; one's own bag on the right. A click on a ware buys it;
//! a right click on a thing of one's own sells it. What the cursor is over is told below.

use crate::audio::{Play, Sfx};
use crate::coop::{Intent, Session, player_name};
use crate::i18n::{Lang, T};
use crate::menu::Screen;
use crate::play::{Game, Phase, my_hero};
use crate::store::Settings;
use crate::theme::*;
use crate::ui::egui::{self, Align, Color32, Layout, Pos2, Rect, RichText, Sense, Stroke, UiBuilder, vec2};
use crate::ui::kit::{self, button, hint};
use crate::ui::theme::c;
use bevy_egui::EguiContexts;
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

/// A press in the shop.
#[derive(Clone, Copy, Debug)]
pub enum ShopAct {
    Buy(usize),
    Shelf(Shelf),
    Haggle,
    Leave,
}

#[derive(Event, Clone, Copy)]
pub struct ShopPress(pub ShopAct);

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

/// The shop closes with the bag.
fn close(phase: Res<Phase>, mut shopping: ResMut<Shopping>) {
    if *phase != Phase::Bag && shopping.0.is_some() {
        shopping.0 = None;
    }
}

/// The keeper's wares beside the bag, drawn with egui: the shelves as tabs, the wares on a
/// grid each at its own size, what the cursor is over told under it, and the deal's buttons.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw(
    mut egui: EguiContexts,
    mut commands: Commands,
    (game, shopping, deal, lang, session, settings): (Res<Game>, Res<Shopping>, Res<Deal>, Res<Lang>, Res<Session>, Res<Settings>),
    mut shelf: ResMut<Shelf>,
    (mut pictures, mut images): (ResMut<crate::bag_ui::draw::Pictures>, ResMut<Assets<Image>>),
) {
    let Some(id) = shopping.0.as_deref() else { return };
    let world = &game.world;
    let Some(keeper) = world.here().creatures.iter().find(|c| c.id == id) else { return };
    let lang = *lang;
    let t = |text: T| text.text(lang);
    let me = player_name(&settings);
    let hero = my_hero(world, &me, matches!(*session, Session::Solo)).unwrap_or(0);
    let wares = world.wares(id);
    let gold = world.story.bags.get(hero).map_or(0, |b| b.gold);
    // A shelf with nothing on it is passed over for the first that has something.
    if !wares.iter().any(|(w, _)| Shelf::of(w) == *shelf)
        && let Some(full) = Shelf::ALL.into_iter().find(|s| wares.iter().any(|(w, _)| Shelf::of(w) == *s))
    {
        *shelf = full;
    }
    let laid: Vec<(usize, (u8, u8), (u8, u8), (egui::TextureId, Vec2))> = laid_out(&wares, *shelf).into_iter().map(|(n, at, size)| (n, at, size, crate::bag_ui::draw::picture_id(&mut pictures, &mut egui, &mut images, wares[n].0, world))).collect();

    let Ok(ctx) = egui.ctx_mut() else { return };
    let Some(mut ui) = kit::screen_ui(ctx, "shop") else { return };
    let (_, rect) = crate::bag_ui::draw::panels(ui.max_rect(), true);
    let mut pressed: Vec<ShopAct> = Vec::new();
    kit::plate(ui.painter(), rect, kit::Look::Parchment, true);
    let inner = rect.shrink(10.0);
    let mut head = ui.new_child(UiBuilder::new().id_salt("shop_head").max_rect(inner).layout(Layout::top_down(Align::Min)));
    head.horizontal(|row| {
        kit::title(row, keeper.name.as_str(), crate::ui::theme::SMALL_TITLE);
        row.with_layout(Layout::right_to_left(Align::Center), |row| {
            row.label(RichText::new(format!("{} {gold}", t(T::GoldCount))).color(c(WAX_DARK)));
        });
    });
    hint(&mut head, t(T::ShopHint));
    head.horizontal_wrapped(|row| {
        for tab in Shelf::ALL {
            let count = wares.iter().filter(|(w, _)| Shelf::of(w) == tab).count();
            let mut b = button(tab.label().text(lang)).enabled(count > 0, t(T::ShelfEmpty));
            if tab == *shelf {
                b = b.primary();
            }
            if row.add(b).clicked() {
                pressed.push(ShopAct::Shelf(tab));
            }
        }
    });
    let top = head.min_rect().bottom() + 6.0;
    // The footer at the bottom first; the grid takes what room is left above the words
    // about a ware.
    let mut foot = ui.new_child(UiBuilder::new().id_salt("shop_foot").max_rect(inner).layout(Layout::bottom_up(Align::Min)));
    foot.horizontal(|row| {
        if row.add(button(t(T::Leave)).key("Esc")).clicked() {
            pressed.push(ShopAct::Leave);
        }
        let open = world.shop_open();
        let why = if open { t(T::HaggleDone) } else { t(T::ShopClosed) };
        if row.add(button(t(T::Haggle)).enabled(!keeper.haggled && open, why)).clicked() {
            pressed.push(ShopAct::Haggle);
        }
        if !open {
            kit::warning(row, t(T::ShopClosed));
        }
    });
    const INFO: f32 = 40.0;
    let cell = crate::bag_ui::draw::cell_for(inner.width(), foot.min_rect().top() - top - INFO - 12.0, BAG_H as f32);
    let grid = Rect::from_min_size(Pos2::new(inner.left(), top), vec2(BAG_W as f32 * cell, BAG_H as f32 * cell));
    let painter = ui.painter().clone();
    painter.rect_stroke(grid, 0.0, Stroke::new(1.0, c(WOOD_DEEP)), egui::StrokeKind::Outside);
    for y in 0..BAG_H {
        for x in 0..BAG_W {
            let shade = if (x + y) % 2 == 0 { WOOD_DARK } else { [0x4a, 0x2e, 0x1d, 0xff] };
            painter.rect_filled(Rect::from_min_size(grid.min + vec2(x as f32 * cell, y as f32 * cell), vec2(cell, cell)), 0.0, c(shade));
        }
    }
    let mut over = None;
    for (n, (x, y), (w, h), (texture, _)) in &laid {
        let (ware, price) = wares[*n];
        let place = Rect::from_min_size(grid.min + vec2(*x as f32 * cell, *y as f32 * cell), vec2(*w as f32 * cell, *h as f32 * cell));
        let response = ui.interact(place, egui::Id::new(("ware", *n)), Sense::click());
        let afford = gold >= price;
        if response.hovered() {
            over = Some(*n);
            painter.rect_stroke(place, 0.0, Stroke::new(1.0, c(BRASS_LIGHT)), egui::StrokeKind::Inside);
        }
        painter.image(*texture, place, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), if afford { Color32::WHITE } else { Color32::from_rgba_unmultiplied(0xff, 0x99, 0x99, 0xbf) });
        if ware.count > 1 {
            painter.text(place.right_bottom(), egui::Align2::RIGHT_BOTTOM, ware.count.to_string(), egui::FontId::proportional(9.0), c(PARCHMENT));
        }
        if response.clicked() {
            pressed.push(ShopAct::Buy(*n));
        }
    }
    // What the cursor is over, or what the last deal came to.
    let mut info = ui.new_child(UiBuilder::new().id_salt("shop_info").max_rect(Rect::from_min_max(Pos2::new(inner.left(), grid.bottom() + 6.0), inner.max)).layout(Layout::top_down(Align::Min)));
    match over.and_then(|n| wares.get(n)) {
        Some((ware, price)) => {
            let lines: Vec<String> = crate::bag_ui::item_lines(ware, lang).into_iter().map(|(line, _)| line).filter(|line| !line.starts_with(t(T::Price)) && *line != t(T::BagUseHint)).collect();
            info.add(egui::Label::new(RichText::new(lines.join(" · ")).color(c(INK))).wrap());
            info.label(RichText::new(format!("{} {price}", t(T::Price))).color(c(if gold >= *price { INK } else { WAX })));
        }
        None if !deal.0.is_empty() => {
            info.label(RichText::new(&deal.0).color(c(MOSS)));
        }
        None => {}
    }
    for act in pressed {
        commands.trigger(ShopPress(act));
    }
}

#[allow(clippy::too_many_arguments)]
fn on_activate(
    used: On<ShopPress>,
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
    let act = &used.0;
    let Some(game) = game.as_mut() else { return };
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
            .add_systems(Update, close.run_if(in_state(Screen::Play)))
            .add_systems(bevy_egui::EguiPrimaryContextPass, draw.in_set(crate::ui::UiSet::Screens).run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>))
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
