//! The master's workshop, in the manner of the map editors of old role-playing makers: the
//! map is painted with tiles (a brush, a rectangle), creatures and objects are put on a cell
//! with a click and moved by dragging them, and what is made — tiles, creatures, objects —
//! is drawn pixel by pixel or by the model, set up with everything the game lets it be, and
//! kept in the library of everyone at the table.

use super::*;
use crate::ui::egui::{self, Rect, Sense, Ui, vec2};
use crate::ui::kit::{self, button, hint};
use crate::ui::theme::c;
use bevy_egui::EguiContexts;
use std::collections::HashMap;
use world::library::{Art, Entity};

/// The size of a drawing, and of a cell of it on the screen.
const SIDE: usize = 16;
const PIXEL: f32 = 11.0;

/// What the master's screen is for at the moment: running the game, or one of the editor's
/// rooms.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Mode {
    #[default]
    Run,
    Map,
    Make,
    Library,
    Rules,
}

/// What the pen does on the drawing.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum Pen {
    #[default]
    Pencil,
    Eraser,
    Fill,
    Picker,
}

/// What a click on the map does.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum MapTool {
    #[default]
    Brush,
    Rect,
    Erase,
    Place,
    Move,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum MakeKind {
    #[default]
    Creature,
    Object,
    Tile,
    /// A thing for a hero's bag.
    Item,
}

/// A thing sold, or kept in a chest.
#[derive(Clone, Default, Debug)]
struct Ware {
    name: String,
    kind: String,
    price: u32,
    heal: String,
    damage: String,
}

/// An attack of a creature with numbers of its own.
#[derive(Clone, Debug)]
struct Hit {
    name: String,
    bonus: i32,
    damage: String,
    range: u32,
}

impl Default for Hit {
    fn default() -> Self {
        Self { name: String::new(), bonus: 3, damage: "1d6+1".into(), range: 5 }
    }
}

/// Kinds a thing can be, as the game knows them.
const KINDS: [&str; 10] = ["weapon", "armor", "light", "potion", "food", "throwable", "key", "tool", "junk", "quest"];

/// A cell of the map the master clicked, with what covers it.
#[derive(Clone, Debug)]
pub struct Focus {
    pub cell: (usize, usize),
    pub id: Option<String>,
    pub at: egui::Pos2,
}

#[derive(Resource)]
pub struct Workshop {
    pub mode: Mode,
    pen: Pen,
    /// The library shown: 0 everything, 1 creatures, 2 objects, 3 tiles.
    filter: usize,
    /// What the cell under the mouse is, for the line at the bottom.
    hovered: String,
    /// Where the editor's panels lie on the screen: clicks there are not the map's.
    panels: Vec<Rect>,
    /// The card chosen in the library: a creature or object (true) or a picture, by place.
    picked: Option<(bool, usize)>,
    /// The shelf of the library shown, by its source, and what it holds.
    source: usize,
    shelf: Option<(Vec<Art>, Vec<Entity>)>,
    /// The worlds saved on this computer: title, place, file; one each, the newest.
    saves: Vec<(String, String, std::path::PathBuf)>,
    saves_read: bool,
    pub favorites: crate::store::Favorites,
    /// The master's question about the rules, and the ruling that came.
    question: String,
    answer: String,
    asking: bool,
    /// The answer the master writes to what the players did, and the deed it answers.
    reply: TurnDraft,
    reply_to: Option<(String, String)>,
    /// The roll being set up for the answer: ability, skill, difficulty, what each way brings.
    roll: (String, String, i64, String, String),
    /// The change being put together in the form of every effect.
    effect: super::effects_ui::Form,
    /// The page of the running window: 0 tell, 1 change the world, 2 the AI, 3 the seat.
    page: usize,
    /// What the master tells, and to whom ("" is everyone).
    telling: String,
    telling_to: String,
    /// Who speaks a line, and the line.
    speaker: String,
    line: String,
    /// What the AI is told.
    order: String,
    /// What the master clicked on the map: the cell, what covers it, and where on the screen,
    /// for the ring of what can be done with it.
    pub focus: Option<Focus>,
    /// The page of the ring opened: what it does, by effect, or "move".
    menu_page: Option<(&'static str, String)>,
    /// The steps of a script being put together for an action or a plate.
    script: Vec<serde_json::Value>,
    script_form: super::effects_ui::Form,
    /// The form of the page opened from the ring.
    ring_form: super::effects_ui::Form,
    script_words: String,
    script_asking: bool,
    /// A creature or thing to be walked to the next cell clicked.
    carry: Option<String>,
    tool: MapTool,
    tile: String,
    /// The library's creature or object put on a cell by a click, by its place in the list.
    placing: Option<Entity>,
    /// What is being dragged across the map, by its id.
    moving: Option<String>,
    rect_from: Option<(usize, usize)>,
    last_cell: Option<(usize, usize)>,
    kind: MakeKind,
    grid: Vec<u8>,
    /// The drawing's width and height in pixels: a cell is sixteen, a thing for the bag
    /// as many cells as it takes.
    gw: usize,
    gh: usize,
    /// A thing for the bag: its cells across and down, and what it is.
    cells: (usize, usize),
    item_kind: String,
    gear: String,
    heal: String,
    damage: String,
    bonus: i32,
    light: u8,
    price: u32,
    rarity: String,
    /// Who gets it, or what it is put into.
    give_to: String,
    stash_in: String,
    color: u8,
    id: String,
    name: String,
    description: String,
    /// 0 hostile, 1 neutral, 2 friendly.
    attitude: usize,
    merchant: bool,
    wares: Vec<Ware>,
    /// A monster of the book whose numbers it fights with; empty for numbers of its own.
    book: String,
    ac: i32,
    hp: i32,
    speed: u32,
    xp: u32,
    hits: Vec<Hit>,
    count: u32,
    faction: String,
    walkable: bool,
    blocking: bool,
    chest: bool,
    contents: Vec<Ware>,
    lock: i32,
    key: String,
    describe: String,
    arts: Vec<Art>,
    made: Vec<Entity>,
    loaded: bool,
}

impl Default for Workshop {
    fn default() -> Self {
        Self {
            mode: Mode::Run,
            pen: Pen::Pencil,
            filter: 0,
            hovered: String::new(),
            panels: Vec::new(),
            picked: None,
            source: 0,
            shelf: None,
            saves: Vec::new(),
            saves_read: false,
            favorites: Default::default(),
            question: String::new(),
            answer: String::new(),
            asking: false,
            reply: TurnDraft::default(),
            reply_to: None,
            roll: ("dex".into(), String::new(), 12, String::new(), String::new()),
            effect: Default::default(),
            page: 0,
            telling: String::new(),
            telling_to: String::new(),
            speaker: String::new(),
            line: String::new(),
            order: String::new(),
            focus: None,
            menu_page: None,
            script: Vec::new(),
            script_form: Default::default(),
            ring_form: Default::default(),
            script_words: String::new(),
            script_asking: false,
            carry: None,
            tool: MapTool::Brush,
            tile: String::new(),
            placing: None,
            moving: None,
            rect_from: None,
            last_cell: None,
            kind: MakeKind::Creature,
            grid: vec![0; SIDE * SIDE],
            gw: SIDE,
            gh: SIDE,
            cells: (1, 1),
            item_kind: "weapon".into(),
            gear: String::new(),
            heal: String::new(),
            damage: String::new(),
            bonus: 0,
            light: 0,
            price: 10,
            rarity: String::new(),
            give_to: String::new(),
            stash_in: String::new(),
            color: 1,
            id: String::new(),
            name: String::new(),
            description: String::new(),
            attitude: 0,
            merchant: false,
            wares: Vec::new(),
            book: String::new(),
            ac: 12,
            hp: 11,
            speed: 30,
            xp: 50,
            hits: vec![Hit::default()],
            count: 0,
            faction: String::new(),
            walkable: true,
            blocking: true,
            chest: false,
            contents: Vec::new(),
            lock: 0,
            key: String::new(),
            describe: String::new(),
            arts: Vec::new(),
            made: Vec::new(),
            loaded: false,
        }
    }
}

impl Workshop {
    /// The drawing as rows of the world's palette characters.
    fn rows(&self) -> Vec<String> {
        self.grid.chunks(self.gw).map(|row| row.iter().map(|i| world::model::char_of(*i)).collect()).collect()
    }

    /// A picture of the world taken into the drawing, when it is of the drawing's size.
    fn take(&mut self, rows: &[String]) {
        let width = rows.first().map_or(0, |r| r.chars().count());
        if width == 0 || width > 64 || rows.len() > 64 || rows.iter().any(|r| r.chars().count() != width) {
            return;
        }
        (self.gw, self.gh) = (width, rows.len());
        let index = |ch: char| (0..world::model::PALETTE_MAX as u8).find(|i| world::model::char_of(*i) == ch).unwrap_or(0);
        self.grid = rows.iter().flat_map(|r| r.chars().map(index)).collect();
    }

    /// The drawing made this size, what was drawn kept in its top left corner.
    fn resize(&mut self, gw: usize, gh: usize) {
        if (gw, gh) == (self.gw, self.gh) {
            return;
        }
        let mut grid = vec![0; gw * gh];
        for y in 0..gh.min(self.gh) {
            for x in 0..gw.min(self.gw) {
                grid[y * gw + x] = self.grid[y * self.gw + x];
            }
        }
        (self.grid, self.gw, self.gh) = (grid, gw, gh);
    }

    /// The spawn of what is being made, with everything set for it.
    fn spawn(&self) -> serde_json::Value {
        let ware = |w: &Ware| serde_json::json!({ "name": w.name, "kind": w.kind, "price": w.price, "heal": w.heal, "damage": w.damage });
        let mut spawn = serde_json::json!({ "name": self.name.trim(), "description": self.description.trim() });
        match self.kind {
            MakeKind::Creature => {
                spawn["attitude"] = ["hostile", "neutral", "friendly"][self.attitude.min(2)].into();
                if self.book.is_empty() {
                    spawn["stats"] = serde_json::json!({
                        "ac": self.ac, "hp": self.hp, "speed": self.speed, "xp": self.xp,
                        "attacks": self.hits.iter().filter(|h| !h.name.trim().is_empty()).map(|h| serde_json::json!({ "name": h.name, "bonus": h.bonus, "damage": h.damage, "range": h.range })).collect::<Vec<_>>()
                    });
                } else {
                    spawn["monster"] = self.book.clone().into();
                }
                if self.count > 0 {
                    spawn["count"] = self.count.into();
                }
                if !self.faction.is_empty() {
                    spawn["faction"] = self.faction.clone().into();
                }
                if self.merchant {
                    spawn["shop"] = self.wares.iter().filter(|w| !w.name.trim().is_empty()).map(ware).collect::<Vec<_>>().into();
                }
            }
            MakeKind::Object => {
                spawn["blocking"] = self.blocking.into();
                if self.chest {
                    spawn["contents"] = self.contents.iter().filter(|w| !w.name.trim().is_empty()).map(ware).collect::<Vec<_>>().into();
                    spawn["lock"] = self.lock.into();
                    spawn["key"] = self.key.trim().into();
                }
            }
            MakeKind::Tile | MakeKind::Item => {}
        }
        spawn
    }

    /// The form filled again from a creature or object of the library.
    fn edit(&mut self, entity: &Entity, world: &World) {
        let s = &entity.spawn;
        self.kind = if entity.creature { MakeKind::Creature } else { MakeKind::Object };
        self.id = entity.id.clone();
        self.name = entity.name.clone();
        self.description = s["description"].as_str().unwrap_or("").to_string();
        self.attitude = match s["attitude"].as_str() {
            Some("neutral") => 1,
            Some("friendly") => 2,
            _ => 0,
        };
        self.book = s["monster"].as_str().unwrap_or("").to_string();
        let n = |v: &serde_json::Value| v.as_i64().unwrap_or(0);
        self.ac = n(&s["stats"]["ac"]) as i32;
        self.hp = n(&s["stats"]["hp"]) as i32;
        self.speed = n(&s["stats"]["speed"]) as u32;
        self.xp = n(&s["stats"]["xp"]) as u32;
        self.hits = s["stats"]["attacks"].as_array().map(|list| list.iter().map(|a| Hit { name: a["name"].as_str().unwrap_or("").into(), bonus: n(&a["bonus"]) as i32, damage: a["damage"].as_str().unwrap_or("").into(), range: n(&a["range"]) as u32 }).collect()).unwrap_or_default();
        self.count = n(&s["count"]) as u32;
        self.faction = s["faction"].as_str().unwrap_or("").to_string();
        let wares = |v: &serde_json::Value| v.as_array().map(|list| list.iter().map(|w| Ware { name: w["name"].as_str().unwrap_or("").into(), kind: w["kind"].as_str().unwrap_or("").into(), price: n(&w["price"]) as u32, heal: w["heal"].as_str().unwrap_or("").into(), damage: w["damage"].as_str().unwrap_or("").into() }).collect()).unwrap_or_default();
        self.merchant = s["shop"].is_array();
        self.wares = wares(&s["shop"]);
        self.blocking = s["blocking"].as_bool().unwrap_or(true);
        self.chest = s["contents"].is_array();
        self.contents = wares(&s["contents"]);
        self.lock = n(&s["lock"]) as i32;
        self.key = s["key"].as_str().unwrap_or("").to_string();
        // The picture in the nearest colours of this world.
        let nearest = |color: [u8; 3]| {
            let distance = |c: &[u8; 3]| (0..3).map(|k| (c[k] as i32 - color[k] as i32).pow(2)).sum::<i32>();
            world.palette.colors.iter().enumerate().min_by_key(|(_, c)| distance(c)).map_or(1, |(i, _)| i as u8 + 1)
        };
        let rows: Vec<String> = entity.art.rows.iter().map(|r| r.chars().map(|ch| entity.art.colors.iter().find(|(c, _)| *c == ch).map_or(ch, |(_, color)| world::model::char_of(nearest(*color)))).collect()).collect();
        self.take(&rows);
    }
}

/// An action of the master's, sent the way every other is.
fn send(act: GmAct, session: &Session, settings: &Settings, table: &mut Table) {
    if session.is_client() {
        session.intend(Intent::Gm(act));
    } else {
        table.inbox.push((player_name(settings), Intent::Gm(act)));
    }
}

/// A picture as egui knows it, made once for what it shows.
fn texture(key: String, pixels: impl FnOnce() -> crate::art::Pixels, egui: &mut EguiContexts, images: &mut Assets<Image>, cache: &mut HashMap<String, egui::TextureId>) -> egui::TextureId {
    if let Some(id) = cache.get(&key) {
        return *id;
    }
    let id = egui.add_image(bevy_egui::EguiTextureHandle::Strong(images.add(pixels().into_image())));
    cache.insert(key, id);
    id
}

fn art_pixels(art: &Art) -> crate::art::Pixels {
    let (w, h) = (art.rows.first().map_or(1, |r| r.chars().count()) as u32, art.rows.len().max(1) as u32);
    let mut p = crate::art::Pixels::new(w, h, CLEAR);
    for (y, row) in art.rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if let Some((_, [r, g, b])) = art.colors.iter().find(|(c, _)| *c == ch) {
                p.set(x as i32, y as i32, [*r, *g, *b, 255]);
            }
        }
    }
    p
}

/// A card of the palette: the picture large, its name under it, outlined when chosen.
fn card(ui: &mut Ui, picture: egui::TextureId, chosen: bool, name: &str, mark: &str, side: f32) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(side, side + 22.0), Sense::click());
    let painter = ui.painter_at(rect);
    let back = if chosen { BRASS_LIGHT } else if response.hovered() { PARCHMENT_LIGHT } else { PARCHMENT_SHADE };
    painter.rect_filled(rect, 2.0, c(back));
    let face = Rect::from_min_size(rect.min + vec2(3.0, 3.0), vec2(side - 6.0, side - 6.0));
    painter.rect_filled(face, 0.0, c(WOOD_DEEP));
    painter.image(picture, face, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
    painter.rect_stroke(rect, 2.0, egui::Stroke::new(if chosen { 2.0 } else { 1.0 }, c(if chosen { WAX_DARK } else { BRASS_DARK })), egui::StrokeKind::Inside);
    // The whole name, in as many as two lines under the picture.
    let galley = painter.layout(name.to_string(), egui::FontId::proportional(8.0), c(INK), side - 2.0);
    let at = egui::pos2(rect.center().x - galley.size().x / 2.0, face.max.y + 2.0);
    painter.with_clip_rect(rect).galley(at, galley, c(INK));
    if !mark.is_empty() {
        painter.text(face.right_top() + vec2(-2.0, 2.0), egui::Align2::RIGHT_TOP, mark, egui::FontId::proportional(10.0), c(PARCHMENT_LIGHT));
    }
    response.on_hover_text(format!("{name} {mark}")).clicked()
}

/// A tab of the editor's bar or a tool: lit while it is the one in use.
fn lit(ui: &mut Ui, label: &str, on: bool, about: &str) -> bool {
    let b = button(label);
    ui.add(if on { b.primary() } else { b }).on_hover_text(about).clicked()
}

fn wares_ui(ui: &mut Ui, list: &mut Vec<Ware>, salt: &str, lang: Lang) {
    let t = |text: T| text.text(lang);
    let mut gone = None;
    for (n, ware) in list.iter_mut().enumerate() {
        ui.horizontal(|row| {
            row.add(egui::TextEdit::singleline(&mut ware.name).desired_width(90.0).hint_text(t(T::WsWareName)));
            egui::ComboBox::from_id_salt(format!("{salt}_kind_{n}")).width(70.0).selected_text(if ware.kind.is_empty() { "—" } else { ware.kind.as_str() }).show_ui(row, |ui| {
                for kind in KINDS {
                    ui.selectable_value(&mut ware.kind, kind.to_string(), kind);
                }
            });
            row.add(egui::DragValue::new(&mut ware.price).range(0..=100_000).prefix("$"));
            if row.add(button("×")).clicked() {
                gone = Some(n);
            }
        });
        if matches!(ware.kind.as_str(), "potion" | "food") {
            ui.horizontal(|row| {
                hint(row, t(T::WsHeal));
                row.add(egui::TextEdit::singleline(&mut ware.heal).desired_width(60.0).hint_text("2d4+2"));
            });
        }
        if matches!(ware.kind.as_str(), "throwable" | "weapon") {
            ui.horizontal(|row| {
                hint(row, t(T::WsDamage));
                row.add(egui::TextEdit::singleline(&mut ware.damage).desired_width(60.0).hint_text("1d6"));
            });
        }
    }
    if let Some(n) = gone {
        list.remove(n);
    }
    if ui.add(button(t(T::WsAddWare))).clicked() {
        list.push(Ware { kind: "junk".into(), price: 5, ..Ware::default() });
    }
}

/// The look of the editor's windows: parchment like every sheet of the game.
fn frame() -> egui::Frame {
    egui::Frame::new().fill(c(PARCHMENT)).stroke(egui::Stroke::new(1.0, c(BRASS_DARK))).inner_margin(8.0).corner_radius(3.0)
}

/// A window of the editor: it can be dragged by its title, folded to its title, and resized,
/// and it never covers more of the screen than it was given.
fn window<R>(ctx: &egui::Context, panels: &mut Vec<Rect>, id: &str, title: &str, at: egui::Pos2, size: egui::Vec2, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    // Folded or not, kept by egui with the window's place.
    let folded_id = egui::Id::new(("workshop_folded", id));
    let folded = ctx.data(|d| d.get_temp::<bool>(folded_id)).unwrap_or(false);
    let mut inner = None;
    let shown = egui::Window::new(title)
        .id(egui::Id::new(("workshop", id)))
        .title_bar(false)
        .default_pos(at)
        .default_size(size)
        .max_width(size.x.max(200.0) * 1.6)
        .resizable(!folded)
        .constrain(true)
        .frame(frame())
        .show(ctx, |ui| {
            // The window's own header: its name, and a button that folds it to that line.
            // The window is dragged by any empty part of it, the header included.
            ui.horizontal(|row| {
                let mark = if folded { "[+]" } else { "[-]" };
                if row.add(egui::Button::new(egui::RichText::new(mark).color(c(INK))).frame(false)).clicked() {
                    row.ctx().data_mut(|d| d.insert_temp(folded_id, !folded));
                }
                row.label(egui::RichText::new(title).strong().size(15.0).color(c(INK)));
            });
            if !folded {
                ui.separator();
                inner = Some(add(ui));
            }
        })?;
    panels.push(shown.response.rect);
    inner
}

/// The name of a group that folds, written dark enough to read.
fn dark(text: &str) -> egui::RichText {
    egui::RichText::new(text).strong().color(c(INK))
}

/// A heading inside a window: the name of a group of things below it.
fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text).strong().color(c(WAX_DARK)));
}

/// The drawing on the screen, `scale` screen pixels to a pixel of it; `tiled` repeats a tile
/// three by three to show how it lies on the ground.
fn preview(ui: &mut Ui, ws: &Workshop, world: &World, scale: f32, tiled: bool) {
    let times = if tiled { 3 } else { 1 };
    let (rect, response) = ui.allocate_exact_size(vec2(ws.gw as f32, ws.gh as f32) * scale * times as f32, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, c(WOOD_DEEP));
    for ty in 0..times {
        for tx in 0..times {
            for y in 0..ws.gh {
                for x in 0..ws.gw {
                    let [r, g, b, a] = world.palette.rgba(ws.grid[y * ws.gw + x]);
                    if a == 0 {
                        continue;
                    }
                    let at = rect.min + vec2(((tx * ws.gw + x) as f32) * scale, ((ty * ws.gh + y) as f32) * scale);
                    painter.rect_filled(Rect::from_min_size(at, vec2(scale, scale)), 0.0, egui::Color32::from_rgb(r, g, b));
                }
            }
        }
    }
    response.on_hover_text(format!("×{}", scale as u32));
}

/// The cells of one colour joined to `start`, filled with `color`.
fn flood(grid: &mut [u8], start: usize, color: u8, side: usize) {
    let rows = grid.len() / side;
    let from = grid[start];
    if from == color {
        return;
    }
    let mut stack = vec![start];
    while let Some(at) = stack.pop() {
        if grid[at] != from {
            continue;
        }
        grid[at] = color;
        let (x, y) = (at % side, at / side);
        if x > 0 {
            stack.push(at - 1);
        }
        if x + 1 < side {
            stack.push(at + 1);
        }
        if y > 0 {
            stack.push(at - side);
        }
        if y + 1 < rows {
            stack.push(at + side);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn workshop_ui(
    mut egui: EguiContexts,
    (mut phase, game, session, settings, store): (ResMut<Phase>, Res<Game>, Res<Session>, Res<Settings>, Res<Store>),
    (mut ws, mut table, mut images, mut panel): (ResMut<Workshop>, ResMut<Table>, ResMut<Assets<Image>>, ResMut<Panel>),
    mut cache: Local<HashMap<String, egui::TextureId>>,
) {
    ws.panels.clear();
    let judging = table.judging.clone();
    if *phase != Phase::Master && !(*phase == Phase::Judging && judging.is_some()) {
        if panel.workshop {
            panel.workshop = false;
        }
        return;
    }
    let lang = settings.lang;
    let t = |text: T| text.text(lang);
    if !ws.loaded {
        ws.arts = store.load_arts();
        ws.made = store.load_entities();
        ws.favorites = store.load_favorites();
        ws.loaded = true;
    }
    // The library's shelves are read when they are looked at.
    if ws.mode == Mode::Library {
        if !ws.saves_read {
            let mut seen = std::collections::HashSet::new();
            ws.saves = store.list_saves().into_iter().filter(|s| seen.insert(s.meta.world_id.clone())).map(|s| (s.meta.title, s.meta.place, s.path)).collect();
            ws.saves_read = true;
        }
        if ws.shelf.is_none() {
            ws.shelf = Some(shelf_of(ws.source, &ws, &game.world, &store));
        }
    }
    let world = &game.world;
    // The pictures of the world's tiles and sprites, and of the library's.
    let mut tiles: Vec<(String, String, egui::TextureId, bool)> = Vec::new();
    for (id, def) in &world.tiles {
        let key = format!("tile:{id}:{}", def.sprite.rows.concat().bytes().map(|b| b as usize).sum::<usize>());
        let picture = texture(key, || crate::scene::sprite(&def.sprite, &world.palette), &mut egui, &mut images, &mut cache);
        tiles.push((id.clone(), def.name.clone(), picture, def.walkable));
    }
    tiles.sort_by(|a, b| b.3.cmp(&a.3).then(a.1.cmp(&b.1)));
    let made_pictures: Vec<egui::TextureId> = ws.made.iter().map(|e| texture(format!("made:{}:{}", e.id, e.art.rows.concat().bytes().map(|b| b as usize).sum::<usize>()), || art_pixels(&e.art), &mut egui, &mut images, &mut cache)).collect();
    let key = |a: &Art| format!("art:{}:{}:{}", a.tile, a.id, a.rows.concat().bytes().map(|b| b as usize).sum::<usize>());
    let mut shelf = ShelfPictures::default();
    if let (Mode::Library, Some((arts, entities))) = (ws.mode, ws.shelf.as_ref()) {
        shelf.arts = arts.iter().map(|a| texture(key(a), || art_pixels(a), &mut egui, &mut images, &mut cache)).collect();
        shelf.entities = entities.iter().map(|e| texture(key(&e.art), || art_pixels(&e.art), &mut egui, &mut images, &mut cache)).collect();
    }
    let favourite_pictures = ShelfPictures {
        arts: ws.favorites.arts.iter().map(|a| texture(key(a), || art_pixels(a), &mut egui, &mut images, &mut cache)).collect(),
        entities: ws.favorites.entities.iter().map(|e| texture(key(&e.art), || art_pixels(&e.art), &mut egui, &mut images, &mut cache)).collect(),
    };
    let Ok(ctx) = egui.ctx_mut() else { return };
    let screen = ctx.content_rect();
    let mut acts: Vec<GmAct> = Vec::new();
    let ws = &mut *ws;
    // The bar of rooms across the top: what the master is doing now, at a glance.
    let rooms = [(Mode::Run, T::WsRun, T::WsRunAbout), (Mode::Map, T::WsMap, T::WsMapAbout), (Mode::Make, T::WsMake, T::WsMakeAbout), (Mode::Library, T::WsLibrary, T::WsLibraryAbout), (Mode::Rules, T::WsRules, T::WsRulesAbout)];
    let bar = egui::Area::new(egui::Id::new("workshop_bar")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, vec2(0.0, 4.0)).show(ctx, |ui| {
        frame().inner_margin(4.0).show(ui, |ui| {
            ui.horizontal(|row| {
                for (mode, label, about) in rooms {
                    if lit(row, t(label), ws.mode == mode, t(about)) {
                        ws.mode = mode;
                        ws.rect_from = None;
                        ws.moving = None;
                    }
                }
            });
        });
    });
    ws.panels.push(bar.response.rect);
    panel.workshop = ws.mode != Mode::Run;
    let top = bar.response.rect.max.y + 6.0;
    match ws.mode {
        Mode::Run => run_room(ctx, ws, world, &mut table, &mut acts, &mut phase, lang, top, screen, &player_name(&settings)),
        Mode::Map => map_room(ctx, ws, &tiles, &made_pictures, &favourite_pictures, &mut acts, lang, top, screen),
        Mode::Make => make_room(ctx, ws, world, &tiles, &mut acts, lang, top, screen),
        Mode::Library => library_room(ctx, ws, world, &shelf, &mut acts, &store, lang, top, screen),
        Mode::Rules => rules_room(ctx, ws, world, &mut table, &mut acts, lang, top, screen),
    }
    // The ring over what was clicked on the map, and the page opened from it.
    if matches!(ws.mode, Mode::Run | Mode::Map) {
        if ws.script_asking
            && let Some(answer) = table.script.take()
        {
            ws.script_asking = false;
            match answer {
                Ok(effects) => ws.script = effects,
                Err(trouble) => table.master_note = trouble,
            }
        }
        ring(ctx, ws, world, &mut acts, lang, screen);
    }
    // What a player did waits for the master's answer, whatever room they are in.
    match judging {
        Some(view) => {
            let done = reply_window(ctx, ws, world, &view, &mut acts, lang, screen);
            if done && session.is_client() {
                table.judging = None;
                *phase = Phase::Walking;
            }
        }
        None => ws.reply_to = None,
    }
    for act in acts {
        send(act, &session, &settings, &mut table);
    }
}

#[allow(clippy::too_many_arguments)]
fn map_room(ctx: &egui::Context, ws: &mut Workshop, tiles: &[(String, String, egui::TextureId, bool)], made: &[egui::TextureId], favourite: &ShelfPictures, acts: &mut Vec<GmAct>, lang: Lang, top: f32, screen: Rect) {
    let t = |text: T| text.text(lang);
    let mut panels = std::mem::take(&mut ws.panels);
    window(ctx, &mut panels, "palette", t(T::WsPalette), egui::pos2(8.0, top), vec2(236.0, (screen.height() - top - 60.0).min(screen.height() * 0.7)), |ui| {
        heading(ui, t(T::WsTools));
        let tools = [(MapTool::Brush, T::WsBrush, T::WsBrushHint), (MapTool::Rect, T::WsRect, T::WsRectHint), (MapTool::Erase, T::WsErase, T::WsEraseHint), (MapTool::Place, T::WsPlace, T::WsPlaceHint), (MapTool::Move, T::WsMove, T::WsMoveHint)];
        ui.horizontal_wrapped(|row| {
            for (tool, label, about) in tools {
                if lit(row, t(label), ws.tool == tool, t(about)) {
                    ws.tool = tool;
                    ws.rect_from = None;
                }
            }
        });
        let about = tools.iter().find(|(tool, _, _)| *tool == ws.tool).map_or(T::WsBrushHint, |(_, _, about)| *about);
        hint(ui, t(about));
        egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            // What the master starred comes first: from any world, put in with a click.
            if !ws.favorites.arts.is_empty() || !ws.favorites.entities.is_empty() {
                heading(ui, t(T::WsFavourites));
                ui.horizontal_wrapped(|row| {
                    for (n, entity) in ws.favorites.entities.clone().iter().enumerate() {
                        let chosen = ws.placing.as_ref().is_some_and(|p| p.id == entity.id) && ws.tool == MapTool::Place;
                        if card(row, favourite.entities[n], chosen, &entity.name, if entity.creature { "☺" } else { "▣" }, 48.0) {
                            ws.placing = Some(entity.clone());
                            ws.tool = MapTool::Place;
                        }
                    }
                    for (n, art) in ws.favorites.arts.clone().iter().enumerate().filter(|(_, a)| a.tile) {
                        let id = world::build::slug(&art.id);
                        if card(row, favourite.arts[n], ws.tile == id && matches!(ws.tool, MapTool::Brush | MapTool::Rect), &art.name, if art.walkable { "○" } else { "×" }, 48.0) {
                            // A tile of another world comes into this one first.
                            if !tiles.iter().any(|(t, ..)| *t == id) {
                                acts.push(GmAct::ImportArt(art.clone()));
                            }
                            ws.tile = id;
                            if !matches!(ws.tool, MapTool::Brush | MapTool::Rect) {
                                ws.tool = MapTool::Brush;
                            }
                        }
                    }
                });
            }
            heading(ui, t(T::WsTiles));
            ui.horizontal_wrapped(|row| {
                for (id, name, picture, walkable) in tiles {
                    let mark = if *walkable { "○" } else { "×" };
                    if card(row, *picture, ws.tile == *id && matches!(ws.tool, MapTool::Brush | MapTool::Rect), name, mark, 48.0) {
                        ws.tile = id.clone();
                        if !matches!(ws.tool, MapTool::Brush | MapTool::Rect) {
                            ws.tool = MapTool::Brush;
                        }
                    }
                }
            });
            if let Some((id, name, _, walkable)) = tiles.iter().find(|(id, ..)| *id == ws.tile) {
                let mut on = *walkable;
                if kit::toggle(ui, &format!("{name}: {}", t(T::WsWalkable)), &mut on).changed() {
                    acts.push(GmAct::SetWalkable { tile: id.clone(), walkable: on });
                }
            }
            heading(ui, t(T::WsMade));
            if ws.made.is_empty() {
                hint(ui, t(T::WsNothingMade));
            }
            ui.horizontal_wrapped(|row| {
                for (n, entity) in ws.made.iter().enumerate() {
                    let mark = if entity.creature { "☺" } else { "▣" };
                    if card(row, made[n], ws.placing.as_ref().is_some_and(|p| p.id == entity.id) && ws.tool == MapTool::Place, &entity.name, mark, 48.0) {
                        ws.placing = Some(entity.clone());
                        ws.tool = MapTool::Place;
                    }
                }
            });
            if ui.add(button(t(T::WsMakeNew))).clicked() {
                ws.mode = Mode::Make;
            }
        });
    });
    // What is in hand and what is under the mouse, along the bottom.
    let chosen = match ws.tool {
        MapTool::Brush | MapTool::Rect => tiles.iter().find(|(id, ..)| *id == ws.tile).map(|(_, name, ..)| name.clone()).unwrap_or_else(|| t(T::WsPickTile).to_string()),
        MapTool::Place => ws.placing.as_ref().map(|e| e.name.clone()).unwrap_or_else(|| t(T::WsPickMade).to_string()),
        MapTool::Erase | MapTool::Move => String::new(),
    };
    let tool = [(MapTool::Brush, T::WsBrush), (MapTool::Rect, T::WsRect), (MapTool::Erase, T::WsErase), (MapTool::Place, T::WsPlace), (MapTool::Move, T::WsMove)].iter().find(|(tool, _)| *tool == ws.tool).map_or("", |(_, l)| t(*l));
    let line = [tool.to_string(), chosen, ws.hovered.clone()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("   ·   ");
    let status = egui::Area::new(egui::Id::new("workshop_status")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -6.0)).show(ctx, |ui| {
        frame().inner_margin(5.0).show(ui, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(line).color(c(INK))).wrap_mode(egui::TextWrapMode::Extend));
        });
    });
    panels.push(status.response.rect);
    ws.panels = panels;
}

#[allow(clippy::too_many_arguments)]
fn make_room(ctx: &egui::Context, ws: &mut Workshop, world: &World, tiles: &[(String, String, egui::TextureId, bool)], acts: &mut Vec<GmAct>, lang: Lang, top: f32, screen: Rect) {
    let t = |text: T| text.text(lang);
    let mut panels = std::mem::take(&mut ws.panels);
    window(ctx, &mut panels, "drawing", t(T::WsDrawing), egui::pos2(8.0, top), vec2(PIXEL * SIDE as f32 + 140.0, 420.0), |ui| {
        let kinds = [(MakeKind::Creature, T::WsCreature), (MakeKind::Object, T::WsObject), (MakeKind::Tile, T::WsTile), (MakeKind::Item, T::WsItem)];
        ui.horizontal(|row| {
            row.label(t(T::WsWhat));
            egui::ComboBox::from_id_salt("ws_kind").width(200.0).selected_text(t(kinds.iter().find(|(k, _)| *k == ws.kind).map_or(T::WsCreature, |(_, l)| *l))).show_ui(row, |ui| {
                for (kind, label) in kinds {
                    ui.selectable_value(&mut ws.kind, kind, t(label));
                }
            });
        });
        // A thing for the bag is drawn a cell of sixteen pixels to each cell it takes.
        let (gw, gh) = if ws.kind == MakeKind::Item { (SIDE * ws.cells.0, SIDE * ws.cells.1) } else { (SIDE, SIDE) };
        ws.resize(gw, gh);
        // The pens.
        ui.horizontal(|row| {
            for (pen, label, about) in [(Pen::Pencil, T::WsPencil, T::WsPencilHint), (Pen::Eraser, T::WsEraser, T::WsEraserHint), (Pen::Fill, T::WsFill, T::WsFillHint), (Pen::Picker, T::WsPicker, T::WsPickerHint)] {
                if lit(row, t(label), ws.pen == pen, t(about)) {
                    ws.pen = pen;
                }
            }
        });
        ui.horizontal_top(|row| {
            // The drawing: the left button uses the pen, the right one always clears.
            // A cell of the drawing as big as fits: a long thing for the bag is drawn smaller.
            let pixel = PIXEL.min(176.0 / ws.gw as f32).min(200.0 / ws.gh as f32);
            let (rect, response) = row.allocate_exact_size(vec2(pixel * ws.gw as f32, pixel * ws.gh as f32), Sense::click_and_drag());
            let painter = row.painter_at(rect);
            for y in 0..ws.gh {
                for x in 0..ws.gw {
                    let cell = Rect::from_min_size(rect.min + vec2(x as f32 * pixel, y as f32 * pixel), vec2(pixel, pixel));
                    let [r, g, b, a] = world.palette.rgba(ws.grid[y * ws.gw + x]);
                    let shade = if (x + y) % 2 == 0 { 200 } else { 225 };
                    painter.rect_filled(cell, 0.0, if a == 0 { egui::Color32::from_gray(shade) } else { egui::Color32::from_rgb(r, g, b) });
                }
            }
            let mut cell_under = None;
            if let Some(at) = response.hover_pos() {
                let (x, y) = (((at.x - rect.min.x) / pixel) as usize, ((at.y - rect.min.y) / pixel) as usize);
                if x < ws.gw && y < ws.gh {
                    cell_under = Some(y * ws.gw + x);
                    let cell = Rect::from_min_size(rect.min + vec2(x as f32 * pixel, y as f32 * pixel), vec2(pixel, pixel));
                    painter.rect_stroke(cell, 0.0, egui::Stroke::new(1.0, c(INK)), egui::StrokeKind::Inside);
                }
            }
            painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, c(BRASS_DARK)), egui::StrokeKind::Inside);
            let (primary, secondary) = row.input(|i| (i.pointer.primary_down(), i.pointer.secondary_down()));
            if let Some(at) = cell_under.filter(|_| response.hovered() || response.dragged()) {
                if secondary {
                    ws.grid[at] = 0;
                } else if primary {
                    match ws.pen {
                        Pen::Pencil => ws.grid[at] = ws.color,
                        Pen::Eraser => ws.grid[at] = 0,
                        Pen::Fill => {
                            if response.is_pointer_button_down_on() && row.input(|i| i.pointer.primary_pressed()) {
                                let color = ws.color;
                                flood(&mut ws.grid, at, color, ws.gw);
                            }
                        }
                        Pen::Picker => {
                            ws.color = ws.grid[at];
                            ws.pen = Pen::Pencil;
                        }
                    }
                }
            }
            // How it will look: as big as on the map, larger, and laid on the ground.
            row.vertical(|col| {
                hint(col, t(T::WsPreview));
                preview(col, ws, world, 1.0, false);
                preview(col, ws, world, 2.0, false);
                match ws.kind {
                    MakeKind::Tile => preview(col, ws, world, 1.0, true),
                    MakeKind::Item => {}
                    _ => preview(col, ws, world, 4.0, false),
                }
            });
        });
        // The colours of this world: index 0 is clear.
        hint(ui, t(T::WsColors));
        ui.horizontal_wrapped(|row| {
            row.spacing_mut().item_spacing = vec2(2.0, 2.0);
            for index in 0..world.palette.len() as u8 {
                let (rect, response) = row.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
                let [r, g, b, a] = world.palette.rgba(index);
                row.painter().rect_filled(rect, 0.0, if a == 0 { egui::Color32::from_gray(210) } else { egui::Color32::from_rgb(r, g, b) });
                if index == 0 {
                    row.painter().line_segment([rect.left_top(), rect.right_bottom()], egui::Stroke::new(1.0, egui::Color32::RED));
                }
                if ws.color == index {
                    row.painter().rect_stroke(rect, 0.0, egui::Stroke::new(2.0, c(INK)), egui::StrokeKind::Inside);
                }
                if response.clicked() {
                    ws.color = index;
                    if ws.pen != Pen::Fill {
                        ws.pen = Pen::Pencil;
                    }
                }
            }
        });
        ui.horizontal_wrapped(|row| {
            if row.add(button(t(T::WsClear))).clicked() {
                ws.grid = vec![0; ws.gw * ws.gh];
            }
            // A picture of the world taken into the drawing, to change it.
            egui::ComboBox::from_id_salt("ws_take").width(150.0).selected_text(t(T::WsTake)).height(260.0).show_ui(row, |ui| {
                for (id, name, _, _) in tiles {
                    if ui.button(format!("▦ {name}")).clicked()
                        && let Some(def) = world.tiles.get(id)
                    {
                        let rows = def.sprite.rows.clone();
                        ws.take(&rows);
                    }
                }
                for (id, sprite) in &world.sprites {
                    if ui.button(format!("☺ {id}")).clicked() {
                        let rows = sprite.rows.clone();
                        ws.take(&rows);
                    }
                }
            });
        });
        // The model draws it instead, as described: it is kept and can be taken to change.
        heading(ui, t(T::WsAskModel));
        ui.add(egui::TextEdit::multiline(&mut ws.describe).desired_rows(2).desired_width(f32::INFINITY).hint_text(t(T::WsDescribe)));
        let ready = !ws.describe.trim().is_empty() && !ws.id.trim().is_empty();
        if ui.add(button(t(T::WsAskModel)).enabled(ready, t(T::WsNeedIdFirst))).clicked() {
            acts.push(GmAct::DrawArt { tile: ws.kind == MakeKind::Tile, id: world::build::slug(&ws.id), description: ws.describe.trim().to_string(), walkable: ws.walkable });
        }
    });
    let width = 300.0;
    window(ctx, &mut panels, "properties", t(T::WsProperties), egui::pos2(screen.width() - width - 24.0, top), vec2(width, (screen.height() - top - 40.0).min(screen.height() * 0.75)), |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| properties(ui, ws, world, acts, lang));
    });
    ws.panels = panels;
}

/// What is being made, set up in groups that fold.
fn properties(ui: &mut Ui, ws: &mut Workshop, world: &World, acts: &mut Vec<GmAct>, lang: Lang) {
    let t = |text: T| text.text(lang);
    let field = |ui: &mut Ui, label: &str, text: &mut String, hint_text: &str| {
        ui.horizontal(|row| {
            row.add_sized(vec2(70.0, 18.0), egui::Label::new(label));
            row.add(egui::TextEdit::singleline(text).desired_width(180.0).hint_text(hint_text));
        });
    };
    egui::CollapsingHeader::new(dark(t(T::WsMain))).default_open(true).show(ui, |ui| {
        field(ui, t(T::WsName), &mut ws.name, t(T::WsNameHint));
        field(ui, t(T::WsId), &mut ws.id, t(T::WsIdHint));
        if ws.kind != MakeKind::Tile {
            ui.label(t(T::WsDescription));
            ui.add(egui::TextEdit::multiline(&mut ws.description).desired_rows(2).desired_width(f32::INFINITY));
        }
        if ws.kind == MakeKind::Tile {
            kit::toggle(ui, t(T::WsWalkable), &mut ws.walkable);
        }
    });
    match ws.kind {
        MakeKind::Tile => {}
        MakeKind::Item => item_properties(ui, ws, world, acts, lang),
        MakeKind::Object => {
            egui::CollapsingHeader::new(dark(t(T::WsBehaviour))).default_open(true).show(ui, |ui| {
                kit::toggle(ui, t(T::WsBlocking), &mut ws.blocking);
            });
            egui::CollapsingHeader::new(dark(t(T::WsChestPart))).default_open(ws.chest).show(ui, |ui| {
                kit::toggle(ui, t(T::WsChest), &mut ws.chest);
                if ws.chest {
                    wares_ui(ui, &mut ws.contents, "contents", lang);
                    ui.horizontal(|row| {
                        row.label(t(T::WsLock));
                        row.add(egui::DragValue::new(&mut ws.lock).range(0..=30));
                    });
                    hint(ui, t(T::WsLockHint));
                    field(ui, t(T::WsKey), &mut ws.key, t(T::WsKeyHint));
                }
            });
        }
        MakeKind::Creature => {
            egui::CollapsingHeader::new(dark(t(T::WsBehaviour))).default_open(true).show(ui, |ui| {
                let attitudes = [t(T::WsHostile), t(T::WsNeutral), t(T::WsFriendly)];
                ui.horizontal(|row| {
                    row.add_sized(vec2(70.0, 18.0), egui::Label::new(t(T::WsAttitude)));
                    egui::ComboBox::from_id_salt("ws_attitude").width(160.0).selected_text(attitudes[ws.attitude.min(2)]).show_ui(row, |ui| {
                        for (n, label) in attitudes.iter().enumerate() {
                            ui.selectable_value(&mut ws.attitude, n, *label);
                        }
                    });
                });
                ui.horizontal(|row| {
                    row.add_sized(vec2(70.0, 18.0), egui::Label::new(t(T::WsFaction)));
                    let shown = if ws.faction.is_empty() { "—".to_string() } else { world.factions.iter().find(|f| f.id == ws.faction).map_or(ws.faction.clone(), |f| f.name.clone()) };
                    egui::ComboBox::from_id_salt("ws_faction").width(160.0).selected_text(shown).show_ui(row, |ui| {
                        ui.selectable_value(&mut ws.faction, String::new(), "—");
                        for faction in &world.factions {
                            ui.selectable_value(&mut ws.faction, faction.id.clone(), faction.name.as_str());
                        }
                    });
                });
                ui.horizontal(|row| {
                    row.add_sized(vec2(70.0, 18.0), egui::Label::new(t(T::WsCount)));
                    row.add(egui::DragValue::new(&mut ws.count).range(0..=8));
                });
                hint(ui, t(T::WsCountHint));
            });
            egui::CollapsingHeader::new(dark(t(T::WsFight))).default_open(true).show(ui, |ui| {
                // Its numbers: a monster of the book's, or its own.
                let mut monsters: Vec<(&String, &String)> = rules::srd().monsters.iter().map(|(index, m)| (index, &m.name)).collect();
                monsters.sort_by(|a, b| a.1.cmp(b.1));
                let shown = if ws.book.is_empty() { t(T::WsOwnNumbers).to_string() } else { rules::srd().monsters.get(&ws.book).map_or(ws.book.clone(), |m| m.name.clone()) };
                ui.horizontal(|row| {
                    row.add_sized(vec2(70.0, 18.0), egui::Label::new(t(T::WsNumbers)));
                    egui::ComboBox::from_id_salt("ws_book").width(160.0).selected_text(shown).height(260.0).show_ui(row, |ui| {
                        ui.selectable_value(&mut ws.book, String::new(), t(T::WsOwnNumbers));
                        for (index, name) in monsters {
                            ui.selectable_value(&mut ws.book, index.clone(), name.as_str());
                        }
                    });
                });
                if !ws.book.is_empty() {
                    if let Some(m) = rules::srd().monsters.get(&ws.book) {
                        hint(ui, &format!("{} {} · {} {} · {} {}", t(T::WsAc), m.ac, t(T::WsHp), m.hp, t(T::WsXp), m.xp));
                    }
                    return;
                }
                egui::Grid::new("ws_numbers").num_columns(4).spacing(vec2(6.0, 4.0)).show(ui, |grid| {
                    grid.label(t(T::WsAc));
                    grid.add(egui::DragValue::new(&mut ws.ac).range(1..=30));
                    grid.label(t(T::WsHp));
                    grid.add(egui::DragValue::new(&mut ws.hp).range(1..=999));
                    grid.end_row();
                    grid.label(t(T::WsSpeed));
                    grid.add(egui::DragValue::new(&mut ws.speed).range(0..=120).speed(5).suffix(" ft"));
                    grid.label(t(T::WsXp));
                    grid.add(egui::DragValue::new(&mut ws.xp).range(0..=50_000).speed(10));
                    grid.end_row();
                });
                hint(ui, t(T::WsAttacks));
                let mut gone = None;
                for (n, hit) in ws.hits.iter_mut().enumerate() {
                    ui.horizontal(|row| {
                        row.add(egui::TextEdit::singleline(&mut hit.name).desired_width(70.0).hint_text(t(T::WsAttackName)));
                        row.add(egui::DragValue::new(&mut hit.bonus).range(-5..=20).prefix("+")).on_hover_text(t(T::WsBonusHint));
                        row.add(egui::TextEdit::singleline(&mut hit.damage).desired_width(50.0).hint_text("1d6+1")).on_hover_text(t(T::WsDamage));
                        row.add(egui::DragValue::new(&mut hit.range).range(5..=600).speed(5).suffix(" ft")).on_hover_text(t(T::WsRangeHint));
                        if row.add(button("×")).clicked() {
                            gone = Some(n);
                        }
                    });
                }
                if let Some(n) = gone {
                    ws.hits.remove(n);
                }
                if ui.add(button(t(T::WsAddAttack))).clicked() {
                    ws.hits.push(Hit::default());
                }
            });
            egui::CollapsingHeader::new(dark(t(T::WsTrade))).default_open(ws.merchant).show(ui, |ui| {
                kit::toggle(ui, t(T::WsMerchant), &mut ws.merchant);
                if ws.merchant {
                    wares_ui(ui, &mut ws.wares, "shop", lang);
                }
            });
        }
    }
    if ws.kind == MakeKind::Item {
        return;
    }
    ui.add_space(6.0);
    let ready = !ws.id.trim().is_empty() && ws.grid.iter().any(|i| *i != 0);
    ui.horizontal(|row| {
        if row.add(button(t(T::WsSave)).primary().enabled(ready, t(T::WsNeedId))).clicked() {
            save(ws, world, acts);
        }
        if row.add(button(t(T::WsNew))).on_hover_text(t(T::WsNewHint)).clicked() {
            let (mode, kind, loaded, arts, made) = (ws.mode, ws.kind, ws.loaded, std::mem::take(&mut ws.arts), std::mem::take(&mut ws.made));
            *ws = Workshop { mode, kind, loaded, arts, made, ..Workshop::default() };
        }
    });
}

/// A thing for the bag: what it is and its numbers, and where it goes — to a hero now, or
/// into a chest or onto a merchant's shelf to be found later.
fn item_properties(ui: &mut Ui, ws: &mut Workshop, world: &World, acts: &mut Vec<GmAct>, lang: Lang) {
    let t = |text: T| text.text(lang);
    let ru = lang == Lang::Ru;
    egui::CollapsingHeader::new(dark(t(T::WsItemWhat))).default_open(true).show(ui, |ui| {
        ui.horizontal(|row| {
            row.label(t(T::WsCells));
            row.add(egui::DragValue::new(&mut ws.cells.0).range(1..=4).suffix(" ×"));
            row.add(egui::DragValue::new(&mut ws.cells.1).range(1..=4));
            hint(row, t(T::WsCellsHint));
        });
        ui.horizontal(|row| {
            row.label(t(T::WsItemKind));
            egui::ComboBox::from_id_salt("item_kind").width(130.0).selected_text(ws.item_kind.clone()).show_ui(row, |ui| {
                for kind in KINDS {
                    ui.selectable_value(&mut ws.item_kind, kind.to_string(), kind);
                }
            });
        });
        if matches!(ws.item_kind.as_str(), "weapon" | "armor") {
            // What it works as by the rules: its own name and picture stay.
            let srd = rules::srd();
            let mut gear: Vec<(String, String)> = if ws.item_kind == "weapon" { srd.weapons.iter().map(|(i, w)| (i.clone(), w.name.clone())).collect() } else { srd.armors.iter().map(|(i, a)| (i.clone(), a.name.clone())).collect() };
            gear.sort_by(|a, b| a.1.cmp(&b.1));
            ui.horizontal(|row| {
                row.label(t(T::WsGear));
                let shown = gear.iter().find(|(i, _)| *i == ws.gear).map_or("—".to_string(), |(_, n)| rules::tr(ru, n).to_string());
                egui::ComboBox::from_id_salt("item_gear").width(150.0).height(240.0).selected_text(shown).show_ui(row, |ui| {
                    for (index, name) in &gear {
                        ui.selectable_value(&mut ws.gear, index.clone(), rules::tr(ru, name));
                    }
                });
                row.label("+");
                row.add(egui::DragValue::new(&mut ws.bonus).range(0..=3));
            });
        }
        egui::Grid::new("item_numbers").num_columns(4).spacing(vec2(6.0, 4.0)).show(ui, |grid| {
            grid.label(t(T::WsHeal));
            grid.add(egui::TextEdit::singleline(&mut ws.heal).desired_width(60.0).hint_text("2d4+2"));
            grid.label(t(T::WsDamage));
            grid.add(egui::TextEdit::singleline(&mut ws.damage).desired_width(60.0).hint_text("1d6"));
            grid.end_row();
            grid.label(t(T::WsLight));
            grid.add(egui::DragValue::new(&mut ws.light).range(0..=8));
            grid.label(t(T::WsPrice));
            grid.add(egui::DragValue::new(&mut ws.price).range(0..=100_000));
            grid.end_row();
        });
        ui.horizontal(|row| {
            row.label(t(T::WsRarity));
            egui::ComboBox::from_id_salt("item_rarity").width(120.0).selected_text(if ws.rarity.is_empty() { "common" } else { ws.rarity.as_str() }).show_ui(row, |ui| {
                for rarity in ["", "uncommon", "rare", "legendary"] {
                    ui.selectable_value(&mut ws.rarity, rarity.to_string(), if rarity.is_empty() { "common" } else { rarity });
                }
            });
        });
    });
    let ready = !ws.name.trim().is_empty() && ws.grid.iter().any(|i| *i != 0);
    let thing = {
        let mut draft = serde_json::json!({ "name": ws.name.trim(), "description": ws.description.trim(), "kind": ws.item_kind, "size": [ws.cells.0, ws.cells.1], "rows": ws.rows(), "heal": ws.heal.trim(), "damage": ws.damage.trim(), "price": ws.price, "rarity": ws.rarity });
        if !ws.gear.is_empty() && matches!(ws.item_kind.as_str(), "weapon" | "armor") {
            draft["gear"] = ws.gear.clone().into();
            draft["bonus"] = ws.bonus.into();
        }
        if ws.light > 0 {
            draft["light"] = ws.light.into();
        }
        draft
    };
    egui::CollapsingHeader::new(dark(t(T::WsItemWhere))).default_open(true).show(ui, |ui| {
        ui.horizontal(|row| {
            let heroes: Vec<String> = world.party.iter().map(|h| h.name.clone()).collect();
            egui::ComboBox::from_id_salt("give_to").width(120.0).selected_text(if ws.give_to.is_empty() { "—".to_string() } else { ws.give_to.clone() }).show_ui(row, |ui| {
                for hero in &heroes {
                    ui.selectable_value(&mut ws.give_to, hero.clone(), hero.as_str());
                }
            });
            if row.add(button(t(T::WsGiveNow)).primary().enabled(ready && !ws.give_to.is_empty(), t(T::WsNeedItem))).clicked() {
                let mut effect = thing.clone();
                effect["op"] = "give".into();
                effect["to"] = ws.give_to.clone().into();
                acts.push(GmAct::Apply(Box::new(TurnDraft { effects: vec![effect], ..TurnDraft::default() })));
            }
        });
        ui.horizontal(|row| {
            let here = world.here();
            let holders: Vec<(String, String)> = here.props.iter().map(|p| (p.id.clone(), p.name.clone())).chain(here.creatures.iter().filter(|c| !c.stock.is_empty()).map(|c| (c.id.clone(), c.name.clone()))).collect();
            let shown = holders.iter().find(|(id, _)| *id == ws.stash_in).map_or("—".to_string(), |(_, n)| n.clone());
            egui::ComboBox::from_id_salt("stash_in").width(120.0).selected_text(shown).show_ui(row, |ui| {
                for (id, name) in &holders {
                    ui.selectable_value(&mut ws.stash_in, id.clone(), name.as_str());
                }
            });
            if row.add(button(t(T::WsStashNow)).enabled(ready && !ws.stash_in.is_empty(), t(T::WsNeedItem))).on_hover_text(t(T::WsStashHint)).clicked() {
                let mut effect = thing.clone();
                effect["op"] = "stash".into();
                effect["id"] = ws.stash_in.clone().into();
                acts.push(GmAct::Apply(Box::new(TurnDraft { effects: vec![effect], ..TurnDraft::default() })));
            }
        });
    });
}

/// What was made, sent to be kept for good: a tile into the world and the library, a
/// creature or an object into the library, ready to be put on the map.
fn save(ws: &mut Workshop, world: &World, acts: &mut Vec<GmAct>) {
    let id = world::build::slug(&ws.id);
    let name = if ws.name.trim().is_empty() { id.replace('_', " ") } else { ws.name.trim().to_string() };
    let rows = ws.rows();
    match ws.kind {
        MakeKind::Tile => acts.push(GmAct::SaveArt { tile: true, id, name, walkable: ws.walkable, rows }),
        kind => {
            // The picture with the colours of this world, so it looks the same anywhere.
            let mut colors: Vec<(char, [u8; 3])> = Vec::new();
            for &index in ws.grid.iter().filter(|i| **i != 0) {
                let ch = world::model::char_of(index);
                if !colors.iter().any(|(c, _)| *c == ch) {
                    let [r, g, b, _] = world.palette.rgba(index);
                    colors.push((ch, [r, g, b]));
                }
            }
            let art = Art { id: id.clone(), name: name.clone(), tile: false, walkable: false, rows, colors };
            let mut spawn = ws.spawn();
            spawn["name"] = name.clone().into();
            let entity = Entity { id: id.clone(), name, creature: kind == MakeKind::Creature, spawn, art };
            match ws.made.iter().position(|e| e.id == id) {
                Some(n) => ws.made[n] = entity.clone(),
                None => ws.made.push(entity.clone()),
            }
            acts.push(GmAct::SaveEntity(Box::new(entity)));
        }
    }
}

/// Where a shelf of the library comes from.
fn source_name(n: usize, saves: &[(String, String, std::path::PathBuf)], lang: Lang) -> String {
    let ru = lang == Lang::Ru;
    match n {
        0 => (if ru { "Эта игра" } else { "This game" }).to_string(),
        1 => (if ru { "Моя библиотека" } else { "My library" }).to_string(),
        2 => (if ru { "Стартовый набор" } else { "Starter pack" }).to_string(),
        3 => (if ru { "★ Избранное" } else { "★ Favourites" }).to_string(),
        n => saves.get(n - 4).map_or_else(String::new, |(title, place, _)| format!("{title} · {place}")),
    }
}

/// The shelf of a source: its tiles and pictures, its creatures and things.
fn shelf_of(n: usize, ws: &Workshop, world: &World, store: &Store) -> (Vec<Art>, Vec<Entity>) {
    match n {
        0 => world::library::harvest(world),
        1 => (store.load_arts(), store.load_entities()),
        2 => world::library::starter(),
        3 => (ws.favorites.arts.clone(), ws.favorites.entities.clone()),
        n => ws.saves.get(n - 4).and_then(|(_, _, path)| store.load_game(path).ok()).map(|(_, w)| world::library::harvest(&w)).unwrap_or_default(),
    }
}

#[allow(clippy::too_many_arguments)]
fn library_room(ctx: &egui::Context, ws: &mut Workshop, world: &World, pictures: &ShelfPictures, acts: &mut Vec<GmAct>, store: &Store, lang: Lang, top: f32, screen: Rect) {
    let t = |text: T| text.text(lang);
    let ru = lang == Lang::Ru;
    let mut panels = std::mem::take(&mut ws.panels);
    let size = vec2(640.0_f32.min(screen.width() - 40.0), (screen.height() - top - 60.0).min(480.0));
    window(ctx, &mut panels, "library", t(T::WsLibrary), egui::pos2((screen.width() - size.x) / 2.0, top), size, |ui| {
        ui.horizontal_top(|row| {
            // The sources on the left: this game, the library, the starter pack, the
            // favourites, and every world saved on this computer.
            row.vertical(|col| {
                col.set_width(170.0);
                egui::ScrollArea::vertical().id_salt("sources").max_height(size.y - 40.0).show(col, |col| {
                    for n in 0..4 + ws.saves.len() {
                        if n == 4 {
                            col.label(egui::RichText::new(if ru { "Сохранения:" } else { "Saves:" }).strong());
                        }
                        let name = source_name(n, &ws.saves, lang);
                        if col.add_sized([165.0, 20.0], egui::Button::selectable(ws.source == n, name)).clicked() {
                            ws.source = n;
                            ws.shelf = None;
                            ws.picked = None;
                        }
                    }
                    if col.add(button(t(T::WsRefresh))).clicked() {
                        ws.saves.clear();
                        ws.saves_read = false;
                        ws.shelf = None;
                        ws.arts = store.load_arts();
                        ws.made = store.load_entities();
                    }
                });
            });
            row.separator();
            row.vertical(|col| {
                let filters = [T::WsAll, T::WsCreatures, T::WsObjects, T::WsTilesAndArt];
                col.horizontal(|row| {
                    for (n, label) in filters.iter().enumerate() {
                        if lit(row, t(*label), ws.filter == n, "") {
                            ws.filter = n;
                        }
                    }
                });
                let Some((arts, entities)) = ws.shelf.clone() else { return };
                egui::ScrollArea::vertical().id_salt("shelf").max_height(size.y - 140.0).auto_shrink([false, true]).show(col, |col| {
                    col.horizontal_wrapped(|row| {
                        for (n, entity) in entities.iter().enumerate() {
                            if ws.filter == 3 || (ws.filter == 1 && !entity.creature) || (ws.filter == 2 && entity.creature) {
                                continue;
                            }
                            let star = ws.favorites.entities.iter().any(|e| e.id == entity.id);
                            let mark = format!("{}{}", if star { "★" } else { "" }, if entity.creature { "☺" } else { "▣" });
                            if card(row, pictures.entities[n], ws.picked == Some((true, n)), &entity.name, &mark, 60.0) {
                                ws.picked = Some((true, n));
                            }
                        }
                        if ws.filter == 0 || ws.filter == 3 {
                            for (n, art) in arts.iter().enumerate() {
                                let star = ws.favorites.arts.iter().any(|a| a.id == art.id && a.tile == art.tile);
                                let mark = format!("{}{}", if star { "★" } else { "" }, if art.tile { if art.walkable { "○" } else { "×" } } else { "☺" });
                                if card(row, pictures.arts[n], ws.picked == Some((false, n)), &art.name, &mark, 60.0) {
                                    ws.picked = Some((false, n));
                                }
                            }
                        }
                    });
                    if entities.is_empty() && arts.is_empty() {
                        hint(col, t(T::WsNothingMade));
                    }
                });
                col.separator();
                match ws.picked {
                    Some((true, n)) if n < entities.len() => {
                        let entity = entities[n].clone();
                        let kind = if entity.creature { t(T::WsCreature) } else { t(T::WsObject) };
                        col.label(egui::RichText::new(format!("{} — {kind}", entity.name)).strong());
                        let s = &entity.spawn;
                        let mut facts = Vec::new();
                        if let Some(a) = s["attitude"].as_str() {
                            facts.push(a.to_string());
                        }
                        if let Some(m) = s["monster"].as_str() {
                            facts.push(m.to_string());
                        }
                        if s["stats"].is_object() {
                            facts.push(format!("{} {} · {} {}", t(T::WsAc), s["stats"]["ac"], t(T::WsHp), s["stats"]["hp"]));
                        }
                        if let Some(n) = s["actions"].as_array().filter(|a| !a.is_empty()) {
                            facts.push(format!("{}: {}", if ru { "действий" } else { "actions" }, n.len()));
                        }
                        hint(col, &facts.join(" · "));
                        col.horizontal(|row| {
                            if row.add(button(t(T::WsPutOnMap)).primary()).on_hover_text(t(T::WsPlaceHint)).clicked() {
                                ws.placing = Some(entity.clone());
                                ws.tool = MapTool::Place;
                                ws.mode = Mode::Map;
                            }
                            if row.add(button(t(T::WsEdit))).clicked() {
                                ws.edit(&entity, world);
                                ws.mode = Mode::Make;
                            }
                            let star = ws.favorites.entities.iter().any(|e| e.id == entity.id);
                            if row.add(button(if star { t(T::WsUnstar) } else { t(T::WsStar) })).clicked() {
                                if star {
                                    ws.favorites.entities.retain(|e| e.id != entity.id);
                                } else {
                                    ws.favorites.entities.push(entity.clone());
                                }
                                let _ = store.save_favorites(&ws.favorites);
                            }
                        });
                    }
                    Some((false, n)) if n < arts.len() => {
                        let art = arts[n].clone();
                        let kind = if art.tile { t(T::WsTile) } else { t(T::WsSprite) };
                        col.label(egui::RichText::new(format!("{} — {kind}", art.name)).strong());
                        col.horizontal(|row| {
                            if row.add(button(t(T::WsToWorld)).primary()).on_hover_text(t(T::WsToWorldHint)).clicked() {
                                acts.push(GmAct::ImportArt(art.clone()));
                                if art.tile {
                                    ws.tile = world::build::slug(&art.id);
                                }
                            }
                            if row.add(button(t(T::WsEdit))).clicked() {
                                let rows = art.rows.clone();
                                ws.kind = if art.tile { MakeKind::Tile } else { MakeKind::Creature };
                                ws.id = art.id.clone();
                                ws.name = art.name.clone();
                                ws.walkable = art.walkable;
                                let entity = Entity { id: art.id.clone(), name: art.name.clone(), creature: !art.tile, spawn: serde_json::json!({}), art: art.clone() };
                                ws.edit(&entity, world);
                                ws.take(&rows);
                                if art.tile {
                                    ws.kind = MakeKind::Tile;
                                }
                                ws.mode = Mode::Make;
                            }
                            let star = ws.favorites.arts.iter().any(|a| a.id == art.id && a.tile == art.tile);
                            if row.add(button(if star { t(T::WsUnstar) } else { t(T::WsStar) })).clicked() {
                                if star {
                                    ws.favorites.arts.retain(|a| !(a.id == art.id && a.tile == art.tile));
                                } else {
                                    ws.favorites.arts.push(art.clone());
                                }
                                let _ = store.save_favorites(&ws.favorites);
                            }
                        });
                    }
                    _ => {
                        hint(col, t(T::WsLibraryHint));
                    }
                }
            });
        });
    });
    ws.panels = panels;
}

/// The pictures of the shelf shown, made once each.
#[derive(Default)]
struct ShelfPictures {
    arts: Vec<egui::TextureId>,
    entities: Vec<egui::TextureId>,
}

/// What a press in the ring does: open a form of an effect filled for the thing, do an
/// effect at once, or pick the thing up to set it down elsewhere.
enum Press {
    Form(&'static str, Vec<(&'static str, String)>),
    Now(serde_json::Value),
    Carry,
    Place,
}

/// The ring of what the master can do with what they clicked, around it, as in a life
/// simulation game, in the parchment of the game; and the page opened from it.
fn ring(ctx: &egui::Context, ws: &mut Workshop, world: &World, acts: &mut Vec<GmAct>, lang: Lang, screen: Rect) {
    let ru = lang == Lang::Ru;
    let Some(focus) = ws.focus.clone() else { return };
    let here = world.here();
    let cell = format!("{}, {}", focus.cell.0, focus.cell.1);
    let creature = focus.id.as_ref().and_then(|id| here.creatures.iter().find(|c| c.id == *id));
    let prop = focus.id.as_ref().and_then(|id| here.props.iter().find(|p| p.id == *id));
    let tile = here.tile_id(focus.cell.0, focus.cell.1).unwrap_or("").to_string();
    let tile_name = world.tiles.get(&tile).map_or(tile.clone(), |t| t.name.clone());
    let l = |r: &'static str, e: &'static str| if ru { r } else { e };
    let mut items: Vec<(&'static str, Press)> = Vec::new();
    let title;
    if let Some(c) = creature {
        title = c.name.clone();
        let id = c.id.clone();
        let stats = c.stats.clone();
        items.push((l("Реплики", "Lines"), Press::Form("lines", vec![("id", id.clone()), ("lines", c.lines.join("\n")), ("every", if c.every > 0 { c.every.to_string() } else { "20".into() }), ("order", if c.random_lines { "random".into() } else { "in_turn".into() })])));
        items.push((l("Сказать", "Say"), Press::Form("say", vec![("id", id.clone())])));
        items.push((l("Характеристики", "Numbers"), Press::Form("edit", vec![("id", id.clone()), ("name", c.name.clone()), ("size", c.size.to_string()), ("ac", stats.as_ref().map_or(String::new(), |s| s.ac.to_string())), ("hp", stats.as_ref().map_or(String::new(), |s| s.hp.to_string())), ("speed", stats.as_ref().map_or(String::new(), |s| s.speed.to_string())), ("monster", c.monster.clone().unwrap_or_default())])));
        let attitude = match c.attitude {
            world::Attitude::Hostile => "hostile",
            world::Attitude::Neutral => "neutral",
            _ => "friendly",
        };
        items.push((l("Отношение", "Attitude"), Press::Form("attitude", vec![("id", id.clone()), ("attitude", attitude.into())])));
        items.push((l("Действие", "Action"), Press::Form("action", vec![("id", id.clone())])));
        items.push((l("Урон", "Damage"), Press::Form("damage", vec![("id", id.clone()), ("amount", "1d6".into())])));
        items.push((l("Бой", "Fight"), Press::Now(serde_json::json!({ "op": "fight", "with": [id.clone()] }))));
        items.push((l("Убить", "Kill"), Press::Now(serde_json::json!({ "op": "kill", "id": id.clone() }))));
        items.push((l("В отряд", "Join"), Press::Now(serde_json::json!({ "op": "join", "id": id.clone() }))));
        items.push((l("Описание", "Description"), Press::Form("describe", vec![("id", id.clone()), ("description", c.description.clone())])));
        items.push((l("Двигать", "Move"), Press::Carry));
        items.push((l("Убрать", "Remove"), Press::Now(serde_json::json!({ "op": "remove", "id": id }))));
    } else if let Some(p) = prop {
        title = p.name.clone();
        let id = p.id.clone();
        items.push((l("Действие", "Action"), Press::Form("action", vec![("id", id.clone())])));
        items.push((l("Изменить", "Change"), Press::Form("edit", vec![("id", id.clone()), ("name", p.name.clone()), ("size", p.size.to_string()), ("blocking", p.blocking.to_string())])));
        items.push((l("Описание", "Description"), Press::Form("describe", vec![("id", id.clone()), ("description", p.description.clone())])));
        items.push((l("Сказать", "Say"), Press::Form("say", vec![("id", id.clone())])));
        items.push((l("Двигать", "Move"), Press::Carry));
        items.push((l("Убрать", "Remove"), Press::Now(serde_json::json!({ "op": "remove", "id": id }))));
    } else {
        title = format!("{tile_name} ({cell})");
        items.push((l("Сменить землю", "Ground"), Press::Form("tile", vec![("cell", cell.clone()), ("tile", tile.clone())])));
        items.push((l("Плита", "Plate"), Press::Form("secret", vec![("cell", cell.clone()), ("kind", "plate".into()), ("dc", "0".into())])));
        items.push((l("Ловушка", "Trap"), Press::Form("secret", vec![("cell", cell.clone()), ("kind", "trap".into()), ("dc", "13".into())])));
        items.push((l("Тайник", "Cache"), Press::Form("secret", vec![("cell", cell.clone()), ("kind", "hidden".into()), ("dc", "14".into())])));
        items.push((l("Выход", "Way out"), Press::Form("exit", vec![("cell", cell.clone())])));
        items.push((l("Создать", "Spawn"), Press::Form("spawn", vec![("cell", cell.clone())])));
        items.push((l("Из библиотеки", "From library"), Press::Place));
        items.push((l("След", "Mark"), Press::Form("mark", vec![("cell", cell.clone())])));
    }
    // The ring: the name in the middle, the presses around it.
    let radius = 78.0 + items.len() as f32 * 2.0;
    let side = vec2(radius * 2.0 + 120.0, radius * 2.0 + 40.0);
    let origin = egui::pos2((focus.at.x - side.x / 2.0).clamp(0.0, (screen.width() - side.x).max(0.0)), (focus.at.y - side.y / 2.0).clamp(0.0, (screen.height() - side.y).max(0.0)));
    let center = origin + side / 2.0;
    let mut pressed = None;
    let mut close = false;
    let shown = egui::Area::new(egui::Id::new("master_ring")).order(egui::Order::Foreground).fixed_pos(origin).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(side, Sense::hover());
        ui.painter().circle_filled(center, radius * 0.55, c(PARCHMENT).gamma_multiply(0.85));
        ui.painter().circle_stroke(center, radius * 0.55, egui::Stroke::new(2.0, c(BRASS_DARK)));
        let name = ui.painter().layout(title.clone(), egui::FontId::proportional(12.0), c(INK), radius);
        ui.painter().galley(center - name.size() / 2.0 - vec2(0.0, 8.0), name, c(INK));
        let close_rect = Rect::from_center_size(center + vec2(0.0, 14.0), vec2(26.0, 18.0));
        if ui.put(close_rect, button("×")).clicked() {
            close = true;
        }
        for (n, (label, _)) in items.iter().enumerate() {
            let angle = -std::f32::consts::FRAC_PI_2 + n as f32 / items.len() as f32 * std::f32::consts::TAU;
            let at = center + vec2(angle.cos() * radius, angle.sin() * radius * 0.8);
            let width = label.chars().count() as f32 * 7.0 + 16.0;
            if ui.put(Rect::from_center_size(at, vec2(width, 20.0)), button(*label)).clicked() {
                pressed = Some(n);
            }
        }
        let _ = rect;
    });
    ws.panels.push(shown.response.rect);
    if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ws.focus = None;
        ws.menu_page = None;
        return;
    }
    if let Some(n) = pressed {
        let (label, press) = items.swap_remove(n);
        match press {
            Press::Now(effect) => {
                acts.push(GmAct::Apply(Box::new(TurnDraft { effects: vec![effect], ..TurnDraft::default() })));
                ws.focus = None;
            }
            Press::Carry => {
                ws.carry = focus.id.clone();
                ws.focus = None;
            }
            Press::Place => {
                ws.mode = Mode::Map;
                ws.tool = MapTool::Place;
                ws.focus = None;
            }
            Press::Form(op, values) => {
                if let Some(n) = super::effects_ui::OPS.iter().position(|o| o.op == op) {
                    ws.ring_form = super::effects_ui::Form { op: n, values: values.into_iter().collect(), open: true, fixed: true, subs: Default::default() };
                    ws.script.clear();
                    ws.menu_page = Some((op, format!("{title}: {label}")));
                }
            }
        }
    }
    // The page opened from the ring, beside it.
    let Some((op, page_title)) = ws.menu_page.clone() else { return };
    let mut panels = std::mem::take(&mut ws.panels);
    let size = vec2(360.0, 360.0);
    let at = egui::pos2(if origin.x + side.x + size.x < screen.width() { origin.x + side.x } else { (origin.x - size.x).max(0.0) }, origin.y);
    let with_steps = matches!(op, "action" | "secret");
    let mut done = false;
    window(ctx, &mut panels, "ring_page", &page_title, at, size, |ui| {
        egui::ScrollArea::vertical().max_height(size.y - 40.0).auto_shrink([false, true]).show(ui, |ui| {
            super::effects_ui::builder(ui, &mut ws.ring_form, world, lang);
            if with_steps {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(if ru { "Что при этом происходит — шаги по порядку:" } else { "What happens then, step by step:" }).strong().color(c(WAX_DARK)));
                let about = format!("{page_title} (id {})", focus.id.clone().unwrap_or_else(|| cell.clone()));
                if let Some(words) = super::effects_ui::script_list(ui, &mut ws.script, &mut ws.script_form, &mut ws.script_words, ws.script_asking, world, lang) {
                    ws.script_asking = true;
                    acts.push(GmAct::AskScript { about, wish: words });
                }
            }
        });
        ui.separator();
        ui.horizontal(|row| {
            if row.add(button(if ru { "Отмена" } else { "Cancel" })).clicked() {
                done = true;
            }
            if kit::right(row, |r| r.add(button(if ru { "Применить" } else { "Apply" }).primary())).clicked() {
                let mut effect = super::effects_ui::build(&super::effects_ui::OPS[ws.ring_form.op], &ws.ring_form.values);
                if with_steps {
                    effect["effects"] = serde_json::Value::Array(ws.script.clone());
                }
                acts.push(GmAct::Apply(Box::new(TurnDraft { effects: vec![effect], ..TurnDraft::default() })));
                done = true;
            }
        });
    });
    ws.panels.extend(panels);
    if done {
        ws.menu_page = None;
        ws.focus = None;
        ws.script.clear();
    }
}

/// Running the game: telling, changing the world, the AI, the seat — one page at a time.
#[allow(clippy::too_many_arguments)]
fn run_room(ctx: &egui::Context, ws: &mut Workshop, world: &World, table: &mut Table, acts: &mut Vec<GmAct>, phase: &mut Phase, lang: Lang, top: f32, screen: Rect, me: &str) {
    let t = |text: T| text.text(lang);
    let mut panels = std::mem::take(&mut ws.panels);
    let size = vec2(440.0_f32.min(screen.width() - 40.0), 300.0);
    window(ctx, &mut panels, "run", t(T::WsRun), egui::pos2(8.0, (screen.height() - size.y - 60.0).max(top)), size, |ui| {
        ui.horizontal(|row| {
            for (n, (label, about)) in [(T::WsTell, T::WsTellAbout), (T::WsChange, T::WsChangeAbout), (T::WsScripts, T::WsScriptsAbout), (T::WsAi, T::WsAiAbout), (T::WsSeat, T::WsSeatAbout)].into_iter().enumerate() {
                if lit(row, t(label), ws.page == n, t(about)) {
                    ws.page = n;
                }
            }
        });
        ui.separator();
        let here = world.here();
        match ws.page {
            0 => {
                // Told to everyone, or whispered to one player.
                let others: Vec<String> = world.present.iter().filter(|p| p.as_str() != me).cloned().collect();
                ui.horizontal(|row| {
                    row.label(t(T::WsTellTo));
                    let shown = if ws.telling_to.is_empty() { t(T::WsEveryone).to_string() } else { ws.telling_to.clone() };
                    egui::ComboBox::from_id_salt("tell_to").width(160.0).selected_text(shown).show_ui(row, |ui| {
                        ui.selectable_value(&mut ws.telling_to, String::new(), t(T::WsEveryone));
                        for player in &others {
                            ui.selectable_value(&mut ws.telling_to, player.clone(), format!("{player} ({})", t(T::WsSecretly)));
                        }
                    });
                });
                ui.add(egui::TextEdit::multiline(&mut ws.telling).desired_rows(3).desired_width(f32::INFINITY).hint_text(t(T::WsTellHint)));
                if ui.add(button(t(T::WsTellSend)).primary().enabled(!ws.telling.trim().is_empty(), t(T::NeedText))).clicked() {
                    let text = std::mem::take(&mut ws.telling).trim().to_string();
                    acts.push(if ws.telling_to.is_empty() { GmAct::Apply(Box::new(TurnDraft { narration: text, ..TurnDraft::default() })) } else { GmAct::Whisper { to: ws.telling_to.clone(), text } });
                }
                heading(ui, t(T::WsSpeak));
                ui.horizontal(|row| {
                    let things: Vec<(String, String)> = here.creatures.iter().map(|c| (c.id.clone(), c.name.clone())).chain(here.props.iter().map(|p| (p.id.clone(), p.name.clone()))).collect();
                    let shown = things.iter().find(|(id, _)| *id == ws.speaker).map_or("—".to_string(), |(_, n)| n.clone());
                    egui::ComboBox::from_id_salt("speaker").width(120.0).selected_text(shown).show_ui(row, |ui| {
                        for (id, name) in &things {
                            ui.selectable_value(&mut ws.speaker, id.clone(), name.as_str());
                        }
                    });
                    row.add(egui::TextEdit::singleline(&mut ws.line).desired_width(170.0).hint_text(t(T::WsLineHint)));
                    if row.add(button(t(T::WsSay)).enabled(!ws.speaker.is_empty() && !ws.line.trim().is_empty(), t(T::NeedText))).clicked() {
                        acts.push(GmAct::Say { who: ws.speaker.clone(), text: std::mem::take(&mut ws.line) });
                    }
                });
            }
            1 => {
                // The commonest changes a click away; every other in the list below.
                hint(ui, t(T::WsChangeHint));
                ui.horizontal_wrapped(|row| {
                    for op in ["damage", "heal", "give", "gold", "xp", "check", "spawn", "fight", "time", "weather", "light", "shake"] {
                        if let Some((n, entry)) = super::effects_ui::OPS.iter().enumerate().find(|(_, o)| o.op == op)
                            && row.add(button(if lang == Lang::Ru { entry.ru } else { entry.en })).clicked()
                        {
                            ws.effect.op = n;
                            ws.effect.values.clear();
                            ws.effect.open = true;
                        }
                    }
                });
                egui::ScrollArea::vertical().max_height(170.0).auto_shrink([false, true]).show(ui, |ui| {
                    if let Some(effect) = super::effects_ui::builder(ui, &mut ws.effect, world, lang) {
                        acts.push(GmAct::Apply(Box::new(TurnDraft { effects: vec![effect], ..TurnDraft::default() })));
                    }
                });
                ui.horizontal(|row| {
                    if row.add(button(t(T::WsSecretRoll))).on_hover_text(t(T::WsSecretRollHint)).clicked() {
                        let roll = Roll::d20(&mut Rng::from_clock(), 0, Edge::Plain);
                        table.master_note = format!("d20: {}", roll.total());
                    }
                    if !table.master_note.is_empty() {
                        row.label(egui::RichText::new(&table.master_note).strong().color(c(WAX_DARK)));
                    }
                });
            }
            2 => {
                // The world's scripts: what sets each off, run now or struck out; a new one
                // is put together below, or written by the AI from words.
                egui::ScrollArea::vertical().max_height(200.0).auto_shrink([false, true]).show(ui, |ui| {
                    if world.scripts.is_empty() {
                        hint(ui, t(T::WsNoScripts));
                    }
                    for (id, script) in &world.scripts {
                        ui.horizontal(|row| {
                            let when = match script.on.as_str() {
                                "enter" => format!("{} {}", t(T::WsOnEnter), script.place),
                                "death" => format!("{} {}", t(T::WsOnDeath), script.of),
                                "hour" => format!("{} {}:00", t(T::WsOnHour), script.at),
                                "var" => format!("{} ≥ {}", script.var, script.at),
                                _ => t(T::WsOnCall).to_string(),
                            };
                            row.label(egui::RichText::new(format!("{id} — {when} — {} {}", script.effects.len(), t(T::WsStepsCount))).color(c(INK)));
                            if row.add(button(t(T::WsRunNow))).clicked() {
                                acts.push(GmAct::Apply(Box::new(TurnDraft { effects: vec![serde_json::json!({ "op": "script", "id": id })], ..TurnDraft::default() })));
                            }
                            if row.add(button("×")).on_hover_text(t(T::WsStrike)).clicked() {
                                acts.push(GmAct::Apply(Box::new(TurnDraft { effects: vec![serde_json::json!({ "op": "define", "id": id, "effects": [] })], ..TurnDraft::default() })));
                            }
                        });
                    }
                    if !world.vars.is_empty() {
                        heading(ui, t(T::WsVars));
                        hint(ui, &world.vars.iter().map(|(k, v)| format!("{k} = {v}")).collect::<Vec<_>>().join(" · "));
                    }
                    if !world.later.is_empty() {
                        hint(ui, &format!("{}: {}", t(T::WsWaiting), world.later.len()));
                    }
                    heading(ui, t(T::WsNewScript));
                    if !ws.effect.open || super::effects_ui::OPS.get(ws.effect.op).is_none_or(|o| o.op != "define") {
                        if ui.add(button(t(T::WsNewScript))).clicked() {
                            ws.effect = super::effects_ui::Form { op: super::effects_ui::OPS.iter().position(|o| o.op == "define").unwrap_or(0), open: true, ..Default::default() };
                        }
                    } else if let Some(effect) = super::effects_ui::builder(ui, &mut ws.effect, world, lang) {
                        acts.push(GmAct::Apply(Box::new(TurnDraft { effects: vec![effect], ..TurnDraft::default() })));
                    }
                });
            }
            3 => {
                hint(ui, t(T::WsAiHint));
                ui.add(egui::TextEdit::multiline(&mut ws.order).desired_rows(3).desired_width(f32::INFINITY).hint_text(t(T::WsOrderHint)));
                let ready = !ws.order.trim().is_empty();
                ui.horizontal_wrapped(|row| {
                    if row.add(button(t(T::WsOrderSend)).primary().enabled(ready, t(T::NeedText))).on_hover_text(t(T::WsOrderSendHint)).clicked() {
                        acts.push(GmAct::Direct(std::mem::take(&mut ws.order)));
                    }
                    if row.add(button(t(T::RedrawHere)).enabled(ready, t(T::NeedText))).on_hover_text(t(T::WsRedrawHint)).clicked() {
                        acts.push(GmAct::Redraw(std::mem::take(&mut ws.order)));
                    }
                    if row.add(button(t(T::WsInvent)).enabled(ready, t(T::NeedText))).on_hover_text(t(T::WsInventHint)).clicked() {
                        acts.push(GmAct::Invent(std::mem::take(&mut ws.order)));
                    }
                });
                if !table.master_note.is_empty() {
                    hint(ui, &table.master_note);
                }
            }
            _ => {
                hint(ui, t(T::WsSeatHint));
                ui.horizontal(|row| {
                    if row.add(button(t(T::WsWalk))).on_hover_text(t(T::WsWalkHint)).clicked() {
                        *phase = Phase::Walking;
                    }
                    if row.add(button(t(T::WsGiveSeat))).on_hover_text(t(T::WsGiveSeatHint)).clicked() {
                        acts.push(GmAct::Seat(false));
                        *phase = Phase::Walking;
                    }
                });
            }
        }
    });
    ws.panels = panels;
}

/// The master's answer to what the players did: told in words, with choices, with a roll
/// the player makes, with whatever the answer changes. True once it is sent or refused.
fn reply_window(ctx: &egui::Context, ws: &mut Workshop, world: &World, view: &DraftView, acts: &mut Vec<GmAct>, lang: Lang, screen: Rect) -> bool {
    let t = |text: T| text.text(lang);
    // A new deed (or a new draft of the model's) starts the answer afresh.
    let key = (view.said.clone(), view.narration.clone());
    if ws.reply_to.as_ref() != Some(&key) {
        ws.reply = view.draft.clone();
        ws.reply_to = Some(key);
    }
    let mut done = false;
    let mut panels = std::mem::take(&mut ws.panels);
    let size = vec2(470.0_f32.min(screen.width() - 40.0), (screen.height() * 0.7).min(520.0));
    window(ctx, &mut panels, "reply", t(T::WsReply), egui::pos2((screen.width() - size.x) / 2.0, screen.height() - size.y - 40.0), size, |ui| {
        if !view.said.is_empty() {
            ui.label(egui::RichText::new(format!("«{}»", view.said)).strong().color(c(WAX_DARK)));
        }
        let mut ai = world.ai_drafts;
        if kit::toggle(ui, t(T::WsAiDrafts), &mut ai).changed() {
            acts.push(GmAct::AiDrafts(ai));
        }
        egui::ScrollArea::vertical().max_height(size.y - 120.0).auto_shrink([false, true]).show(ui, |ui| {
            heading(ui, t(T::WsTelling));
            ui.add(egui::TextEdit::multiline(&mut ws.reply.narration).desired_rows(4).desired_width(f32::INFINITY).hint_text(t(T::WsTellingHint)));
            // A roll: the player throws the die on their own screen and everyone sees it.
            egui::CollapsingHeader::new(dark(t(T::WsAskRoll))).default_open(true).show(ui, |ui| {
                let ru = lang == Lang::Ru;
                ui.horizontal(|row| {
                    let shown = rules::character::Ability::from_index(&ws.roll.0).map_or("—", |a| rules::tr(ru, a.name()));
                    egui::ComboBox::from_id_salt("roll_ability").width(110.0).selected_text(shown).show_ui(row, |ui| {
                        for ability in rules::character::Ability::ALL {
                            ui.selectable_value(&mut ws.roll.0, ability.index().to_string(), rules::tr(ru, ability.name()));
                        }
                    });
                    let skill = rules::srd().skills.iter().find(|s| s.index == ws.roll.1).map_or(t(T::WsNoSkill).to_string(), |s| rules::tr(lang == Lang::Ru, &s.name).to_string());
                    egui::ComboBox::from_id_salt("roll_skill").width(130.0).selected_text(skill).height(240.0).show_ui(row, |ui| {
                        ui.selectable_value(&mut ws.roll.1, String::new(), t(T::WsNoSkill));
                        for s in &rules::srd().skills {
                            ui.selectable_value(&mut ws.roll.1, s.index.clone(), rules::tr(lang == Lang::Ru, &s.name));
                        }
                    });
                    row.label(t(T::WsDc));
                    row.add(egui::DragValue::new(&mut ws.roll.2).range(1..=30));
                });
                hint(ui, t(T::WsDcHint));
                ui.add(egui::TextEdit::singleline(&mut ws.roll.3).desired_width(f32::INFINITY).hint_text(t(T::WsOnSuccess)));
                ui.add(egui::TextEdit::singleline(&mut ws.roll.4).desired_width(f32::INFINITY).hint_text(t(T::WsOnFailure)));
                if ui.add(button(t(T::WsAddRoll))).clicked() {
                    let skill = if ws.roll.1.is_empty() { serde_json::Value::Null } else { ws.roll.1.clone().into() };
                    ws.reply.effects.push(serde_json::json!({ "op": "check", "ability": ws.roll.0, "skill": skill, "dc": ws.roll.2, "success": { "narration": ws.roll.3.trim(), "effects": [] }, "failure": { "narration": ws.roll.4.trim(), "effects": [] } }));
                    ws.roll.3.clear();
                    ws.roll.4.clear();
                }
            });
            heading(ui, t(T::WsChoices));
            let heroes: Vec<String> = world.party.iter().map(|h| h.name.clone()).collect();
            let mut gone = None;
            for (n, option) in ws.reply.options.iter_mut().enumerate() {
                frame().fill(c(PARCHMENT_SHADE)).inner_margin(4.0).show(ui, |ui| {
                    ui.horizontal(|row| {
                        row.add(egui::TextEdit::singleline(&mut option.label).desired_width(250.0).hint_text(t(T::WsChoiceLabel)));
                        let shown = if option.for_hero.is_empty() { t(T::WsForAnyone).to_string() } else { option.for_hero.clone() };
                        egui::ComboBox::from_id_salt(("for", n)).width(90.0).selected_text(shown).show_ui(row, |ui| {
                            ui.selectable_value(&mut option.for_hero, String::new(), t(T::WsForAnyone));
                            for hero in &heroes {
                                ui.selectable_value(&mut option.for_hero, hero.clone(), hero.as_str());
                            }
                        });
                        if row.add(button("×")).clicked() {
                            gone = Some(n);
                        }
                    });
                    ui.add(egui::TextEdit::singleline(&mut option.outcome).desired_width(f32::INFINITY).hint_text(t(T::WsChoiceOutcome)));
                });
            }
            if let Some(n) = gone {
                ws.reply.options.remove(n);
            }
            if ws.reply.options.len() < 5 && ui.add(button(t(T::WsAddChoice))).clicked() {
                ws.reply.options.push(world::story::ActionDraft::default());
            }
            heading(ui, t(T::WsChanges));
            if ws.reply.effects.is_empty() {
                hint(ui, t(T::WsNoChanges));
            }
            let mut gone = None;
            for (n, effect) in ws.reply.effects.iter().enumerate() {
                ui.horizontal(|row| {
                    row.label(egui::RichText::new(super::effects_ui::describe(effect, world, lang)).color(c(INK)));
                    if row.add(button("×")).clicked() {
                        gone = Some(n);
                    }
                });
            }
            if let Some(n) = gone {
                ws.reply.effects.remove(n);
            }
            if let Some(effect) = super::effects_ui::builder(ui, &mut ws.effect, world, lang) {
                ws.reply.effects.push(effect);
            }
        });
        ui.separator();
        ui.horizontal(|row| {
            if row.add(button(t(T::Reject))).on_hover_text(t(T::WsRefuseHint)).clicked() {
                acts.push(GmAct::Verdict { accept: false, narration: String::new() });
                done = true;
            }
            if row.add(button(t(T::WsAiWrite))).on_hover_text(t(T::WsAiWriteHint)).clicked() {
                let hint_text = ws.reply.narration.trim();
                acts.push(GmAct::Direct(if hint_text.is_empty() { "Answer what the players did.".to_string() } else { format!("Answer what the players did, along these lines: {hint_text}") }));
                done = true;
            }
            let ready = !ws.reply.narration.trim().is_empty() || !ws.reply.effects.is_empty();
            if kit::right(row, |r| r.add(button(t(T::WsSendAnswer)).primary().enabled(ready, t(T::WsNeedTelling)))).clicked() {
                let mut reply = ws.reply.clone();
                reply.options.retain(|o| !o.label.trim().is_empty());
                acts.push(GmAct::Answer(Box::new(reply)));
                done = true;
            }
        });
    });
    ws.panels = panels;
    if done {
        ws.reply_to = None;
    }
    done
}

#[allow(clippy::too_many_arguments)]
fn rules_room(ctx: &egui::Context, ws: &mut Workshop, world: &World, table: &mut Table, acts: &mut Vec<GmAct>, lang: Lang, top: f32, screen: Rect) {
    let t = |text: T| text.text(lang);
    let mut panels = std::mem::take(&mut ws.panels);
    let book = super::rules_book::book(world, lang);
    let width = 420.0_f32.min(screen.width() / 2.0 - 20.0);
    window(ctx, &mut panels, "rules", t(T::WsRulesBook), egui::pos2(8.0, top), vec2(width, screen.height() - top - 40.0), |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            for (n, (title, body)) in book.iter().enumerate() {
                egui::CollapsingHeader::new(dark(title)).id_salt(("rule", n)).default_open(n == 0).show(ui, |ui| {
                    for line in body.lines() {
                        ui.label(line);
                    }
                });
            }
            egui::CollapsingHeader::new(dark(t(T::WsMasterGuide))).id_salt("guide").default_open(false).show(ui, |ui| {
                hint(ui, t(T::WsMasterGuideHint));
                ui.label(egui::RichText::new(world::prompt::play_prompt(world)).small());
            });
        });
    });
    // The answer that came for the question.
    if ws.asking
        && let Some(answer) = table.ruling.take()
    {
        ws.asking = false;
        ws.answer = match answer {
            Ok(text) => text,
            Err(trouble) => format!("{} {trouble}", t(T::WsNoRuling)),
        };
    }
    let side = 340.0_f32.min(screen.width() / 2.0 - 20.0);
    window(ctx, &mut panels, "ask_rules", t(T::WsAskRules), egui::pos2(screen.width() - side - 24.0, top), vec2(side, 380.0), |ui| {
        hint(ui, t(T::WsAskRulesHint));
        ui.add(egui::TextEdit::multiline(&mut ws.question).desired_rows(5).desired_width(f32::INFINITY).hint_text(t(T::WsQuestionHint)));
        ui.horizontal(|row| {
            let ready = !ws.question.trim().is_empty() && !ws.asking;
            let why = if ws.asking { t(T::WsThinking) } else { t(T::NeedText) };
            if row.add(button(t(T::WsAsk)).primary().enabled(ready, why)).clicked() {
                ws.asking = true;
                ws.answer.clear();
                table.ruling = None;
                acts.push(GmAct::AskRules { book: super::rules_book::text(world, lang), question: ws.question.trim().to_string() });
            }
            if row.add(button(t(T::WsClear))).clicked() {
                ws.question.clear();
                ws.answer.clear();
            }
        });
        if ws.asking {
            ui.horizontal(|row| {
                row.spinner();
                hint(row, t(T::WsThinking));
            });
        }
        if !ws.answer.is_empty() {
            heading(ui, t(T::WsRuling));
            egui::ScrollArea::vertical().max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
                ui.label(&ws.answer);
            });
        }
    });
    ws.panels = panels;
}

/// The map under the master's hand, while the workshop is open: painted, filled, cleared,
/// set with creatures and objects, and rearranged by dragging.
#[allow(clippy::too_many_arguments)]
pub(super) fn workshop_map(
    mut egui: EguiContexts,
    (phase, game, session, settings): (Res<Phase>, Res<Game>, Res<Session>, Res<Settings>),
    (mut ws, mut table): (ResMut<Workshop>, ResMut<Table>),
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    frame: Res<crate::view::View>,
) {
    if *phase != Phase::Master || !matches!(ws.mode, Mode::Map | Mode::Run) {
        return;
    }
    // Over the editor's windows the mouse is theirs, not the map's.
    let pointer = egui.ctx_mut().ok().and_then(|ctx| ctx.pointer_latest_pos());
    let over_window = pointer.is_some_and(|at| ws.panels.iter().any(|rect| rect.contains(at)));
    if over_window && !mouse.just_released(MouseButton::Left) {
        return;
    }
    let here = game.world.here();
    let (w, h) = here.size();
    let cell = crate::view::map_under_cursor(&window, &frame).and_then(|point| {
        let side = TILE as f32 * crate::scene::PX as f32;
        let (cx, cy) = ((point.x / side).floor() as i32, (point.y / side).floor() as i32);
        (cx >= 0 && cy >= 0 && cx < w as i32 && cy < h as i32).then_some((cx as usize, cy as usize))
    });
    let Some(cell) = cell else {
        ws.hovered.clear();
        return;
    };
    // What is under the mouse, for the line at the bottom.
    let ground = here.tile_id(cell.0, cell.1).and_then(|id| game.world.tiles.get(id).map(|def| format!("{} {}", def.name, if def.walkable { "○" } else { "×" }))).unwrap_or_default();
    let mut on_it: Vec<String> = here.creatures.iter().filter(|c| (c.x, c.y) == cell).map(|c| c.name.clone()).collect();
    on_it.extend(here.props.iter().filter(|p| (p.x, p.y) == cell).map(|p| p.name.clone()));
    ws.hovered = format!("({}, {}) {ground}{}{}", cell.0, cell.1, if on_it.is_empty() { "" } else { ": " }, on_it.join(", "));
    if let (Some(from), MapTool::Rect) = (ws.rect_from, ws.tool) {
        ws.hovered.push_str(&format!("  [{}×{}]", from.0.abs_diff(cell.0) + 1, from.1.abs_diff(cell.1) + 1));
    }
    let thing_at = |cell: (usize, usize)| here.thing_covering(cell.0, cell.1).map(str::to_string);
    let apply = |effects: Vec<serde_json::Value>| GmAct::Apply(Box::new(TurnDraft { effects, ..TurnDraft::default() }));
    // A click while running the game (a right click in the map room) opens the ring over
    // what is there; with something picked up from the ring, it is set down there instead.
    let ring_click = (ws.mode == Mode::Run && mouse.just_pressed(MouseButton::Left)) || mouse.just_pressed(MouseButton::Right);
    if ring_click && !over_window {
        match ws.carry.take() {
            Some(id) => send(apply(vec![serde_json::json!({ "op": "move", "id": id, "x": cell.0, "y": cell.1 })]), &session, &settings, &mut table),
            None => {
                ws.focus = Some(Focus { cell, id: thing_at(cell), at: pointer.unwrap_or_default() });
                ws.menu_page = None;
            }
        }
        return;
    }
    if ws.mode == Mode::Run {
        return;
    }
    let mut act = None;
    match ws.tool {
        MapTool::Brush if mouse.pressed(MouseButton::Left) && !ws.tile.is_empty() => {
            if ws.last_cell != Some(cell) {
                ws.last_cell = Some(cell);
                act = Some(apply(vec![serde_json::json!({ "op": "tile", "x": cell.0, "y": cell.1, "tile": ws.tile })]));
            }
        }
        MapTool::Rect if !ws.tile.is_empty() => {
            if mouse.just_pressed(MouseButton::Left) {
                ws.rect_from = Some(cell);
            } else if mouse.just_released(MouseButton::Left)
                && let Some(from) = ws.rect_from.take()
            {
                act = Some(apply(vec![serde_json::json!({ "op": "tile", "x": from.0, "y": from.1, "to": [cell.0, cell.1], "tile": ws.tile })]));
            }
        }
        MapTool::Erase if mouse.just_pressed(MouseButton::Left) => {
            if let Some(id) = thing_at(cell) {
                act = Some(apply(vec![serde_json::json!({ "op": "remove", "id": id })]));
            }
        }
        MapTool::Place if mouse.just_pressed(MouseButton::Left) => {
            if let Some(entity) = ws.placing.as_ref() {
                act = Some(GmAct::Place { entity: Box::new(entity.clone()), x: cell.0, y: cell.1 });
            }
        }
        MapTool::Move => {
            if mouse.just_pressed(MouseButton::Left) {
                ws.moving = thing_at(cell);
            } else if mouse.just_released(MouseButton::Left)
                && let Some(id) = ws.moving.take()
            {
                act = Some(apply(vec![serde_json::json!({ "op": "move", "id": id, "x": cell.0, "y": cell.1 })]));
            }
        }
        _ => {}
    }
    if !mouse.pressed(MouseButton::Left) {
        ws.last_cell = None;
    }
    if let Some(act) = act {
        send(act, &session, &settings, &mut table);
    }
}
