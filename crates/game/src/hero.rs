//! Making a character for a world that has none yet, and showing the sheet of one that exists.

use crate::art::Art;
use crate::coop::{Session, Up, owner_name};
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
    /// The ability picked first of two to swap, under methods that deal fixed numbers.
    picked: Option<usize>,
    problem: Option<String>,
}

impl Default for Draft {
    fn default() -> Self {
        let class = srd().classes.iter().position(|c| c.index == "fighter").unwrap_or(0);
        Self { race: srd().races.iter().position(|r| r.index == "human").unwrap_or(0), class, method: AbilityMethod::PointBuy, base: suggested_scores("fighter", STANDARD_ARRAY), rolled: STANDARD_ARRAY, skills: Vec::new(), picked: None, problem: None }
    }
}

impl Draft {
    fn class_index(&self) -> &'static str {
        &srd().classes[self.class].index
    }

    /// Deals the scores again for the current class and method.
    fn deal(&mut self) {
        let values = if self.method == AbilityMethod::Roll { self.rolled } else { STANDARD_ARRAY };
        self.base = suggested_scores(self.class_index(), values);
        self.picked = None;
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
        let spells = suggested_spells(data, &hero);
        hero.set_spells(&spells)?;
        Ok(hero)
    }
}

#[derive(Component, Clone, PartialEq)]
enum Action {
    Race,
    Class,
    Method,
    Minus(usize),
    Plus(usize),
    Pick(usize),
    Reroll,
    Skill(String),
    Create,
    /// Take the hero of a player who is away instead of making a new one.
    Take(usize),
}

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
    commands.insert_resource(Draft::default());
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
fn redraw(mut commands: Commands, draft: Res<Draft>, lang: Res<Lang>, art: Res<Art>, fonts: Res<Fonts>, choices: Query<Entity, With<Choices>>, fresh: Query<(), Added<Choices>>) {
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
            text_button(line, &art, &fonts, method_label(draft.method).text(*lang).to_string(), Action::Method);
            if draft.method == AbilityMethod::Roll {
                button(line, &art, &fonts, T::Reroll, true, Action::Reroll);
            }
        });
        area.spawn(Node { column_gap: px(10), align_items: AlignItems::FlexStart, ..default() }).with_children(|columns| {
            columns.spawn(Node { width: px(214), flex_shrink: 0.0, flex_direction: FlexDirection::Column, row_gap: px(1), ..default() }).with_children(|left| {
                let hint = match draft.method {
                    AbilityMethod::PointBuy => format!("{} {}", T::PointsLeft.text(*lang), draft.points_left()),
                    _ => T::SwapHint.text(*lang).to_string(),
                };
                left.spawn((Text::new(hint), fonts.body(), faded));
                for ability in Ability::ALL {
                    let i = ability as usize;
                    let total = draft.base[i] + race.bonuses[i];
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
                right.spawn(scroll_list(Node { flex_direction: FlexDirection::Column, row_gap: px(1), ..default() }, 4.0 * (BUTTON_HEIGHT + 1.0))).with_children(|rows| {
                    for skill in &class.skill_options {
                        let name = data.skill(skill).map_or(skill.as_str(), |s| tr(ru, &s.name));
                        let mark = if draft.skills.contains(skill) { "[x]" } else { "[ ]" };
                        text_button(rows, &art, &fonts, format!("{mark} {name}"), Action::Skill(skill.clone()));
                    }
                });
                // What the choices come to, so the player sees the character before making it.
                let preview = Draft { skills: class.skill_options.iter().take(class.skill_picks).cloned().collect(), picked: None, problem: None, ..clone_of(&draft) };
                if let Ok(hero) = preview.build("x", "") {
                    let spells: Vec<&str> = hero.spells.iter().filter_map(|s| data.spells.get(s)).map(|s| tr(ru, &s.name)).collect();
                    let mut summary = format!("{} {} · {} {}", T::HitPoints.text(*lang), hero.max_hp, T::ArmorClass.text(*lang), hero.ac());
                    if !spells.is_empty() {
                        summary.push_str(&format!(" · {} {}", T::Spells.text(*lang), spells.join(", ")));
                    }
                    right.spawn((Text::new(summary), fonts.body(), ink));
                }
                if let Some(problem) = &draft.problem {
                    right.spawn((Text::new(problem.clone()), fonts.body(), TextColor(color(WAX))));
                }
                right.spawn(Node { column_gap: px(6), ..default() }).with_children(|line| {
                    button(line, &art, &fonts, T::Back, true, Nav::Go(Screen::Main));
                    button(line, &art, &fonts, T::CreateHero, true, Action::Create);
                });
            });
        });
    });
}

fn clone_of(draft: &Draft) -> Draft {
    Draft { race: draft.race, class: draft.class, method: draft.method, base: draft.base, rolled: draft.rolled, skills: draft.skills.clone(), picked: draft.picked, problem: draft.problem.clone() }
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
        Action::Race => draft.race = (draft.race + 1) % data.races.len(),
        Action::Class => {
            draft.class = (draft.class + 1) % data.classes.len();
            draft.skills.clear();
            draft.deal();
        }
        Action::Method => {
            draft.method = match draft.method {
                AbilityMethod::PointBuy => AbilityMethod::StandardArray,
                AbilityMethod::StandardArray => AbilityMethod::Roll,
                AbilityMethod::Roll => AbilityMethod::PointBuy,
            };
            if draft.method == AbilityMethod::Roll {
                draft.rolled = roll_scores(&mut Rng::from_clock());
            }
            draft.deal();
        }
        Action::Reroll => {
            draft.rolled = roll_scores(&mut Rng::from_clock());
            draft.deal();
        }
        Action::Minus(i) => draft.nudge(*i, -1),
        Action::Plus(i) => draft.nudge(*i, 1),
        Action::Pick(i) => draft.pick(*i),
        Action::Skill(skill) => draft.toggle_skill(skill),
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
                    world.party.push(hero);
                    world.owners.push(owner_name(&session, &settings));
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
    if hero.points_to_place > 0 {
        lines.push(format!("{} {}", T::PointsToPlace.text(lang), hero.points_to_place));
    }
    lines
}

pub struct HeroPlugin;

impl Plugin for HeroPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::Hero), spawn_screen).add_systems(Update, redraw.run_if(in_state(Screen::Hero)).run_if(resource_exists::<Draft>)).add_observer(on_activate);
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
