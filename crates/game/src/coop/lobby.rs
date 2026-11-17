//! The lobby, as games have it: the player's own table is open from the moment the screen
//! is, with a seat for everyone who comes, so friends are asked in rather than told an
//! address. At the left the party, seat by seat, with who is ready; at the right the tables
//! found on the network and through Steam, a field for a table's code, and the table's talk.
//! The host begins the game; the others say they are ready.

use super::*;
use crate::ui::egui::{Align, Layout, Rect, Sense, Stroke, Ui, UiBuilder};

/// What is typed on the lobby screen, and what it is looking at.
#[derive(Resource)]
struct Lobby {
    name: String,
    /// A table's code or address to join.
    typed: String,
    talk: String,
    /// Tables on the networks this machine is on, for as long as the screen is open.
    listening: Option<net::beacon::Listening>,
    /// The Steam friends' window, open.
    inviting: bool,
    /// The host asked to begin with no game yet: a new world or a saved one.
    choosing: bool,
    /// This guest said it is ready.
    ready: bool,
}

/// A table that can be joined, wherever it was found.
struct Found {
    name: String,
    /// Where it was found, for the player: the network's name or Steam.
    via: String,
    about: String,
    seats: String,
    join: Join,
    /// Of another version: shown, not joined.
    other_version: bool,
}

enum Join {
    Address(String),
    Steam(u64),
}

/// Steam is looked for, the network listened to, and the player's table opened as the
/// screen opens: friends can be asked in at once.
fn enter(mut commands: Commands, mut settings: ResMut<Settings>, store: Res<Store>, mut link: ResMut<SteamLink>, mut session: ResMut<Session>, mut table: ResMut<Table>) {
    if !link.tried {
        link.tried = true;
        match net::Steam::init() {
            Ok(steam) => {
                link.friends = steam.friends_hosting();
                link.online = steam.friends_online();
                link.steam = Some(steam);
            }
            Err(e) => info!("Steam is not available: {e}"),
        }
    }
    if let Some(steam) = &link.steam
        && settings.player_name.trim().is_empty()
    {
        settings.player_name = steam.name();
    } else if settings.player_name.trim().is_empty() {
        settings.player_name = computer_name();
    }
    if let Err(e) = store.save_settings(&settings) {
        error!("could not save settings: {e}");
    }
    // Alone and not on the way back to a table: the player's own table opens.
    if matches!(*session, Session::Solo) && table.rejoin.is_none() {
        open_table(&mut session, &mut table, &settings, &link);
    }
    commands.insert_resource(Lobby { name: player_name(&settings), typed: String::new(), talk: String::new(), listening: net::beacon::Listening::start().ok(), inviting: false, choosing: false, ready: false });
}

fn open_table(session: &mut Session, table: &mut Table, settings: &Settings, link: &SteamLink) {
    match host(link.steam.as_ref(), &player_name(settings), None) {
        Ok(hosting) => {
            *session = hosting;
            table.status = None;
        }
        Err(e) => {
            error!("could not host: {e}");
            table.status = Some(T::LobbyHostFailed);
        }
    }
}

/// The friends hosting and those online are looked up every few seconds while the lobby is open.
fn look(time: Res<Time>, mut link: ResMut<SteamLink>) {
    link.since_look += time.delta_secs();
    if link.since_look < FRIENDS_LOOK_PERIOD {
        return;
    }
    link.since_look = 0.0;
    if let Some(steam) = &link.steam {
        let hosting = steam.friends_hosting();
        let online = steam.friends_online();
        if hosting != link.friends {
            link.friends = hosting;
        }
        if online != link.online {
            link.online = online;
        }
    }
}

/// The tables there are to join: found on the network, and hosted by Steam friends.
fn found(lobby: &Lobby, link: &SteamLink, lang: Lang) -> Vec<Found> {
    let mut found = Vec::new();
    if let Some(listening) = &lobby.listening {
        let networks = net::beacon::networks();
        for heard in listening.tables() {
            // One's own table is not among those to join.
            if networks.iter().any(|n| n.address == *heard.address.ip()) && heard.info.name == lobby.name {
                continue;
            }
            let via = networks
                .iter()
                .find(|n| n.address.octets()[..2] == heard.address.ip().octets()[..2])
                .map_or(T::NetLocal.text(lang).to_string(), |n| match n.kind() {
                    net::beacon::NetworkKind::Radmin => "Radmin VPN".to_string(),
                    net::beacon::NetworkKind::ZeroTier => "ZeroTier".to_string(),
                    net::beacon::NetworkKind::Hamachi => "Hamachi".to_string(),
                    net::beacon::NetworkKind::Local => T::NetLocal.text(lang).to_string(),
                });
            let about = match (heard.info.world.is_empty(), heard.info.playing) {
                (true, _) => T::TableChoosing.text(lang).to_string(),
                (false, true) => format!("«{}» · {}", heard.info.world, T::TablePlaying.text(lang)),
                (false, false) => format!("«{}»", heard.info.world),
            };
            found.push(Found {
                name: heard.info.name.clone(),
                via,
                about,
                seats: format!("{}/{}", heard.info.taken, heard.info.seats),
                join: Join::Address(heard.address.to_string()),
                other_version: heard.info.protocol != PROTOCOL,
            });
        }
    }
    for (name, lobby_id) in &link.friends {
        found.push(Found { name: name.clone(), via: "Steam".to_string(), about: String::new(), seats: String::new(), join: Join::Steam(*lobby_id), other_version: false });
    }
    found
}

/// A seat at the table: a plate with the player's initial, the name, the hero, whether
/// ready, and for the host a button to take a guest off the table.
fn seat(ui: &mut Ui, seat: Option<&Seat>, me: &str, host: bool, lang: Lang) -> (bool, bool) {
    let t = |text: T| text.text(lang);
    let (mut kick, mut invite) = (false, false);
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 34.0), Sense::hover());
    let painter = ui.painter().clone();
    let mine = seat.is_some_and(|s| s.name == me);
    kit::plate(&painter, rect, if mine { kit::Look::Brass } else { kit::Look::Wood }, false);
    let face = Rect::from_min_size(rect.min + vec2(4.0, 4.0), vec2(26.0, 26.0));
    let mut inner = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_max(rect.min + vec2(36.0, 2.0), rect.max - vec2(4.0, 2.0))).layout(Layout::left_to_right(Align::Center)));
    match seat {
        Some(seat) => {
            painter.rect_filled(face, 0.0, c(WOOD_DEEP));
            painter.rect_stroke(face, 0.0, Stroke::new(1.0, c(if seat.ready { MOSS } else { WOOD_LIGHT })), egui::StrokeKind::Inside);
            let initial: String = seat.name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            painter.text(face.center(), egui::Align2::CENTER_CENTER, initial, egui::FontId::proportional(14.0), c(PARCHMENT));
            let ink = if mine { INK } else { PARCHMENT };
            inner.vertical(|col| {
                col.spacing_mut().item_spacing.y = 1.0;
                let crown = if seat.host { "★ " } else { "" };
                col.label(RichText::new(format!("{crown}{}", seat.name)).color(c(ink)));
                let hero = if seat.hero.is_empty() { t(T::LobbyNoHero).to_string() } else { seat.hero.clone() };
                col.label(RichText::new(hero).color(c(if mine { WAX_DARK } else { TALLOW })));
            });
            inner.with_layout(Layout::right_to_left(Align::Center), |row| {
                if host && !seat.host && row.add(button("×")).on_hover_text(t(T::KickHint)).clicked() {
                    kick = true;
                }
                let (word, tint) = if seat.ready { (t(T::Ready), MOSS) } else { (t(T::NotReady), if mine { WAX_DARK } else { TALLOW }) };
                row.label(RichText::new(word).color(c(tint)));
            });
        }
        None => {
            painter.rect_stroke(face, 0.0, Stroke::new(1.0, c(WOOD_LIGHT)), egui::StrokeKind::Inside);
            painter.text(face.center(), egui::Align2::CENTER_CENTER, "+", egui::FontId::proportional(14.0), c(WOOD_LIGHT));
            inner.label(RichText::new(t(T::SeatFree)).color(c(TALLOW)));
            inner.with_layout(Layout::right_to_left(Align::Center), |row| {
                if row.add(button(t(T::Invite))).clicked() {
                    invite = true;
                }
            });
        }
    }
    (kick, invite)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn screen(
    mut egui: bevy_egui::EguiContexts,
    (mut session, mut settings, mut table, store, game): (ResMut<Session>, ResMut<Settings>, ResMut<Table>, Res<Store>, Option<Res<Game>>),
    (mut link, mut lobby): (ResMut<SteamLink>, ResMut<Lobby>),
    (mut next, mut return_to): (ResMut<NextState<Screen>>, ResMut<ReturnTo>),
) {
    let lang = settings.lang;
    let t = |text: T| text.text(lang);
    let me = player_name(&settings);
    let hosting = session.is_host();
    let guest = session.is_client();
    let roster = table.roster.clone();
    let tables = found(&lobby, &link, lang);
    let steam = link.steam.is_some();
    let Ok(ctx) = egui.ctx_mut() else { return };
    let Some(mut ui) = kit::screen_ui(ctx, "lobby") else { return };
    let mut kick: Option<String> = None;
    let mut invite = false;
    let mut join: Option<Join> = None;
    let mut say = false;
    let mut leave = false;
    let mut back = false;
    let mut begin = false;
    let mut ready_pressed = false;
    let mut reopen = false;
    let title = match roster.seats.first() {
        Some(host) => format!("{} {}", t(T::TableOf), host.name),
        None => t(T::Multiplayer).to_string(),
    };
    let lobby = &mut *lobby;
    let about = if guest { t(T::LobbyGuestAbout) } else { t(T::LobbyAbout) };
    let sheet = Sheet { id: "lobby", title: &title, about, max: vec2(640.0, 470.0), top: 0.0, fixed: true };
    kit::sheet(
        &mut ui,
        sheet,
        |ui| {
            let area = ui.available_rect_before_wrap();
            let split = area.left() + area.width() * 0.5;
            let column = |rect: Rect| UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min));
            let mut left = ui.new_child(column(Rect::from_min_max(area.min, egui::pos2(split - 6.0, area.bottom()))));
            let mut right = ui.new_child(column(Rect::from_min_max(egui::pos2(split + 6.0, area.top()), area.max)));

            // The party: everyone at the table, then the free seats.
            left.horizontal(|row| {
                row.label(t(T::LobbyName));
                let field = row.add_enabled(!guest, egui::TextEdit::singleline(&mut lobby.name).desired_width(150.0).char_limit(40));
                if field.lost_focus() && lobby.name.trim() != settings.player_name && !lobby.name.trim().is_empty() {
                    settings.player_name = lobby.name.trim().to_string();
                    if let Err(e) = store.save_settings(&settings) {
                        error!("could not save settings: {e}");
                    }
                }
            });
            kit::title(&mut left, format!("{} {}/{}", t(T::Party), roster.seats.len().max(1), SEATS), crate::ui::theme::SMALL_TITLE);
            for n in 0..SEATS {
                let (k, i) = seat(&mut left, roster.seats.get(n), &me, hosting, lang);
                if k {
                    kick = roster.seats.get(n).map(|s| s.name.clone());
                }
                invite |= i;
            }
            // How others get in: the codes for each network this machine is on.
            if !roster.codes.is_empty() {
                kit::title(&mut left, t(T::TableCode), crate::ui::theme::SMALL_TITLE);
                for (network, code) in &roster.codes {
                    left.horizontal(|row| {
                        row.label(RichText::new(code).color(c(INK)).strong());
                        if row.add(button(t(T::Copy))).on_hover_text(t(T::CopyHint)).clicked() {
                            row.ctx().copy_text(code.clone());
                        }
                        hint(row, network.as_str());
                    });
                }
            }
            if roster.steam {
                hint(&mut left, t(T::SteamOpen));
            }
            // Alone: on the way back to a table, or with no table of one's own.
            if !hosting && !guest && table.rejoin.is_some() {
                hint(&mut left, t(T::Rejoining));
            } else if !hosting && !guest {
                kit::warning(&mut left, table.status.map_or(t(T::LobbyHostFailed), |s| s.text(lang)));
                if left.add(button(t(T::OpenTable))).clicked() {
                    reopen = true;
                }
            }

            // Other tables: on the network, through Steam, or by a code.
            kit::title(&mut right, t(T::FindTable), crate::ui::theme::SMALL_TITLE);
            egui::ScrollArea::vertical().id_salt("found").max_height(130.0).show(&mut right, |ui| {
                if tables.is_empty() {
                    hint(ui, t(T::NoTablesSeen));
                }
                for found in &tables {
                    ui.horizontal(|row| {
                        let label = format!("{} · {}", found.name, found.via);
                        row.label(RichText::new(label).color(c(if found.other_version { FADED } else { INK })));
                        row.with_layout(Layout::right_to_left(Align::Center), |row| {
                            let why = if found.other_version { t(T::OtherVersion) } else { t(T::LobbyAlready) };
                            if row.add(button(t(T::LobbyJoin)).enabled(!found.other_version && !guest, why)).clicked() {
                                join = Some(match &found.join {
                                    Join::Address(a) => Join::Address(a.clone()),
                                    Join::Steam(l) => Join::Steam(*l),
                                });
                            }
                            hint(row, format!("{} {}", found.about, found.seats).trim());
                        });
                    });
                }
            });
            right.horizontal(|row| {
                let field = row.add(egui::TextEdit::singleline(&mut lobby.typed).desired_width(170.0).char_limit(60).hint_text("K7Q4-MD2X-9HTA"));
                let ready = !lobby.typed.trim().is_empty() && !guest;
                let entered = field.lost_focus() && row.input(|i| i.key_pressed(Key::Enter)) && ready;
                if row.add(button(t(T::LobbyJoin)).enabled(ready, t(T::AddressFirst))).clicked() || entered {
                    join = Some(Join::Address(lobby.typed.trim().to_string()));
                }
            });
            hint(&mut right, t(T::CodeHint));
            if !steam {
                hint(&mut right, t(T::SteamUnavailable));
            }

            // The table's talk.
            kit::title(&mut right, t(T::TalkTitle), crate::ui::theme::SMALL_TITLE);
            egui::ScrollArea::vertical().id_salt("talk").max_height(90.0).stick_to_bottom(true).show(&mut right, |ui| {
                if table.chat.is_empty() {
                    hint(ui, t(T::TalkEmpty));
                }
                for line in &table.chat {
                    ui.add(egui::Label::new(RichText::new(format!("{}: {}", line.from, line.text)).color(c(INK))).wrap());
                }
            });
            right.horizontal(|row| {
                let field = row.add_enabled(hosting || guest, egui::TextEdit::singleline(&mut lobby.talk).desired_width(220.0).char_limit(300));
                if field.lost_focus() && row.input(|i| i.key_pressed(Key::Enter)) && !lobby.talk.trim().is_empty() {
                    say = true;
                }
            });
            if let Some(status) = table.status.filter(|_| hosting || guest) {
                kit::warning(&mut right, status.text(lang));
            }
            let tall = left.min_rect().height().max(right.min_rect().height());
            ui.allocate_space(vec2(area.width(), tall));
        },
        |footer| {
            if footer.add(button(t(T::Back)).key("Esc")).on_hover_text(t(T::LobbyBackHint)).clicked() {
                back = true;
            }
            if guest && footer.add(button(t(T::LeaveTable))).clicked() {
                leave = true;
            }
            if guest {
                let label = if lobby.ready { t(T::NotReadyAction) } else { t(T::ReadyAction) };
                if kit::right(footer, |r| r.add(button(label).primary())).clicked() {
                    ready_pressed = true;
                }
            }
            if hosting {
                let label = if game.is_some() { t(T::BackToGame) } else { t(T::Begin) };
                let all_ready = roster.seats.iter().all(|s| s.ready);
                let pressed = kit::right(footer, |r| r.add(button(label).primary().key("Enter")));
                let pressed = if all_ready { pressed } else { pressed.on_hover_text(t(T::NotAllReady)) };
                if pressed.clicked() {
                    begin = true;
                }
            }
        },
    );

    // The host with no game yet chooses where the table begins.
    if lobby.choosing {
        let mut close = false;
        egui::Window::new(t(T::Begin)).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            hint(ui, t(T::BeginHint));
            ui.horizontal(|row| {
                if row.add(button(t(T::NewGame)).primary()).clicked() {
                    return_to.push(Screen::Lobby);
                    next.set(Screen::NewGame);
                    close = true;
                }
                if row.add(button(t(T::LoadGame))).clicked() {
                    return_to.push(Screen::Lobby);
                    next.set(Screen::Load);
                    close = true;
                }
                if row.add(button(t(T::Cancel)).key("Esc")).clicked() {
                    close = true;
                }
            });
        });
        if close {
            lobby.choosing = false;
        }
    }
    // Friends asked in through Steam, one by one, or through Steam's own window.
    if lobby.inviting {
        let mut close = false;
        egui::Window::new(t(T::Invite)).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            if link.steam.is_some() && steam_table(&session) {
                hint(ui, t(T::SteamAsk));
                egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                    for (name, id) in link.online.clone().iter().take(30) {
                        let asked = link.asked.contains(id);
                        let label = if asked { format!("✓ {name}") } else { name.clone() };
                        if ui.add(button(label).wide()).clicked()
                            && let (Some(steam), Session::Host(hosting)) = (&link.steam, &*session)
                            && let Some(Ok(lobby_id)) = hosting.lock().unwrap().net.lobby()
                        {
                            steam.invite_friend(*id, lobby_id);
                            link.asked.push(*id);
                        }
                    }
                });
                if ui.add(button(t(T::SteamOverlay))).clicked() {
                    invite_friends(&link, &session);
                }
            } else {
                // Without Steam a friend comes in by the table's code.
                hint(ui, t(T::InviteByCode));
                for (network, code) in &roster.codes {
                    ui.horizontal(|row| {
                        row.label(RichText::new(code).strong());
                        if row.add(button(t(T::Copy))).clicked() {
                            row.ctx().copy_text(code.clone());
                        }
                        hint(row, network.as_str());
                    });
                }
            }
            if ui.add(button(t(T::Close)).key("Esc")).clicked() {
                close = true;
            }
        });
        if close {
            lobby.inviting = false;
        }
    }

    if invite {
        lobby.inviting = true;
    }
    if let Some(name) = kick {
        session.kick(&name);
    }
    if say {
        let text = std::mem::take(&mut lobby.talk);
        session.chat(&mut table, &me, text.trim());
    }
    if ready_pressed {
        lobby.ready = !lobby.ready;
        session.send(Up::Ready(lobby.ready));
    }
    if reopen {
        open_table(&mut session, &mut table, &settings, &link);
    }
    if let Some(way) = join {
        table.status = None;
        lobby.ready = false;
        let player = settings.player_id;
        match way {
            Join::Address(address) => {
                direct_name(&mut settings, link.steam.as_ref());
                match super::join(&address, &player_name(&settings), player) {
                    Ok(joined) => *session = joined,
                    Err(e) => {
                        error!("could not join {address}: {e}");
                        table.status = Some(T::LobbyJoinFailed);
                    }
                }
            }
            Join::Steam(lobby_id) => {
                if let Some(steam) = &link.steam {
                    settings.player_name = steam.name();
                    *session = join_steam(steam, lobby_id, &player_name(&settings), player);
                }
            }
        }
    }
    if begin {
        if game.is_some() {
            next.set(Screen::Play);
        } else {
            lobby.choosing = true;
        }
    }
    if leave {
        *session = Session::Solo;
        table.chat.clear();
        lobby.ready = false;
        // Out of somebody else's table, one's own opens again.
        open_table(&mut session, &mut table, &settings, &link);
    }
    if back && !lobby.choosing && !lobby.inviting {
        next.set(return_to.back());
    }
}

/// The line to the host dropped: a strip over whatever is on screen, while the table is
/// tried again, and a word when it could not be reached.
fn rejoin_strip(mut egui: bevy_egui::EguiContexts, mut table: ResMut<Table>, settings: Res<Settings>, mut next: ResMut<NextState<Screen>>) {
    if table.rejoin.is_none() && !table.lost {
        return;
    }
    let lang = settings.lang;
    let t = |text: T| text.text(lang);
    let Ok(ctx) = egui.ctx_mut() else { return };
    if !kit::ready(ctx) {
        return;
    }
    let mut alone = false;
    let mut menu = false;
    egui::Area::new(egui::Id::new("rejoin")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, vec2(0.0, 6.0)).show(ctx, |ui| {
        egui::Frame::new().fill(c(PARCHMENT)).stroke(Stroke::new(1.0, c(WAX))).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
            ui.horizontal(|row| {
                match &table.rejoin {
                    Some(rejoin) => {
                        row.add(egui::Spinner::new().size(9.0).color(c(WAX)));
                        row.label(RichText::new(format!("{} {}/{}", t(T::Rejoining), rejoin.tries.max(1), REJOIN_TRIES)).color(c(INK)));
                    }
                    None => {
                        row.label(RichText::new(t(T::LobbyLost)).color(c(WAX_DARK)));
                    }
                }
                if row.add(button(t(T::PlayAlone))).on_hover_text(t(T::PlayAloneHint)).clicked() {
                    alone = true;
                }
                if row.add(button(t(T::ToMenu))).clicked() {
                    menu = true;
                }
            });
        });
    });
    if alone || menu {
        table.rejoin = None;
        table.lost = false;
        table.status = None;
    }
    if menu {
        next.set(Screen::Main);
    }
}

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::Lobby), enter)
            .add_systems(OnExit(Screen::Lobby), |mut commands: Commands| commands.remove_resource::<Lobby>())
            .add_systems(Update, look.run_if(in_state(Screen::Lobby)))
            .add_systems(bevy_egui::EguiPrimaryContextPass, screen.in_set(crate::ui::UiSet::Screens).run_if(in_state(Screen::Lobby)).run_if(resource_exists::<Lobby>))
            .add_systems(bevy_egui::EguiPrimaryContextPass, rejoin_strip.in_set(crate::ui::UiSet::Screens));
    }
}
