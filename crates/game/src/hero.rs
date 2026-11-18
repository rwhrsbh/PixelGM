//! Making a character for a world that has none yet, and showing the sheet of one that exists.

use crate::coop::{GmAct, Intent, Session, Up, owner_name};
use crate::i18n::{Lang, T};
use crate::menu::Screen;
use crate::play::Game;
use crate::store::{Settings, Store};
use crate::theme::*;
use crate::ui::egui::{self, Sense, vec2};
use crate::ui::kit::{self, Sheet, button, hint};
use crate::ui::theme::c;
use bevy::prelude::*;
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

#[derive(Clone, PartialEq)]
enum Action {
    Race(usize),
    Class(usize),
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

/// What is typed in the two fields of the screen.
#[derive(Resource, Default)]
struct Typed {
    name: String,
    story: String,
}

fn enter(mut commands: Commands, game: Option<Res<Game>>, store: Res<Store>, mut next: ResMut<NextState<Screen>>) {
    let Some(game) = game else {
        next.set(Screen::Main);
        return;
    };
    // Saved at once, so leaving now and choosing Continue comes back to this screen.
    if let Err(e) = store.save_game(&game.world_id, &game.world, false) {
        error!("could not save the game: {e}");
    }
    commands.insert_resource(Draft::with_method(game.world.table.method));
    commands.insert_resource(Typed::default());
}

/// The figure in the preview as egui knows it: the frames of the look shown, made once.
#[derive(Default)]
struct Figure {
    key: String,
    frames: Vec<(egui::TextureId, Vec2)>,
}

/// The hero screen, drawn with egui: the name and the story, people and class, and two
/// tabs, the looks with a figure that walks on the spot and the numbers with the skills.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn hero_screen(
    mut egui: bevy_egui::EguiContexts,
    (mut draft, mut typed, game, session, settings, lang, time): (ResMut<Draft>, ResMut<Typed>, Option<ResMut<Game>>, Res<Session>, Res<Settings>, Res<Lang>, Res<Time>),
    mut next: ResMut<NextState<Screen>>,
    mut images: ResMut<Assets<Image>>,
    mut figure: Local<Figure>,
    mut clock: Local<f32>,
) {
    let lang = *lang;
    let ru = lang == Lang::Ru;
    let t = |text: T| text.text(lang);
    let data = srd();
    // The figure's frames, made again only when the looks change.
    let shown = draft.preview();
    let key = shown.as_ref().map(crate::doll::key).unwrap_or_default();
    if figure.key != key {
        figure.key = key;
        figure.frames = shown
            .as_ref()
            .map(|hero| {
                crate::doll::frames(hero)
                    .into_iter()
                    .map(|frame| {
                        let size = Vec2::new(frame.w as f32, frame.h as f32);
                        (egui.add_image(bevy_egui::EguiTextureHandle::Strong(images.add(frame.into_image()))), size)
                    })
                    .collect()
            })
            .unwrap_or_default();
    }
    *clock += time.delta_secs();
    let step = (*clock / 0.14) as usize % crate::doll::STEPS;
    // The one chosen in the lobby to run the game makes no hero: straight to the table.
    let me = crate::coop::player_name(&settings);
    if game.as_deref().is_some_and(|g| g.world.gm.as_deref() == Some(me.as_str())) {
        next.set(Screen::Play);
        return;
    }
    let Ok(ctx) = egui.ctx_mut() else { return };
    let Some(mut ui) = kit::screen_ui(ctx, "hero") else { return };
    let mut acts: Vec<Action> = Vec::new();
    let mut leave = false;
    let mut create = false;
    let (race, class) = (&data.races[draft.race], &data.classes[draft.class]);
    let world = game.as_deref().map(|g| &g.world);
    let shared = !matches!(*session, Session::Solo);
    let seat_free = world.is_some_and(|w| w.gm.as_deref().is_none_or(|gm| !w.present.iter().any(|p| p == gm)));
    let free: Vec<(usize, String)> = match world {
        Some(w) if shared => (0..w.party.len())
            .filter(|i| w.is_away(*i) && !w.party[*i].dead)
            .map(|i| {
                let hero = &w.party[i];
                let class = srd().class(&hero.class).map_or(hero.class.as_str(), |c| tr(ru, &c.name));
                (i, format!("{} ({class} {})", hero.name, hero.level))
            })
            .collect(),
        _ => Vec::new(),
    };
    let problem = draft.problem.clone().unwrap_or_default();
    let sheet = Sheet { id: "hero", title: t(T::HeroTitle), about: t(T::HeroAbout), max: vec2(480.0, 470.0), top: 0.0, fixed: true };
    kit::sheet(
        &mut ui,
        sheet,
        |ui| {
            // At a shared table somebody may run the game instead of playing in it, and the
            // heroes of players who are away can be taken over.
            if shared && seat_free && ui.add(button(t(T::SeatNoHero))).clicked() {
                acts.push(Action::Lead);
            }
            if !free.is_empty() {
                ui.horizontal_wrapped(|row| {
                    hint(row, t(T::FreeHeroes));
                    for (i, label) in &free {
                        if row.add(button(label.as_str())).clicked() {
                            acts.push(Action::Take(*i));
                        }
                    }
                });
            }
            ui.horizontal(|row| {
                row.label(t(T::HeroName));
                let field = row.add(egui::TextEdit::singleline(&mut typed.name).desired_width(150.0).char_limit(30));
                if row.memory(|m| m.focused().is_none()) {
                    field.request_focus();
                }
                row.label(t(T::HeroStory));
                row.add(egui::TextEdit::singleline(&mut typed.story).desired_width(f32::INFINITY).char_limit(300));
            });
            let races: Vec<&str> = data.races.iter().map(|r| tr(ru, &r.name)).collect();
            let mut at = draft.race;
            if kit::choice(ui, t(T::HeroRace).trim_end_matches(':'), &races, &mut at).changed() {
                acts.push(Action::Race(at));
            }
            let classes: Vec<&str> = data.classes.iter().map(|c| tr(ru, &c.name)).collect();
            let mut at = draft.class;
            if kit::choice(ui, t(T::HeroClass).trim_end_matches(':'), &classes, &mut at).changed() {
                acts.push(Action::Class(at));
            }
            ui.horizontal(|row| {
                for (tab, label) in [(Tab::Look, T::TabLook), (Tab::Stats, T::TabStats)] {
                    let mut b = button(t(label));
                    if draft.tab == tab {
                        b = b.primary();
                    }
                    if row.add(b).clicked() {
                        acts.push(Action::Show(tab));
                    }
                }
                // The host chose the method for the whole world.
                hint(row, method_label(draft.method).text(lang));
                if draft.method == AbilityMethod::Roll && draft.tab == Tab::Stats && row.add(button(t(T::Reroll))).clicked() {
                    acts.push(Action::Reroll);
                }
            });
            ui.add_space(2.0);
            if draft.tab == Tab::Look {
                {
                    // The figure at the left in a narrow column, the looks at the right in
                    // the rest: their names are long.
                    let area = ui.available_rect_before_wrap();
                    let column = |rect: egui::Rect| egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min));
                    let mut left = ui.new_child(column(egui::Rect::from_min_size(area.min, vec2(150.0, area.height()))));
                    let mut right = ui.new_child(column(egui::Rect::from_min_max(egui::pos2(area.left() + 160.0, area.top()), area.max)));
                    let (left, right) = (&mut left, &mut right);
                    // The figure at four times its size in the world, on a patch of ground.
                    let (stage, _) = left.allocate_exact_size(vec2(140.0, 120.0), Sense::hover());
                    left.painter().rect_filled(stage, 0.0, egui::Color32::from_rgb(0x4c, 0x5c, 0x3d));
                    if let Some((texture, size)) = figure.frames.get(crate::doll::frame_index(draft.facing, step)) {
                        let scale = ((stage.height() - 8.0) / size.y).min(stage.width() / size.x).floor().max(1.0);
                        let place = egui::Rect::from_center_size(stage.center(), vec2(size.x * scale, size.y * scale));
                        left.painter().image(*texture, place, egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)), egui::Color32::WHITE);
                    }
                    left.horizontal(|row| {
                        if row.add(button(t(T::LookTurn))).clicked() {
                            acts.push(Action::Rotate);
                        }
                        if row.add(button(t(T::LookRandom))).clicked() {
                            acts.push(Action::Random);
                        }
                    });
                    for feature in Feature::ALL {
                        let value = *feature.slot(&mut draft.look.clone());
                        let names: Vec<String> = (0..feature.count()).map(|v| feature.name(v as u8, ru)).collect();
                        let names: Vec<&str> = names.iter().map(String::as_str).collect();
                        let mut at = value as usize;
                        let row = kit::choice(right, feature.label().text(lang), &names, &mut at);
                        if row.changed() {
                            acts.push(Action::Turn(feature, at as i32 - value as i32));
                        }
                        // A colour shows as a swatch at the left of its value.
                        if let Some((r, g, b)) = feature.swatch(value) {
                            let x = row.rect.right() - kit::ROW - 2.0 - kit::VALUE_COLUMN + 6.0;
                            let swatch = egui::Rect::from_center_size(egui::pos2(x + 4.0, row.rect.center().y), vec2(8.0, 8.0));
                            right.painter().rect_filled(swatch, 0.0, egui::Color32::from_rgb(r, g, b));
                            right.painter().rect_stroke(swatch, 0.0, egui::Stroke::new(1.0, c(INK)), egui::StrokeKind::Outside);
                        }
                    }
                    let mut helmet = draft.look.helmet;
                    if kit::toggle(right, t(T::LookHelmet), &mut helmet).changed() {
                        acts.push(Action::Helmet);
                    }
                    let tall = left.min_rect().height().max(right.min_rect().height());
                    ui.allocate_space(vec2(area.width(), tall));
                }
            } else {
                ui.columns(2, |cols| {
                    let left = &mut cols[0];
                    hint(left, match draft.method {
                        AbilityMethod::PointBuy => format!("{} {}", t(T::PointsLeft), draft.points_left()),
                        _ => t(T::SwapHint).to_string(),
                    });
                    for ability in Ability::ALL {
                        let i = ability as usize;
                        let total = draft.base[i] + race.bonuses[i] + class.background[i];
                        left.horizontal(|row| {
                            let picked = draft.picked == Some(i);
                            let (r, _) = row.allocate_exact_size(vec2(84.0, kit::ROW), Sense::hover());
                            row.painter().text(r.left_center(), egui::Align2::LEFT_CENTER, tr(ru, ability.name()), egui::FontId::proportional(crate::ui::theme::BODY), c(if picked { WAX } else { INK }));
                            let (r, _) = row.allocate_exact_size(vec2(46.0, kit::ROW), Sense::hover());
                            row.painter().text(r.left_center(), egui::Align2::LEFT_CENTER, format!("{total} ({})", signed(modifier(total))), egui::FontId::proportional(crate::ui::theme::BODY), c(INK));
                            if draft.method == AbilityMethod::PointBuy {
                                if row.add(button("−")).clicked() {
                                    acts.push(Action::Minus(i));
                                }
                                if row.add(button("+")).clicked() {
                                    acts.push(Action::Plus(i));
                                }
                            } else {
                                let mut swap = button(t(T::Swap));
                                if picked {
                                    swap = swap.primary();
                                }
                                if row.add(swap).clicked() {
                                    acts.push(Action::Pick(i));
                                }
                            }
                        });
                    }
                    // What the choices come to, so the player sees the character before making it.
                    if let Some(hero) = &shown {
                        left.label(format!("{} {} · {} {}", t(T::HitPoints), hero.max_hp, t(T::ArmorClass), hero.ac()));
                    }
                    let right = &mut cols[1];
                    hint(right, format!("{} {}/{}", t(T::HeroSkills), draft.skills.len(), class.skill_picks));
                    let caster = draft.caster().filter(|hero| hero.cantrips_allowed() + hero.spells_allowed() > 0);
                    egui::ScrollArea::vertical().id_salt("skills").max_height(if caster.is_some() { 90.0 } else { 200.0 }).show(right, |ui| {
                        for skill in &class.skill_options {
                            let name = data.skill(skill).map_or(skill.as_str(), |s| tr(ru, &s.name));
                            let mut on = draft.skills.contains(skill);
                            if kit::toggle(ui, name, &mut on).changed() {
                                acts.push(Action::Skill(skill.clone()));
                            }
                        }
                    });
                    if let Some(hero) = &caster {
                        let chosen = |cantrips: bool| draft.spells.iter().filter_map(|s| data.spells.get(s)).filter(|s| (s.level == 0) == cantrips).count();
                        hint(right, format!("{} {}/{} · {} {}/{}", t(T::Cantrips), chosen(true), hero.cantrips_allowed(), t(T::Spells), chosen(false), hero.spells_allowed()));
                        egui::ScrollArea::vertical().id_salt("spells").max_height(90.0).show(right, |ui| {
                            let mut pool = data.class_spells(&hero.class, hero.highest_slot());
                            pool.sort_by_key(|s| (s.level, tr(ru, &s.name).to_string()));
                            for spell in pool {
                                // Spells the rules cannot carry out by themselves go through the master.
                                let kind = if spell.mechanised() { "" } else { " *" };
                                let mut on = draft.spells.contains(&spell.index);
                                if kit::toggle(ui, &format!("{} {}{kind}", spell.level, tr(ru, &spell.name)), &mut on).changed() {
                                    acts.push(Action::Spell(spell.index.clone()));
                                }
                            }
                        });
                    }
                });
            }
        },
        |footer| {
            if footer.add(button(t(T::Back)).key("Esc")).clicked() {
                leave = true;
            }
            kit::right(footer, |r| {
                if r.add(button(t(T::CreateHero)).primary()).clicked() {
                    create = true;
                }
                // What is wrong with the hero, beside the button that would make it.
                if !problem.is_empty() {
                    kit::warning(r, problem.as_str());
                }
            });
        },
    );
    if leave {
        next.set(Screen::Main);
    }
    if create {
        acts.push(Action::Create);
    }
    let mut game = game;
    for act in acts {
        apply(act, &mut draft, &typed, game.as_deref_mut(), lang, &mut next, &session, &settings);
    }
}

#[allow(clippy::too_many_arguments)]
fn apply(action: Action, draft: &mut Draft, typed: &Typed, game: Option<&mut Game>, lang: Lang, next: &mut NextState<Screen>, session: &Session, settings: &Settings) {
    draft.problem = None;
    let data = srd();
    match action {
        Action::Race(at) => {
            draft.race = at % data.races.len();
            // Another people changes the scores, and with them what may be cast.
            draft.spells = draft.usual_spells();
        }
        Action::Spell(spell) => draft.toggle_spell(&spell),
        Action::Class(at) => {
            draft.class = at % data.classes.len();
            draft.skills.clear();
            draft.deal();
        }
        Action::Reroll => {
            draft.rolled = roll_scores(&mut Rng::from_clock());
            draft.deal();
        }
        Action::Show(tab) => draft.tab = tab,
        Action::Turn(feature, by) => draft.turn(feature, by),
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
        Action::Minus(i) => draft.nudge(i, -1),
        Action::Plus(i) => draft.nudge(i, 1),
        Action::Pick(i) => draft.pick(i),
        Action::Skill(skill) => draft.toggle_skill(&skill),
        Action::Lead => {
            if let Some(game) = game {
                session.intend(Intent::Gm(GmAct::Seat(true)));
                game.world.gm = Some(owner_name(session, settings));
                next.set(Screen::Play);
            }
        }
        Action::Take(hero) => {
            if let Some(game) = game {
                // The host decides; the local copy is changed too so the guest can walk on
                // before the host's world comes back.
                session.send(Up::Claim(hero));
                game.world.claim(hero, &owner_name(session, settings));
                next.set(Screen::Play);
            }
        }
        Action::Create => {
            let name = typed.name.trim().to_string();
            match (draft.build(&name, typed.story.trim()), game) {
                (Ok(hero), Some(game)) => {
                    // A guest's hero is the host's to add; it is put in the local copy too
                    // so the guest can start walking before the host's world comes back.
                    session.send(Up::Hero(Box::new(hero.clone())));
                    let world = &mut game.world;
                    let length = world.party.len();
                    world.owners.resize(length, String::new());
                    let owner = owner_name(session, settings);
                    // A player whose hero died lets go of it.
                    if !owner.is_empty()
                        && let Some(old) = world.owners.iter().position(|o| *o == owner)
                        && world.party[old].dead
                    {
                        world.owners[old] = String::new();
                    }
                    world.party.push(hero);
                    world.outfit(world.party.len() - 1);
                    world.owners.push(owner);
                    world.tidy_gear();
                    next.set(Screen::Play);
                }
                (Err(_), _) if name.is_empty() => draft.problem = Some(T::NeedName.text(lang).to_string()),
                (Err(_), _) => {
                    // The skills are on the other tab: shown, so that what is missing is seen.
                    draft.tab = Tab::Stats;
                    draft.problem = Some(format!("{} {}", T::NeedSkills.text(lang), data.classes[draft.class].skill_picks));
                }
                (Ok(_), None) => {}
            }
        }
    }
}

/// The sheet of a character, line by line, for the sheet shown during play. A line that
/// begins with "# " is a heading, one with "~ " a note on what the numbers mean.
pub fn sheet_lines(hero: &Character, lang: Lang) -> Vec<String> {
    let data = srd();
    let ru = lang == Lang::Ru;
    let t = |key: T| key.text(lang);
    let name = |index: &str, names: &dyn Fn(&str) -> Option<&'static str>| names(index).map_or(index.to_string(), |n| tr(ru, n).to_string());
    let race = name(&hero.race, &|i| data.race(i).map(|r| r.name.as_str()));
    let class = name(&hero.class, &|i| data.class(i).map(|c| c.name.as_str()));
    // A character of several classes names each with its levels.
    let split = hero.class_levels();
    let class = if split.len() > 1 { split.iter().map(|(c, levels)| format!("{} {levels}", tr(ru, &c.name))).collect::<Vec<_>>().join(" / ") } else { class };
    let mut lines = vec![format!("# {} — {race}, {class}, {} {}", hero.name, t(T::Level), hero.level)];
    lines.push(match hero.xp_for_next_level() {
        Some(next) => format!("{} {} / {next} · {} {}: {}", t(T::Experience), hero.xp, t(T::SheetToNext), hero.level + 1, next.saturating_sub(hero.xp)),
        None => format!("{} {} · {}", t(T::Experience), hero.xp, t(T::SheetTop)),
    });
    if hero.points_to_place > 0 {
        lines.push(format!("{} {}", t(T::PointsToPlace), hero.points_to_place));
    }

    lines.push(format!("# {}", t(T::SheetBody)));
    lines.push(format!("{} {}/{} · {} {} · {} {} {} · {} {}", t(T::HitPoints), hero.hp.max(0), hero.max_hp, t(T::ArmorClass), hero.ac(), t(T::SheetSpeed), hero.speed(), t(T::SheetFeet), t(T::SheetProficiency), signed(hero.proficiency())));
    lines.push(format!("{} {}/{}", t(T::HitDice), hero.hit_dice_left(), hero.level));
    lines.push(format!("~ {}", t(T::SheetBodyHint)));

    lines.push(format!("# {}", t(T::SheetAbilities)));
    for half in Ability::ALL.chunks(3) {
        lines.push(half.iter().map(|a| format!("{} {} ({})", tr(ru, a.name()), hero.scores[*a as usize], signed(hero.modifier(*a)))).collect::<Vec<_>>().join(" · "));
    }

    lines.push(format!("# {}", t(T::SheetSkills)));
    let skills: Vec<String> = hero
        .skills
        .iter()
        .map(|s| match data.skill(s) {
            Some(skill) => format!("{} {}", tr(ru, &skill.name), signed(hero.check_bonus(skill.ability, Some(s)))),
            None => s.clone(),
        })
        .collect();
    lines.push(if skills.is_empty() { t(T::SheetNoSkills).to_string() } else { skills.join(" · ") });

    if !hero.weapons.is_empty() {
        lines.push(format!("# {}", t(T::SheetWeapons)));
        for w in &hero.weapons {
            lines.push(format!("{} — {} {}, {} {}", name(w, &|i| data.weapons.get(i).map(|x| x.name.as_str())), t(T::SheetAttack), signed(hero.attack_bonus(w)), t(T::SheetDamage), hero.damage(w)));
        }
    }
    if !hero.spells.is_empty() {
        lines.push(format!("# {}", t(T::SheetSpells)));
        let spells: Vec<&str> = hero.spells.iter().filter_map(|s| data.spells.get(s)).map(|s| tr(ru, &s.name)).collect();
        lines.push(spells.join(", "));
        let slots: Vec<String> = (1..=9u8).filter(|l| hero.slots()[(*l - 1) as usize] > 0).map(|l| format!("{l}: {}/{}", hero.slots_left(l), hero.slots()[(l - 1) as usize])).collect();
        if !slots.is_empty() {
            lines.push(format!("{} {}", t(T::Slots), slots.join(" · ")));
        }
    }
    if !hero.feats.is_empty() {
        lines.push(format!("# {}", t(T::SheetSpecial)));
        for feat in &hero.feats {
            lines.push(feat_line(feat, lang));
        }
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
        app.add_systems(OnEnter(Screen::Hero), enter).add_systems(bevy_egui::EguiPrimaryContextPass, hero_screen.in_set(crate::ui::UiSet::Screens).run_if(in_state(Screen::Hero)).run_if(resource_exists::<Draft>));
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
        assert!(en[0].starts_with("# Brak — Human, Fighter, level 1"));
        assert!(en[1].contains("0 / 300") && en[1].contains("to level 2: 300"), "{en:?}");
        assert!(en.iter().any(|l| l.contains("12/12") && l.contains("AC 18") && l.contains("+2")), "{en:?}");
        assert!(en.iter().any(|l| l == "Longsword — attack +5, damage 1d8+3"), "{en:?}");
        assert!(en.iter().any(|l| l.contains("Athletics +5") && l.contains("Perception")), "{en:?}");
        let ru = sheet_lines(&hero, Lang::Ru);
        assert!(ru[0].contains("Воин") && ru.iter().any(|l| l.starts_with("Сила 16 (+3)")), "{ru:?}");
    }
}
