//! A shop, as in the old games: the keeper's wares on the left, one's own bag on the right.
//! A click on a ware buys it; a right click on a thing of one's own sells it.

use crate::art::Art;
use crate::audio::{Play, Sfx};
use crate::coop::{Intent, Session, player_name};
use crate::i18n::{Lang, T};
use crate::menu::Screen;
use crate::play::{Game, Phase, my_hero};
use crate::store::Settings;
use crate::theme::*;
use crate::widgets::{Activate, button, panel_backing, scroll_list};
use bevy::prelude::*;
use world::shop::ShopTrouble;

/// The keeper being traded with, while the bag is open for it.
#[derive(Resource, Default)]
pub struct Shopping(pub Option<String>);

/// What the last deal came to, for the line under the wares.
#[derive(Resource, Default)]
pub struct Deal(pub String);

#[derive(Component)]
struct ShopPanel;

#[derive(Component, Clone, Copy)]
enum ShopAct {
    Buy(usize),
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

/// What a ware does, in a few words after its name.
fn ware_words(item: &world::Item, lang: Lang) -> String {
    let mut words = Vec::new();
    if let Some(index) = &item.gear {
        words.push(world::bag::gear_name(index, lang == Lang::Ru));
    }
    if !item.heal.trim().is_empty() {
        words.push(format!("{} {}", T::BagHeals.text(lang), item.heal));
    }
    if !item.damage.trim().is_empty() {
        words.push(format!("{} {}", T::BagDamage.text(lang), item.damage));
    }
    match item.rarity.as_str() {
        "uncommon" => words.push(T::RarityUncommon.text(lang).to_string()),
        "rare" => words.push(T::RarityRare.text(lang).to_string()),
        "legendary" => words.push(T::RarityLegendary.text(lang).to_string()),
        _ => {}
    }
    if words.is_empty() { String::new() } else { format!(" ({})", words.join(", ")) }
}

#[allow(clippy::too_many_arguments)]
fn build(
    mut commands: Commands,
    phase: Res<Phase>,
    game: Res<Game>,
    mut shopping: ResMut<Shopping>,
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
    let print = match keeper {
        Some(k) => format!("{}|{gold}|{}|{}|{:?}|{:?}", k.id, k.haggled, deal.0, *lang, wares.iter().map(|(w, p)| (&w.name, w.count, *p)).collect::<Vec<_>>()),
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
    let faded = TextColor(color(FADED));
    let root = Node { position_type: PositionType::Absolute, left: px(2), top: px(2), width: px(236), height: px(252), padding: UiRect::all(px(8)), flex_direction: FlexDirection::Column, row_gap: px(3), ..default() };
    commands.spawn((ShopPanel, DespawnOnExit(Screen::Play), root, GlobalZIndex(11))).with_children(|panel| {
        panel.spawn(panel_backing(&art));
        panel.spawn((Text::new(format!("{} — {}", T::ShopTitle.text(*lang), keeper.name)), fonts.title(1), ink, TextLayout::no_wrap()));
        panel.spawn((Text::new(format!("{} {gold}", T::GoldCount.text(*lang))), fonts.body(), TextColor(color(WAX_DARK))));
        if !world.shop_open() {
            panel.spawn((Text::default(), T::ShopClosed, fonts.body(), TextColor(color(WAX))));
        }
        panel.spawn(scroll_list(Node { flex_direction: FlexDirection::Column, row_gap: px(1), width: percent(100), ..default() }, 150.0)).with_children(|rows| {
            for (n, (ware, price)) in wares.iter().enumerate() {
                let picture = images.add(crate::bag_ui::picture(ware, world).into_image());
                let many = if ware.count > 1 { format!(" ×{}", ware.count) } else { String::new() };
                let afford = gold >= *price;
                rows.spawn((
                    Button,
                    crate::widgets::PixelButton::plain(),
                    crate::widgets::ReplyLine,
                    ShopAct::Buy(n),
                    Node { width: percent(100), column_gap: px(4), align_items: AlignItems::Center, padding: UiRect::horizontal(px(2)), ..default() },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|row| {
                    row.spawn((Node { width: px(16), height: px(16), flex_shrink: 0.0, ..default() }, ImageNode::new(picture)));
                    row.spawn((Node { flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }, Text::new(format!("{}{many}{}", ware.name, ware_words(ware, *lang))), fonts.body(), TextColor(color(if afford { WOOD_DARK } else { FADED }))));
                    row.spawn((Node { flex_shrink: 0.0, ..default() }, Text::new(price.to_string()), fonts.body(), TextColor(color(if afford { WAX_DARK } else { WAX })), TextLayout::no_wrap()));
                });
            }
        });
        if !deal.0.is_empty() {
            panel.spawn((Text::new(deal.0.clone()), fonts.body(), TextColor(color(MOSS))));
        }
        panel.spawn((Text::default(), T::ShopHint, fonts.body(), faded));
        panel.spawn(Node { column_gap: px(4), ..default() }).with_children(|line| {
            button(line, &art, &fonts, T::Haggle, !keeper.haggled && world.shop_open(), ShopAct::Haggle);
            button(line, &art, &fonts, T::Leave, true, ShopAct::Leave);
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn on_activate(used: On<Activate>, acts: Query<&ShopAct>, mut game: Option<ResMut<Game>>, mut shopping: ResMut<Shopping>, mut deal: ResMut<Deal>, mut phase: ResMut<Phase>, session: Res<Session>, settings: Res<Settings>, lang: Res<Lang>, mut sound: MessageWriter<Play>) {
    let (Ok(act), Some(game)) = (acts.get(used.entity), game.as_mut()) else { return };
    let Some(keeper) = shopping.0.clone() else { return };
    let me = player_name(&settings);
    let hero = my_hero(&game.world, &me, matches!(*session, Session::Solo)).unwrap_or(0);
    match *act {
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
        app.init_resource::<Shopping>().init_resource::<Deal>().add_systems(Update, build.run_if(in_state(Screen::Play)).run_if(resource_exists::<Game>)).add_observer(on_activate);
    }
}
