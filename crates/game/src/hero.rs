//! Making a character for a world that has none yet, and showing the sheet of one that exists.

use crate::art::Art;
use crate::coop::{GmAct, Intent, Session, Up, owner_name};
use crate::i18n::{Lang, T};
use crate::menu::{Nav, Screen, screen_root, spawn_panel};
use crate::play::Game;
use crate::store::{Settings, Store};
use crate::theme::*;
use crate::widgets::{Activate, BUTTON_HEIGHT, backing, button, scroll_list, text_button};
use bevy::input_focus::AutoFocus;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigation;
use rules::character::{AbilityMethod, POINT_BUDGET, STANDARD_ARRAY, modifier, point_cost, points_spent, roll_scores, suggested_scores, suggested_spells};
use rules::{Ability, Character, Rng, srd, tr};

/// The character being put together.
#[derive(Resource)]
struct Draft {
    race: usize,
    class: usize,
    method: AbilityMethod,
    /// Scores before the people's bonuses.
    base: [i32; 6],
    /// The six numbers last rolled, kept so that changing class re-deals the same roll.
    rolled: [i32; 6],
    skills: Vec<String>,
    /// Cantrips and spells the player chose; the usual ones for the class until they do.
    spells: Vec<String>,
    /// The ability picked first of two to swap, under methods that deal fixed numbers.
    picked: Option<usize>,
    problem: Option<String>,
    look: rules::Look,
    tab: Tab,
    /// Which way the hero in the preview faces.
    facing: crate::doll::Facing,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Look,
    Stats,
}

/// One of the things about a hero's looks a player can turn through.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Feature {
    Build,
    Skin,
    Hair,
    HairColor,
    Beard,
    Eyes,
    Cloth,
    Trim,
}

impl Feature {
    const ALL: [Feature; 8] = [Feature::Build, Feature::Skin, Feature::Hair, Feature::HairColor, Feature::Beard, Feature::Eyes, Feature::Cloth, Feature::Trim];

    fn label(self) -> T {
        match self {
            Feature::Build => T::LookBuild,
            Feature::Skin => T::LookSkin,
            Feature::Hair => T::LookHair,
            Feature::HairColor => T::LookHairColor,
            Feature::Beard => T::LookBeard,
            Feature::Eyes => T::LookEyes,
            Feature::Cloth => T::LookCloth,
            Feature::Trim => T::LookTrim,
        }
    }

    fn count(self) -> usize {
        use crate::doll::*;
        match self {
            Feature::Build => BUILDS,
            Feature::Skin => SKINS.len(),
            Feature::Hair => HAIRS,
            Feature::HairColor => HAIR_COLORS.len(),
            Feature::Beard => BEARDS,
            Feature::Eyes => EYE_COLORS.len(),
            Feature::Cloth | Feature::Trim => CLOTHS.len(),
        }
    }

    fn slot(self, look: &mut rules::Look) -> &mut u8 {
        match self {
            Feature::Build => &mut look.build,
            Feature::Skin => &mut look.skin,
            Feature::Hair => &mut look.hair,
            Feature::HairColor => &mut look.hair_color,
            Feature::Beard => &mut look.beard,
            Feature::Eyes => &mut look.eyes,
            Feature::Cloth => &mut look.cloth,
            Feature::Trim => &mut look.trim,
        }
    }

    /// A swatch for the features that are colours.
    fn swatch(self, value: u8) -> Option<(u8, u8, u8)> {
        use crate::doll::*;
        let at = |list: &[(u8, u8, u8)]| list[value as usize % list.len()];
        match self {
            Feature::Skin => Some(at(&SKINS)),
            Feature::HairColor => Some(at(&HAIR_COLORS)),
            Feature::Eyes => Some(at(&EYE_COLORS)),
            Feature::Cloth | Feature::Trim => Some(at(&CLOTHS)),
            _ => None,
        }
    }

    /// The name of a choice that is a shape rather than a colour.
    fn name(self, value: u8, ru: bool) -> String {
        let pick = |names: &[(&str, &str)]| {
            let (r, e) = names[value as usize % names.len()];
            if ru { r } else { e }.to_string()
        };
        match self {
            Feature::Build => pick(&[("стройное", "slim"), ("среднее", "average"), ("крепкое", "broad")]),
            Feature::Hair => pick(&[
                ("короткая", "short"),
                ("бритая", "shaved"),
                ("до плеч", "medium"),
                ("длинная", "long"),
                ("хвост", "ponytail"),
                ("ирокез", "mohawk"),
                ("кудри", "curly"),
                ("пучок", "bun"),
                ("косы", "braids"),
            ]),
            Feature::Beard => pick(&[("нет", "none"), ("щетина", "stubble"), ("полная", "full"), ("длинная", "long"), ("усы", "moustache")]),
            _ => format!("{}/{}", value as usize % self.count() + 1, self.count()),
        }
    }
}

impl Default for Draft {
    fn default() -> Self {
        let class = srd().classes.iter().position(|c| c.index == "fighter").unwrap_or(0);
        Self { race: srd().races.iter().position(|r| r.index == "human").unwrap_or(0), class, method: AbilityMethod::PointBuy, base: suggested_scores("fighter", STANDARD_ARRAY), rolled: STANDARD_ARRAY, skills: Vec::new(), spells: Vec::new(), picked: None, problem: None, look: rules::Look::default(), tab: Tab::Look, facing: crate::doll::Facing::Down }
    }
}

impl Draft {
    /// A draft under the method the host chose for the world.
    fn with_method(method: AbilityMethod) -> Self {
        let mut draft = Self { method, ..Self::default() };
        draft.randomize(&mut Rng::from_clock());
        if method == AbilityMethod::Roll {
            draft.rolled = roll_scores(&mut Rng::from_clock());
        }
        draft.deal();
        draft
    }

    /// The character as it would be with the class's first skills and no spells: enough to
    /// know what it may cast.
    fn caster(&self) -> Option<Character> {
        let class = &srd().classes[self.class];
        let skills: Vec<String> = class.skill_options.iter().take(class.skill_picks).cloned().collect();
        Character::create("x", "", &srd().races[self.race].index, self.class_index(), self.base, &skills, &[]).ok()
    }

    /// The spells the class would usually start with.
    fn usual_spells(&self) -> Vec<String> {
        self.caster().map(|hero| suggested_spells(srd(), &hero)).unwrap_or_default()
    }

    /// Adds or drops a spell, within what the character may have of its kind.
    fn toggle_spell(&mut self, index: &str) {
        if let Some(at) = self.spells.iter().position(|s| s == index) {
            self.spells.remove(at);
            return;
        }
        let (Some(hero), Some(spell)) = (self.caster(), srd().spells.get(index)) else { return };
        let cantrip = spell.level == 0;
        let chosen = self.spells.iter().filter_map(|s| srd().spells.get(s)).filter(|s| (s.level == 0) == cantrip).count();
        let allowed = if cantrip { hero.cantrips_allowed() } else { hero.spells_allowed() };
        if chosen < allowed {
            self.spells.push(index.to_string());
        }
    }
}

impl Draft {
    /// A face to start from, so that no two new heroes look the same.
    fn randomize(&mut self, rng: &mut Rng) {
        for feature in Feature::ALL {
            *feature.slot(&mut self.look) = (rng.die(feature.count() as u32) - 1) as u8;
        }
        // Mostly the skin colours of people, now and then one of the stranger ones.
        let strange = rng.die(6) == 1;
        self.look.skin = (rng.die(if strange { crate::doll::SKINS.len() as u32 } else { 7 }) - 1) as u8;
        self.look.helmet = (rng.die(2) - 1) == 0;
    }

    fn turn(&mut self, feature: Feature, by: i32) {
        let count = feature.count() as i32;
        let slot = feature.slot(&mut self.look);
        *slot = (*slot as i32 + by).rem_euclid(count) as u8;
    }

    /// The hero as the choices so far would make it, to show it.
    fn preview(&self) -> Option<Character> {
        let class = &srd().classes[self.class];
        let preview = Draft { skills: class.skill_options.iter().take(class.skill_picks).cloned().collect(), picked: None, problem: None, ..clone_of(self) };
        preview.build("x", "").ok()
    }

    fn class_index(&self) -> &'static str {
        &srd().classes[self.class].index
    }

    /// Deals the scores again for the current class and method.
    fn deal(&mut self) {
        let values = if self.method == AbilityMethod::Roll { self.rolled } else { STANDARD_ARRAY };
        self.base = suggested_scores(self.class_index(), values);
        self.picked = None;
        self.spells = self.usual_spells();
    }

    fn points_left(&self) -> i32 {
        POINT_BUDGET as i32 - points_spent(&self.base).unwrap_or(POINT_BUDGET) as i32
    }

    /// Raises or lowers a score under point buy, if the budget and the 8–15 range allow.
    fn nudge(&mut self, ability: usize, by: i32) {
        let wanted = self.base[ability] + by;
        let Some(cost) = point_cost(wanted) else { return };
        let now = point_cost(self.base[ability]).unwrap_or(0);
        if cost as i32 - now as i32 <= self.points_left() {
            self.base[ability] = wanted;
        }
    }

    /// Under methods that deal fixed numbers, two picks swap the numbers of two abilities.
    fn pick(&mut self, ability: usize) {
        match self.picked.take() {
            Some(first) if first != ability => self.base.swap(first, ability),
            Some(_) => {}
            None => self.picked = Some(ability),
        }
    }

    fn toggle_skill(&mut self, skill: &str) {
        let class = &srd().classes[self.class];
        if let Some(at) = self.skills.iter().position(|s| s == skill) {
            self.skills.remove(at);
        } else if self.skills.len() < class.skill_picks && class.skill_options.iter().any(|s| s == skill) {
            self.skills.push(skill.to_string());
        }
    }

    fn build(&self, name: &str, description: &str) -> Result<Character, String> {
        let data = srd();
        let mut hero = Character::create(name, description, &data.races[self.race].index, self.class_index(), self.base, &self.skills, &[])?;
        // A change of scores can leave fewer spells allowed than were chosen: then the
        // usual ones are taken rather than refusing the character.
        if hero.set_spells(&self.spells).is_err() {
            let spells = suggested_spells(data, &hero);
            hero.set_spells(&spells)?;
        }
        hero.look = self.look;
        Ok(hero)
    }
}

#[derive(Component, Clone, PartialEq)]
enum Action {
    Race,
    Class,
    Minus(usize),
    Plus(usize),
    Pick(usize),
    Reroll,
    Skill(String),
    Spell(String),
    Create,
    /// Take the hero of a player who is away instead of making a new one.
    Take(usize),
    /// Sit in the master's seat and play no hero.
    Lead,
    Show(Tab),
    Turn(Feature, i32),
    Helmet,
    Random,
    Rotate,
}

/// The hero in the preview, walking on the spot.
#[derive(Component)]
struct Preview(Vec<Handle<Image>>, crate::doll::Facing);

#[derive(Component)]
struct NameInput;

#[derive(Component)]
struct StoryInput;

/// The part of the screen that is rebuilt whenever the draft changes.
#[derive(Component)]
struct Choices;

fn method_label(method: AbilityMethod) -> T {
    match method {
        AbilityMethod::PointBuy => T::MethodPointBuy,
        AbilityMethod::StandardArray => T::MethodArray,
        AbilityMethod::Roll => T::MethodRoll,
    }
}

fn signed(n: i32) -> String {
    if n >= 0 { format!("+{n}") } else { n.to_string() }
}

fn field(parent: &mut ChildSpawnerCommands, art: &Art, fonts: &Fonts, width: Val, marker: impl Bundle, limit: usize) {
    parent.spawn(Node { width, height: px(BUTTON_HEIGHT), padding: UiRect::axes(px(5), px(0)), align_items: AlignItems::Center, overflow: Overflow::clip(), flex_shrink: 0.0, ..default() }).with_children(|slot| {
        slot.spawn(backing(art.field.clone(), 4.0));
        slot.spawn((
            marker,
            AutoDirectionalNavigation::default(),
            Node { width: percent(100), ..default() },
            EditableText { allow_newlines: false, max_characters: Some(limit), ..default() },
            TextLayout::no_wrap(),
            fonts.body(),
            TextColor(color(INK)),
            TextCursorStyle { color: color(INK), selection_color: color(BRASS_LIGHT), ..default() },
        ));
    });
}

fn spawn_screen(mut commands: Commands, art: Res<Art>, fonts: Res<Fonts>, game: Option<Res<Game>>, store: Res<Store>, mut next: ResMut<NextState<Screen>>, session: Res<Session>, lang: Res<Lang>) {
    let Some(game) = game else {
        next.set(Screen::Main);
        return;
    };
    // Saved at once, so leaving now and choosing Continue comes back to this screen.
    if let Err(e) = store.save_game(&game.world_id, &game.world, false) {
        error!("could not save the game: {e}");
    }
    commands.insert_resource(Draft::with_method(game.world.table.method));
    let ink = TextColor(color(INK));
    commands.spawn(screen_root(Screen::Hero)).with_children(|root| {
        let sheet = Node { margin: UiRect::top(px(6)), width: px(464), flex_direction: FlexDirection::Column, row_gap: px(4), ..default() };
        spawn_panel(root, &art, sheet, |sheet| {
            sheet.spawn((Text::default(), T::HeroTitle, fonts.title(1), ink));
            // At a shared table the heroes of players who are away can be taken over.
            let free: Vec<usize> = if matches!(*session, Session::Solo) { Vec::new() } else { (0..game.world.party.len()).filter(|i| game.world.is_away(*i) && !game.world.party[*i].dead).collect() };
            if !free.is_empty() {
                let ru = *lang == Lang::Ru;
                sheet.spawn(Node { column_gap: px(4), row_gap: px(2), flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, ..default() }).with_children(|line| {
                    line.spawn((Text::default(), T::FreeHeroes, fonts.body(), TextColor(color(FADED))));
                    for i in free {
                        let hero = &game.world.party[i];
                        let class = srd().class(&hero.class).map_or(hero.class.as_str(), |c| tr(ru, &c.name));
                        text_button(line, &art, &fonts, format!("{} ({class} {})", hero.name, hero.level), Action::Take(i));
                    }
                });
            }
            sheet.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                line.spawn((Node { width: px(60), flex_shrink: 0.0, ..default() }, Text::default(), T::HeroName, fonts.body(), ink));
                field(line, &art, &fonts, px(150), (NameInput, AutoFocus), 30);
            });
            sheet.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                line.spawn((Node { width: px(60), flex_shrink: 0.0, ..default() }, Text::default(), T::HeroStory, fonts.body(), ink));
                field(line, &art, &fonts, px(372), StoryInput, 300);
            });
            sheet.spawn((Choices, Node { flex_direction: FlexDirection::Column, row_gap: px(4), ..default() }));
        });
    });
}

/// Rebuilds everything that depends on the draft. The two text fields are left alone, so
/// what was typed in them survives.
#[allow(clippy::too_many_arguments)]
fn redraw(
    mut commands: Commands,
    draft: Res<Draft>,
    lang: Res<Lang>,
    art: Res<Art>,
    fonts: Res<Fonts>,
    choices: Query<Entity, With<Choices>>,
    fresh: Query<(), Added<Choices>>,
    session: Res<Session>,
    game: Option<Res<Game>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Ok(choices) = choices.single() else { return };
    if !draft.is_changed() && !lang.is_changed() && fresh.is_empty() {
        return;
    }
    let data = srd();
    let ru = *lang == Lang::Ru;
    let (race, class) = (&data.races[draft.race], &data.classes[draft.class]);
    let ink = TextColor(color(INK));
    let faded = TextColor(color(FADED));
    commands.entity(choices).despawn_children().with_children(|area| {
        area.spawn(Node { column_gap: px(4), align_items: AlignItems::Center, ..default() }).with_children(|line| {
            text_button(line, &art, &fonts, format!("{}: {}", T::HeroRace.text(*lang), tr(ru, &race.name)), Action::Race);
            text_button(line, &art, &fonts, format!("{}: {}", T::HeroClass.text(*lang), tr(ru, &class.name)), Action::Class);
            // The host chose the method for the whole world.
            line.spawn((Text::new(method_label(draft.method).text(*lang).to_string()), fonts.body(), faded));
            if draft.method == AbilityMethod::Roll && draft.tab == Tab::Stats {
                button(line, &art, &fonts, T::Reroll, true, Action::Reroll);
            }
            line.spawn(Node { flex_grow: 1.0, ..default() });
            for (tab, label) in [(Tab::Look, T::TabLook), (Tab::Stats, T::TabStats)] {
                let mark = if draft.tab == tab { "> " } else { "" };
                text_button(line, &art, &fonts, format!("{mark}{}", label.text(*lang)), Action::Show(tab));
            }
        });
        if draft.tab == Tab::Look {
            look_tab(area, &draft, *lang, &art, &fonts, &mut images, &session, game.as_deref());
            return;
        }
        area.spawn(Node { column_gap: px(10), align_items: AlignItems::FlexStart, ..default() }).with_children(|columns| {
            columns.spawn(Node { width: px(214), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: px(1), ..default() }).with_children(|left| {
                let hint = match draft.method {
                    AbilityMethod::PointBuy => format!("{} {}", T::PointsLeft.text(*lang), draft.points_left()),
                    _ => T::SwapHint.text(*lang).to_string(),
                };
                left.spawn((Text::new(hint), fonts.body(), faded));
                for ability in Ability::ALL {
                    let i = ability as usize;
                    let total = draft.base[i] + race.bonuses[i] + class.background[i];
                    left.spawn(Node { column_gap: px(3), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                        let picked = draft.picked == Some(i);
                        let name = TextColor(color(if picked { WAX } else { INK }));
                        line.spawn((Node { width: px(92), flex_shrink: 0.0, ..default() }, Text::new(tr(ru, ability.name()).to_string()), TextLayout::no_wrap(), fonts.body(), name));
                        line.spawn((Node { width: px(54), flex_shrink: 0.0, ..default() }, Text::new(format!("{total} ({})", signed(modifier(total)))), TextLayout::no_wrap(), fonts.body(), ink));
                        if draft.method == AbilityMethod::PointBuy {
                            text_button(line, &art, &fonts, "-".to_string(), Action::Minus(i));
                            text_button(line, &art, &fonts, "+".to_string(), Action::Plus(i));
                        } else {
                            button(line, &art, &fonts, T::Swap, true, Action::Pick(i));
                        }
                    });
                }
            });
            columns.spawn(Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }).with_children(|right| {
                right.spawn((Text::new(format!("{} {}/{}", T::HeroSkills.text(*lang), draft.skills.len(), class.skill_picks)), fonts.body(), faded));
                let caster = draft.caster().filter(|hero| hero.cantrips_allowed() + hero.spells_allowed() > 0);
                let skill_rows = if caster.is_some() { 2.5 } else { 4.0 };
                right.spawn(scroll_list(Node { flex_direction: FlexDirection::Column, row_gap: px(1), ..default() }, skill_rows * (BUTTON_HEIGHT + 1.0))).with_children(|rows| {
                    for skill in &class.skill_options {
                        let name = data.skill(skill).map_or(skill.as_str(), |s| tr(ru, &s.name));
                        let mark = if draft.skills.contains(skill) { "[x]" } else { "[ ]" };
                        text_button(rows, &art, &fonts, format!("{mark} {name}"), Action::Skill(skill.clone()));
                    }
                });
                if let Some(hero) = &caster {
                    let chosen = |cantrips: bool| draft.spells.iter().filter_map(|s| data.spells.get(s)).filter(|s| (s.level == 0) == cantrips).count();
                    right.spawn((
                        Text::new(format!("{} {}/{} · {} {}/{}", T::Cantrips.text(*lang), chosen(true), hero.cantrips_allowed(), T::Spells.text(*lang), chosen(false), hero.spells_allowed())),
                        fonts.body(),
                        faded,
                    ));
                    right.spawn(scroll_list(Node { flex_direction: FlexDirection::Column, row_gap: px(1), ..default() }, 2.5 * (BUTTON_HEIGHT + 1.0))).with_children(|rows| {
                        let mut pool = data.class_spells(&hero.class, hero.highest_slot());
                        pool.sort_by_key(|s| (s.level, tr(ru, &s.name).to_string()));
                        for spell in pool {
                            let mark = if draft.spells.contains(&spell.index) { "[x]" } else { "[ ]" };
                            // Spells the rules cannot carry out by themselves go through the master.
                            let kind = if spell.mechanised() { "" } else { " *" };
                            text_button(rows, &art, &fonts, format!("{mark} {} {}{kind}", spell.level, tr(ru, &spell.name)), Action::Spell(spell.index.clone()));
                        }
                    });
                }
                // What the choices come to, so the player sees the character before making it.
                if let Some(hero) = draft.preview() {
                    right.spawn((Text::new(format!("{} {} · {} {}", T::HitPoints.text(*lang), hero.max_hp, T::ArmorClass.text(*lang), hero.ac())), fonts.body(), ink));
                }
                finish(right, &draft, &art, &fonts, &session, game.as_deref());
            });
        });
    });
}

/// What ends either tab: a problem with the hero, if any, and the ways out of the screen.
fn finish(parent: &mut ChildSpawnerCommands, draft: &Draft, art: &Art, fonts: &Fonts, session: &Session, game: Option<&Game>) {
    if let Some(problem) = &draft.problem {
        parent.spawn((Text::new(problem.clone()), fonts.body(), TextColor(color(WAX))));
    }
    parent.spawn(Node { column_gap: px(6), ..default() }).with_children(|line| {
        button(line, art, fonts, T::Back, true, Nav::Go(Screen::Main));
        button(line, art, fonts, T::CreateHero, true, Action::Create);
    });
    // At a shared table somebody may run the game instead of playing in it.
    let seat_free = game.is_some_and(|g| g.world.gm.as_deref().is_none_or(|gm| !g.world.present.iter().any(|p| p == gm)));
    if !matches!(*session, Session::Solo) && seat_free {
        parent.spawn(Node::default()).with_children(|slot| button(slot, art, fonts, T::SeatNoHero, true, Action::Lead));
    }
}

/// The looks of the hero: a large figure that walks on the spot, and a row for each feature.
#[allow(clippy::too_many_arguments)]
fn look_tab(area: &mut ChildSpawnerCommands, draft: &Draft, lang: Lang, art: &Art, fonts: &Fonts, images: &mut Assets<Image>, session: &Session, game: Option<&Game>) {
    let ru = lang == Lang::Ru;
    let ink = TextColor(color(INK));
    area.spawn(Node { column_gap: px(10), align_items: AlignItems::FlexStart, ..default() }).with_children(|columns| {
        columns.spawn(Node { width: px(150), flex_shrink: 0.0, flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(3), ..default() }).with_children(|left| {
            // The figure at four times its size in the world, on a patch of ground.
            left.spawn((Node { width: px(120), height: px(118), justify_content: JustifyContent::Center, align_items: AlignItems::FlexEnd, padding: UiRect::bottom(px(6)), ..default() }, BackgroundColor(Color::srgb(0.3, 0.36, 0.24))))
                .with_children(|stage| {
                    if let Some(hero) = draft.preview() {
                        let frames: Vec<Handle<Image>> = crate::doll::frames(&hero).into_iter().map(|frame| images.add(frame.into_image())).collect();
                        let first = frames[crate::doll::frame_index(draft.facing, 0)].clone();
                        stage.spawn((Preview(frames, draft.facing), Node { width: px(64), height: px(96), ..default() }, ImageNode::new(first)));
                    }
                });
            left.spawn(Node { column_gap: px(3), ..default() }).with_children(|line| {
                button(line, art, fonts, T::LookTurn, true, Action::Rotate);
                button(line, art, fonts, T::LookRandom, true, Action::Random);
            });
            finish(left, draft, art, fonts, session, game);
        });
        columns.spawn(Node { flex_grow: 1.0, flex_direction: FlexDirection::Column, row_gap: px(1), ..default() }).with_children(|right| {
            for feature in Feature::ALL {
                let value = feature.slot(&mut draft.look.clone()).to_owned();
                right.spawn(Node { column_gap: px(3), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                    line.spawn((Node { width: px(78), flex_shrink: 0.0, ..default() }, Text::new(feature.label().text(lang).to_string()), TextLayout::no_wrap(), fonts.body(), ink));
                    text_button(line, art, fonts, "<".to_string(), Action::Turn(feature, -1));
                    line.spawn(Node { width: px(78), flex_shrink: 0.0, column_gap: px(3), align_items: AlignItems::Center, ..default() }).with_children(|cell| {
                        if let Some((r, g, b)) = feature.swatch(value) {
                            cell.spawn((Node { width: px(10), height: px(10), flex_shrink: 0.0, border: UiRect::all(px(1)), ..default() }, BackgroundColor(Color::srgb_u8(r, g, b)), BorderColor::all(color(INK))));
                        }
                        cell.spawn((Text::new(feature.name(value, ru)), TextLayout::no_wrap(), fonts.body(), ink));
                    });
                    text_button(line, art, fonts, ">".to_string(), Action::Turn(feature, 1));
                });
            }
            right.spawn(Node { column_gap: px(3), align_items: AlignItems::Center, ..default() }).with_children(|line| {
                line.spawn((Node { width: px(78), flex_shrink: 0.0, ..default() }, Text::new(T::LookHelmet.text(lang).to_string()), TextLayout::no_wrap(), fonts.body(), ink));
                let mark = if draft.look.helmet { "[x]" } else { "[ ]" };
                text_button(line, art, fonts, mark.to_string(), Action::Helmet);
            });
        });
    });
}

/// The figure in the preview walks on the spot.
fn walk_preview(time: Res<Time>, mut figures: Query<(&Preview, &mut ImageNode)>, mut since: Local<f32>, mut step: Local<usize>) {
    *since += time.delta_secs();
    if *since < 0.14 {
        return;
    }
    *since = 0.0;
    *step = (*step + 1) % crate::doll::STEPS;
    for (preview, mut image) in &mut figures {
        if let Some(frame) = preview.0.get(crate::doll::frame_index(preview.1, *step))
            && image.image != *frame
        {
            image.image = frame.clone();
        }
    }
}

fn clone_of(draft: &Draft) -> Draft {
    Draft {
        race: draft.race,
        class: draft.class,
        method: draft.method,
        base: draft.base,
        rolled: draft.rolled,
        skills: draft.skills.clone(),
        spells: draft.spells.clone(),
        picked: draft.picked,
        problem: draft.problem.clone(),
        look: draft.look,
        tab: draft.tab,
        facing: draft.facing,
    }
}

#[allow(clippy::too_many_arguments)]
fn on_activate(
    used: On<Activate>,
    buttons: Query<&Action>,
    draft: Option<ResMut<Draft>>,
    game: Option<ResMut<Game>>,
    lang: Res<Lang>,
    name: Query<&EditableText, (With<NameInput>, Without<StoryInput>)>,
    story: Query<&EditableText, (With<StoryInput>, Without<NameInput>)>,
    mut next: ResMut<NextState<Screen>>,
    session: Res<Session>,
    settings: Res<Settings>,
) {
    let (Ok(action), Some(mut draft)) = (buttons.get(used.entity), draft) else { return };
    draft.problem = None;
    let data = srd();
    match action {
        Action::Race => {
            draft.race = (draft.race + 1) % data.races.len();
            // Another people changes the scores, and with them what may be cast.
            draft.spells = draft.usual_spells();
        }
        Action::Spell(spell) => draft.toggle_spell(spell),
        Action::Class => {
            draft.class = (draft.class + 1) % data.classes.len();
            draft.skills.clear();
            draft.deal();
        }
        Action::Reroll => {
            draft.rolled = roll_scores(&mut Rng::from_clock());
            draft.deal();
        }
        Action::Show(tab) => draft.tab = *tab,
        Action::Turn(feature, by) => draft.turn(*feature, *by),
        Action::Helmet => draft.look.helmet = !draft.look.helmet,
        Action::Random => draft.randomize(&mut Rng::from_clock()),
        Action::Rotate => {
            use crate::doll::Facing;
            draft.facing = match draft.facing {
                Facing::Down => Facing::Left,
                Facing::Left => Facing::Up,
                Facing::Up => Facing::Right,
                Facing::Right => Facing::Down,
            };
        }
        Action::Minus(i) => draft.nudge(*i, -1),
        Action::Plus(i) => draft.nudge(*i, 1),
        Action::Pick(i) => draft.pick(*i),
        Action::Skill(skill) => draft.toggle_skill(skill),
        Action::Lead => {
            if let Some(mut game) = game {
                session.intend(Intent::Gm(GmAct::Seat(true)));
                game.world.gm = Some(owner_name(&session, &settings));
                next.set(Screen::Play);
            }
        }
        Action::Take(hero) => {
            if let Some(mut game) = game {
                // The host decides; the local copy is changed too so the guest can walk on
                // before the host's world comes back.
                session.send(Up::Claim(*hero));
                game.world.claim(*hero, &owner_name(&session, &settings));
                next.set(Screen::Play);
            }
        }
        Action::Create => {
            let name = name.single().map(|n| n.value().to_string()).unwrap_or_default();
            let description = story.single().map(|s| s.value().to_string()).unwrap_or_default();
            match (draft.build(&name, &description), game) {
                (Ok(hero), Some(mut game)) => {
                    // A guest's hero is the host's to add; it is put in the local copy too
                    // so the guest can start walking before the host's world comes back.
                    session.send(Up::Hero(Box::new(hero.clone())));
                    let world = &mut game.world;
                    let length = world.party.len();
                    world.owners.resize(length, String::new());
                    let owner = owner_name(&session, &settings);
                    // A player whose hero died lets go of it.
                    if !owner.is_empty()
                        && let Some(old) = world.owners.iter().position(|o| *o == owner)
                        && world.party[old].dead
                    {
                        world.owners[old] = String::new();
                    }
                    world.party.push(hero);
                    world.owners.push(owner);
                    next.set(Screen::Play);
                }
                (Err(_), _) if name.trim().is_empty() => draft.problem = Some(T::NeedName.text(*lang).to_string()),
                (Err(_), _) => draft.problem = Some(format!("{} {}", T::NeedSkills.text(*lang), data.classes[draft.class].skill_picks)),
                (Ok(_), None) => {}
            }
        }
    }
}

/// The sheet of a character, line by line, for the sheet shown during play.
pub fn sheet_lines(hero: &Character, lang: Lang) -> Vec<String> {
    let data = srd();
    let ru = lang == Lang::Ru;
    let name = |index: &str, names: &dyn Fn(&str) -> Option<&'static str>| names(index).map_or(index.to_string(), |n| tr(ru, n).to_string());
    let race = name(&hero.race, &|i| data.race(i).map(|r| r.name.as_str()));
    let class = name(&hero.class, &|i| data.class(i).map(|c| c.name.as_str()));
    // A character of several classes names each with its levels.
    let split = hero.class_levels();
    let class = if split.len() > 1 { split.iter().map(|(c, levels)| format!("{} {levels}", tr(ru, &c.name))).collect::<Vec<_>>().join(" / ") } else { class };
    let mut lines = vec![format!("{} — {race}, {class}, {} {}", hero.name, T::Level.text(lang), hero.level)];
    let next = hero.xp_for_next_level().map_or(String::new(), |n| format!(" / {n}"));
    lines.push(format!("{} {}/{} · {} {} · {} {}{next}", T::HitPoints.text(lang), hero.hp.max(0), hero.max_hp, T::ArmorClass.text(lang), hero.ac(), T::Experience.text(lang), hero.xp));
    lines.push(Ability::ALL.map(|a| format!("{} {} ({})", tr(ru, a.name()), hero.scores[a as usize], signed(hero.modifier(a)))).join(" · "));
    let skills: Vec<&str> = hero.skills.iter().map(|s| data.skill(s).map_or(s.as_str(), |k| tr(ru, &k.name))).collect();
    lines.push(format!("{} {}", T::HeroSkills.text(lang), skills.join(", ")));
    let weapons: Vec<String> = hero.weapons.iter().map(|w| format!("{} {} {}", name(w, &|i| data.weapons.get(i).map(|x| x.name.as_str())), signed(hero.attack_bonus(w)), hero.damage(w))).collect();
    lines.push(format!("{} {}", T::Weapons.text(lang), weapons.join(", ")));
    if !hero.spells.is_empty() {
        let spells: Vec<&str> = hero.spells.iter().filter_map(|s| data.spells.get(s)).map(|s| tr(ru, &s.name)).collect();
        let slots: Vec<String> = (1..=9u8).filter(|l| hero.slots()[(*l - 1) as usize] > 0).map(|l| format!("{l}: {}/{}", hero.slots_left(l), hero.slots()[(l - 1) as usize])).collect();
        lines.push(format!("{} {}", T::Spells.text(lang), spells.join(", ")));
        if !slots.is_empty() {
            lines.push(format!("{} {}", T::Slots.text(lang), slots.join(" · ")));
        }
    }
    lines.push(format!("{} {}/{}", T::HitDice.text(lang), hero.hit_dice_left(), hero.level));
    for feat in &hero.feats {
        lines.push(format!("{} {}", T::Feats.text(lang), feat_line(feat, lang)));
    }
    if hero.points_to_place > 0 {
        lines.push(format!("{} {}", T::PointsToPlace.text(lang), hero.points_to_place));
    }
    lines
}

/// An ability from outside the book in one line: its name, what it does in numbers, and
/// how it reads.
pub fn feat_line(feat: &rules::Feat, lang: Lang) -> String {
    let mut numbers = Vec::new();
    if !feat.dice.is_empty() {
        let what = if feat.heals { T::FightHeals.text(lang) } else { T::FightDamage.text(lang) };
        let often = if feat.uses == 0 { T::AtWill.text(lang).to_string() } else { format!("{}/{} {}", feat.uses.saturating_sub(feat.used), feat.uses, T::PerRest.text(lang)) };
        numbers.push(format!("{what} {}, {often}", feat.dice));
    }
    if feat.ac != 0 {
        numbers.push(format!("{} +{}", T::ArmorClass.text(lang), feat.ac));
    }
    if feat.hp != 0 {
        numbers.push(format!("{} +{}", T::HitPoints.text(lang), feat.hp));
    }
    if feat.speed != 0 {
        numbers.push(format!("{} +{}", T::Movement.text(lang), feat.speed / 5));
    }
    let mark = if feat.secret { "★ " } else { "" };
    format!("{mark}{} ({}) — {}", feat.name, numbers.join("; "), feat.description)
}

pub struct HeroPlugin;

impl Plugin for HeroPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::Hero), spawn_screen).add_systems(Update, (redraw.run_if(resource_exists::<Draft>), walk_preview).run_if(in_state(Screen::Hero))).add_observer(on_activate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_draft_spends_the_whole_budget_and_suits_its_class() {
        let d = Draft::default();
        assert_eq!(d.points_left(), 0);
        assert_eq!(d.base[Ability::Str as usize], 15);
        assert_eq!(d.class_index(), "fighter");
    }

    #[test]
    fn point_buy_cannot_overspend_or_leave_the_range() {
        let mut d = Draft::default();
        d.nudge(Ability::Str as usize, 1);
        assert_eq!(d.base[Ability::Str as usize], 15, "15 is the most that can be bought");
        d.nudge(Ability::Int as usize, -1);
        assert_eq!(d.base[Ability::Int as usize], 8, "8 is the least");
        d.nudge(Ability::Str as usize, -1);
        assert_eq!((d.base[Ability::Str as usize], d.points_left()), (14, 2));
        d.nudge(Ability::Int as usize, 1);
        d.nudge(Ability::Int as usize, 1);
        d.nudge(Ability::Int as usize, 1);
        assert_eq!((d.base[Ability::Int as usize], d.points_left()), (10, 0), "the third point was not there to spend");
    }

    #[test]
    fn two_picks_swap_two_scores_and_picking_the_same_twice_cancels() {
        let mut d = Draft { method: AbilityMethod::StandardArray, ..Draft::default() };
        d.deal();
        let (str_before, int_before) = (d.base[0], d.base[3]);
        d.pick(0);
        d.pick(3);
        assert_eq!((d.base[0], d.base[3]), (int_before, str_before));
        d.pick(1);
        d.pick(1);
        assert_eq!(d.picked, None);
        assert_eq!(d.base[1], suggested_scores("fighter", STANDARD_ARRAY)[1]);
    }

    #[test]
    fn skills_are_limited_to_the_classes_number_and_list() {
        let mut d = Draft::default();
        for skill in ["athletics", "arcana", "perception", "survival"] {
            d.toggle_skill(skill);
        }
        assert_eq!(d.skills, ["athletics", "perception"], "arcana is not a fighter skill, and two is the limit");
        d.toggle_skill("athletics");
        assert_eq!(d.skills, ["perception"]);
    }

    #[test]
    fn a_finished_draft_makes_a_character_and_an_unfinished_one_says_why() {
        let mut d = Draft::default();
        assert!(d.build("Brak", "A big man.").is_err(), "no skills chosen yet");
        d.toggle_skill("athletics");
        d.toggle_skill("perception");
        assert!(d.build(" ", "").is_err(), "no name");
        let hero = d.build("Brak", "A big man.").unwrap();
        assert_eq!((hero.name.as_str(), hero.class.as_str(), hero.level), ("Brak", "fighter", 1));
        // Human: +1 to the 15.
        assert_eq!(hero.scores[0], 16);
    }

    #[test]
    fn a_caster_draft_comes_with_spells() {
        let mut d = Draft::default();
        d.class = srd().classes.iter().position(|c| c.index == "wizard").unwrap();
        d.deal();
        d.toggle_skill("arcana");
        d.toggle_skill("history");
        let hero = d.build("Zed", "").unwrap();
        assert_eq!(hero.spells.len(), 7);
    }

    #[test]
    fn the_sheet_names_everything_in_the_players_language() {
        let mut d = Draft::default();
        d.toggle_skill("athletics");
        d.toggle_skill("perception");
        let hero = d.build("Brak", "").unwrap();
        let en = sheet_lines(&hero, Lang::En);
        assert!(en[0].starts_with("Brak — Human, Fighter, level 1"));
        assert!(en[1].contains("12/12") && en[1].contains("18"));
        assert!(en[4].contains("Longsword +5 1d8+3"));
        let ru = sheet_lines(&hero, Lang::Ru);
        assert!(ru[0].contains("Воин") && ru[2].contains("Сила 16 (+3)"), "{ru:?}");
    }
}
