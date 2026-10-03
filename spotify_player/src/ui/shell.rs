//! The app's layout, in three columns:
//! - a sidebar: a greeting, the menu (search, liked songs, ...) and the playlists
//! - the main column: now playing on the home screen, the open page otherwise
//!   (with a slim now-playing strip under it)
//! - Up Next: the queue
//!
//! Narrow terminals drop the queue first, then the sidebar.

use super::utils::to_bidi_string;
use super::{
    config, page, popup, Alignment, Block, BorderType, Borders, Constraint, Frame, Layout, Line,
    List, ListItem, Modifier, Paragraph, Rect, SharedState, Span, Style, UIStateGuard,
};
#[cfg(feature = "image")]
use crate::state::ImageRenderInfo;
use crate::state::{
    ContextId, ContextPageType, LibraryFocusState, PageState, PageType, PlaylistFolderItem,
    LIBRARY_MENU, USER_LIKED_TRACKS_URI, USER_RECENTLY_PLAYED_TRACKS_URI, USER_TOP_TRACKS_URI,
};
use chrono::Timelike;
use rspotify::model::{Id, PlayableItem};

const SIDEBAR_WIDTH: u16 = 30;
const QUEUE_WIDTH: u16 = 32;
/// Below this width there's no sidebar; below `QUEUE_MIN_WIDTH`, no queue.
const SIDEBAR_MIN_WIDTH: u16 = 80;
const QUEUE_MIN_WIDTH: u16 = 120;

/// Render the whole application.
pub fn render(frame: &mut Frame, state: &SharedState, ui: &mut UIStateGuard, rect: Rect) {
    let show_sidebar = rect.width >= SIDEBAR_MIN_WIDTH;
    let show_queue = rect.width >= QUEUE_MIN_WIDTH;
    let mut constraints = vec![];
    if show_sidebar {
        constraints.push(Constraint::Length(SIDEBAR_WIDTH));
    }
    constraints.push(Constraint::Fill(1));
    if show_queue {
        constraints.push(Constraint::Length(QUEUE_WIDTH));
    }
    let cols = Layout::horizontal(constraints).split(rect);
    let main = cols[usize::from(show_sidebar)];

    let is_home = ui.current_page().page_type() == PageType::Library;
    // too narrow for a sidebar: the home screen shows its lists in the middle
    let home_list = is_home && !show_sidebar;

    // The main column first: its cover image mustn't be drawn over.
    let main = if is_home && !home_list {
        main
    } else {
        render_now_playing_strip(frame, state, ui, main)
    };
    let main = popup::render_shortcut_help_popup(frame, ui, main);
    let (main, is_active) = popup::render_popup(frame, state, ui, main);
    if home_list {
        render_sidebar(frame, state, ui, main, is_active);
    } else if is_home {
        render_now_playing(frame, state, ui, main);
    } else {
        render_page(is_active, frame, state, ui, main);
    }

    if show_sidebar {
        render_sidebar(frame, state, ui, cols[0], is_home && is_active);
    }
    if show_queue {
        render_queue(frame, state, ui, cols[cols.len() - 1]);
    }
}

fn render_page(
    is_active: bool,
    frame: &mut Frame,
    state: &SharedState,
    ui: &mut UIStateGuard,
    rect: Rect,
) {
    match ui.current_page().page_type() {
        // the home screen's lists are in the sidebar
        PageType::Library => {}
        PageType::Search => page::render_search_page(is_active, frame, state, ui, rect),
        PageType::Context => page::render_context_page(is_active, frame, state, ui, rect),
        PageType::Browse => page::render_browse_page(is_active, frame, state, ui, rect),
        PageType::Lyrics => page::render_lyrics_page(is_active, frame, state, ui, rect),
        PageType::Queue => {
            if state.player.read().local_queue.is_some() {
                render_local_queue_page(frame, state, ui, rect);
            } else {
                page::render_queue_page(frame, state, ui, rect);
            }
        }
        PageType::CommandHelp => page::render_commands_help_page(frame, ui, rect),
        PageType::Logs => page::render_logs_page(frame, state, ui, rect),
    }
}

/// Which accent colour each part of the screen uses.
const ACCENT_ROSE: usize = 0;
const ACCENT_GOLD: usize = 1;
const ACCENT_FOAM: usize = 2;
const ACCENT_IRIS: usize = 3;
const ACCENT_LOVE: usize = 4;
const ACCENT_PINE: usize = 5;

/// Menu entries, each its own colour (the heart in love-red).
const MENU_COLORS: [usize; 8] = [
    ACCENT_FOAM,
    ACCENT_LOVE,
    ACCENT_GOLD,
    ACCENT_IRIS,
    ACCENT_ROSE,
    ACCENT_PINE,
    ACCENT_FOAM,
    ACCENT_GOLD,
];
/// Shuffle / repeat / volume / device on the status line.
const DETAIL_COLORS: [usize; 4] = [ACCENT_FOAM, ACCENT_IRIS, ACCENT_GOLD, ACCENT_PINE];

fn accent(ui: &UIStateGuard, i: usize) -> Style {
    Style::default().fg(ui.theme.accent(i))
}

/// A rounded box in an accent colour, with a title; returns the inside.
fn panel(frame: &mut Frame, ui: &UIStateGuard, title: &str, color: usize, rect: Rect) -> Rect {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(accent(ui, color).add_modifier(Modifier::DIM));
    if !title.is_empty() {
        block = block.title(Span::styled(
            format!(" {title} "),
            accent(ui, color).add_modifier(Modifier::BOLD),
        ));
    }
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

// ---------- sidebar ----------

/// "Good evening" etc., by the local time.
fn greeting(hour: u32) -> &'static str {
    match hour {
        5..=11 => "Good morning",
        12..=16 => "Good afternoon",
        17..=21 => "Good evening",
        _ => "Hey",
    }
}

/// Which menu entry the open page belongs to, to mark it in the sidebar.
fn menu_entry_for(page: &PageState) -> Option<usize> {
    let tracks_kind = |id: &str| match id {
        USER_LIKED_TRACKS_URI => Some(1),
        USER_RECENTLY_PLAYED_TRACKS_URI => Some(2),
        USER_TOP_TRACKS_URI => Some(3),
        _ => None,
    };
    match page {
        PageState::Search { .. } => Some(0),
        PageState::Context {
            context_page_type: ContextPageType::Browsing(ContextId::Tracks(id)),
            ..
        } => tracks_kind(&id.uri),
        PageState::Browse { .. } => Some(6),
        PageState::Lyrics { .. } => Some(7),
        _ => None,
    }
}

/// The shortest key sequence bound to `command`, shown next to menu entries.
fn shortcut_for(command: crate::command::Command) -> Option<String> {
    config::get_config()
        .keymap_config
        .keymaps
        .iter()
        .filter(|k| k.command == command)
        .map(|k| k.key_sequence.to_string())
        .min_by_key(|s| s.chars().count())
}

/// The playlist open in the main column, to mark it in the sidebar.
fn open_playlist_uri(page: &PageState) -> Option<String> {
    match page {
        PageState::Context {
            context_page_type: ContextPageType::Browsing(ContextId::Playlist(id)),
            ..
        } => Some(id.uri()),
        _ => None,
    }
}

fn render_sidebar(
    frame: &mut Frame,
    state: &SharedState,
    ui: &mut UIStateGuard,
    rect: Rect,
    is_active: bool,
) {
    let inner = panel(frame, ui, "", ACCENT_IRIS, rect);
    let data = state.data.read();
    let playing_uri = state.player.read().playing_context_id().map(|c| c.uri());

    // greeting
    let name = data.user_data.user.as_ref().map(|u| {
        u.display_name
            .clone()
            .unwrap_or_else(|| u.id.id().to_string())
    });
    let chunks = Layout::vertical([
        Constraint::Length(3),                         // greeting
        Constraint::Length(LIBRARY_MENU.len() as u16), // menu
        Constraint::Length(2),                         // "Playlists" heading
        Constraint::Fill(1),                           // playlists
    ])
    .split(inner);
    let hello = greeting(chrono::Local::now().hour());
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(" ✦ ", accent(ui, ACCENT_GOLD)),
                Span::styled(format!("{hello},"), accent(ui, ACCENT_GOLD)),
            ]),
            Line::from(Span::styled(
                format!("   {}", name.as_deref().unwrap_or("friend")),
                accent(ui, ACCENT_ROSE).add_modifier(Modifier::BOLD),
            )),
        ]),
        chunks[0],
    );

    let current = ui.current_page();
    let open_menu = menu_entry_for(current);
    let open_playlist = open_playlist_uri(current);

    // the home screen's list states live in its page state; find it
    let Some(lib) = ui
        .history
        .iter()
        .rposition(|p| matches!(p, PageState::Library { .. }))
    else {
        return;
    };
    let (focus, folder_id) = match &ui.history[lib] {
        PageState::Library { state } => (state.focus, state.playlist_folder_id),
        _ => return,
    };

    // playlists, filtered by `/` while on the home screen
    let all = data.user_data.folder_playlists_items(folder_id);
    let playlists: Vec<&PlaylistFolderItem> = if is_active {
        ui.search_filtered_items(&all)
            .into_iter()
            .copied()
            .collect()
    } else {
        all.clone()
    };
    let name_width = chunks[3].width.saturating_sub(2) as usize;
    let playlist_items: Vec<ListItem> = playlists
        .iter()
        .enumerate()
        .map(|(i, item)| match item {
            PlaylistFolderItem::Playlist(p) => {
                let uri = p.id.uri();
                let style = if playing_uri.as_deref() == Some(uri.as_str()) {
                    ui.theme.current_playing()
                } else if open_playlist.as_deref() == Some(uri.as_str()) {
                    ui.theme.playback_status()
                } else {
                    Style::default()
                };
                let dot = if playing_uri.as_deref() == Some(uri.as_str()) {
                    "♪"
                } else {
                    "•"
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {dot} "), accent(ui, i)),
                    Span::styled(
                        truncate(&to_bidi_string(&p.name), name_width.saturating_sub(2)),
                        style,
                    ),
                ]))
            }
            PlaylistFolderItem::Folder(f) => ListItem::new(Line::from(vec![
                Span::styled(" ▸ ", accent(ui, ACCENT_GOLD)),
                Span::raw(to_bidi_string(&f.name)),
            ])),
        })
        .collect();
    let n_playlists = playlist_items.len();

    let menu_width = chunks[1].width as usize;
    let menu_items: Vec<ListItem> = LIBRARY_MENU
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let open = open_menu == Some(i);
            let hint = shortcut_for(item.command).unwrap_or_default();
            let pad =
                menu_width.saturating_sub(item.label.chars().count() + hint.chars().count() + 4);
            let icon_style = accent(ui, MENU_COLORS[i % MENU_COLORS.len()]);
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {} ", item.icon), icon_style),
                Span::styled(
                    item.label,
                    if open {
                        icon_style.add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
                Span::raw(" ".repeat(pad)),
                Span::styled(hint, ui.theme.playlist_desc()),
            ]))
        })
        .collect();

    let heading = if folder_id == 0 {
        " Playlists".to_string()
    } else {
        " ← Playlists".to_string()
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::default(),
            Line::from(vec![
                Span::styled(
                    heading,
                    accent(ui, ACCENT_IRIS).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" · {n_playlists}"), ui.theme.playlist_desc()),
            ]),
        ]),
        chunks[2],
    );

    let theme = ui.theme.clone();
    let PageState::Library { state: lib_state } = &mut ui.history[lib] else {
        return;
    };
    let highlight = |active: bool| {
        if active {
            theme.selection(true)
        } else {
            Style::default()
        }
    };
    let menu_active = is_active && focus == LibraryFocusState::Menu;
    let list_active = is_active && focus == LibraryFocusState::Playlists;
    if let Some(sel) = lib_state.menu_list.selected() {
        lib_state
            .menu_list
            .select(Some(sel.min(LIBRARY_MENU.len() - 1)));
    }
    frame.render_stateful_widget(
        List::new(menu_items).highlight_style(highlight(menu_active)),
        chunks[1],
        &mut lib_state.menu_list,
    );
    if n_playlists == 0 {
        let msg = if data.user_data.playlists.is_empty() {
            " Loading…"
        } else {
            " No matches"
        };
        frame.render_widget(
            Paragraph::new(Span::styled(msg, theme.playlist_desc())),
            chunks[3],
        );
        return;
    }
    match lib_state.playlist_list.selected() {
        Some(p) if p >= n_playlists => lib_state.playlist_list.select(Some(n_playlists - 1)),
        None => lib_state.playlist_list.select(Some(0)),
        _ => {}
    }
    frame.render_stateful_widget(
        List::new(playlist_items).highlight_style(highlight(list_active)),
        chunks[3],
        &mut lib_state.playlist_list,
    );
}

// ---------- now playing ----------

struct NowPlaying {
    title: String,
    artists: String,
    album: String,
    liked: bool,
    playing: bool,
    progress: chrono::Duration,
    duration: chrono::Duration,
    details: Vec<String>, // shuffle, repeat, volume, device
    #[cfg_attr(not(feature = "image"), allow(dead_code))]
    cover_url: Option<String>,
}

fn now_playing(state: &SharedState) -> Option<NowPlaying> {
    let player = state.player.read();
    let data = state.data.read();
    let mut np = if let Some(local) = &player.local {
        // playing here: librespot's own state, current even when the Web API
        // is slow or rate limited
        NowPlaying {
            title: local.name.clone(),
            artists: local.artists.clone(),
            album: local.album.clone(),
            liked: data.user_data.saved_tracks.contains_key(&local.uri),
            playing: local.is_playing,
            progress: local.progress(),
            duration: local.duration,
            details: vec![],
            cover_url: local.cover_url.clone(),
        }
    } else {
        let item = player.currently_playing()?;
        let (title, artists, album, duration, liked, cover_url) = match item {
            PlayableItem::Track(t) => (
                t.name.clone(),
                crate::utils::map_join(&t.artists, |a| &a.name, ", "),
                t.album.name.clone(),
                t.duration,
                t.id.as_ref()
                    .is_some_and(|id| data.user_data.saved_tracks.contains_key(&id.uri())),
                crate::utils::get_track_album_image_url(t).map(String::from),
            ),
            #[allow(deprecated)]
            PlayableItem::Episode(e) => (
                e.name.clone(),
                e.show.publisher.clone(),
                e.show.name.clone(),
                e.duration,
                false,
                crate::utils::get_episode_show_image_url(e).map(String::from),
            ),
            PlayableItem::Unknown(_) => return None,
        };
        let progress = player
            .playback_progress()
            .unwrap_or_default()
            .clamp(chrono::Duration::zero(), duration);
        let playing = player
            .buffered_playback
            .as_ref()
            .is_some_and(|p| p.is_playing);
        NowPlaying {
            title,
            artists,
            album,
            liked,
            playing,
            progress,
            duration,
            details: vec![],
            cover_url,
        }
    };
    if let Some(p) = &player.buffered_playback {
        if p.shuffle_state {
            np.details.push("⇄ shuffle".to_string());
        }
        match p.repeat_state {
            rspotify::model::RepeatState::Track => np.details.push("↻¹ repeat song".to_string()),
            rspotify::model::RepeatState::Context => {
                np.details.push("↻ repeat playlist".to_string());
            }
            rspotify::model::RepeatState::Off => {}
        }
        match p.mute_state {
            Some(v) => np.details.push(format!("{v}% muted")),
            None => np
                .details
                .push(format!("{}%", p.volume.unwrap_or_default())),
        }
        np.details.push(p.device_name.clone());
    }
    Some(np)
}

/// The home screen's centre: cover, title, the wave and the progress bar.
fn render_now_playing(frame: &mut Frame, state: &SharedState, ui: &mut UIStateGuard, rect: Rect) {
    let inner = panel(frame, ui, "Now Playing", ACCENT_ROSE, rect);
    let Some(np) = now_playing(state) else {
        #[cfg(feature = "image")]
        {
            ui.last_cover_image_render_info = ImageRenderInfo::default();
        }
        render_idle(frame, state, ui, inner);
        return;
    };

    let wave_rows: u16 = if wave_active(state) {
        (inner.height / 5).clamp(3, 8)
    } else {
        1
    };
    // title, artists, gap, wave, gap, progress, details, and a margin
    let text_rows = 2 + 1 + wave_rows + 1 + 1 + 1 + 2;
    let cover_rows = inner.height.saturating_sub(text_rows).min(22);
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(cover_rows),
        Constraint::Length(1),
        Constraint::Length(1), // title
        Constraint::Length(1), // artists · album
        Constraint::Length(1),
        Constraint::Length(wave_rows),
        Constraint::Length(1),
        Constraint::Length(1), // progress
        Constraint::Length(1), // details
    ])
    .flex(ratatui::layout::Flex::Center)
    .split(inner);

    #[cfg(feature = "image")]
    if cover_rows >= 4 {
        let font = ui.picker.font_size();
        let cols = (u32::from(cover_rows) * u32::from(font.height) / u32::from(font.width.max(1)))
            .min(u32::from(inner.width.saturating_sub(4))) as u16;
        let [cover] = Layout::horizontal([Constraint::Length(cols)])
            .flex(ratatui::layout::Flex::Center)
            .areas(chunks[1]);
        render_cover(frame, state, ui, np.cover_url.as_deref(), cover);
    }

    let mut title = vec![Span::styled(np.title.clone(), ui.theme.playback_track())];
    if np.liked {
        title.push(Span::styled(
            format!(" {}", config::get_config().app_config.liked_icon),
            ui.theme.like(),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(title)).alignment(Alignment::Center),
        chunks[3],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(np.artists.clone(), ui.theme.playback_artists()),
            Span::styled("  ·  ", ui.theme.playlist_desc()),
            Span::styled(np.album.clone(), ui.theme.playback_album()),
        ]))
        .alignment(Alignment::Center),
        chunks[4],
    );

    let side = inner.width / 10;
    let wave = Rect {
        x: inner.x + side,
        width: inner.width.saturating_sub(side * 2),
        ..chunks[6]
    };
    render_wave(frame, state, ui, wave);
    let bar = Rect {
        x: inner.x + side,
        width: inner.width.saturating_sub(side * 2),
        ..chunks[8]
    };
    render_progress(frame, ui, &np, bar);

    let status = if np.playing {
        "▶ playing"
    } else {
        "▌▌ paused"
    };
    let mut details = vec![Span::styled(
        status,
        accent(ui, ACCENT_ROSE).add_modifier(Modifier::BOLD),
    )];
    for (i, d) in np.details.iter().enumerate() {
        details.push(Span::styled("  ·  ", ui.theme.playlist_desc()));
        details.push(Span::styled(d.clone(), accent(ui, DETAIL_COLORS[i % 4])));
    }
    frame.render_widget(
        Paragraph::new(Line::from(details)).alignment(Alignment::Center),
        chunks[9],
    );
}

/// Nothing playing: say so, and how to start.
fn render_idle(frame: &mut Frame, state: &SharedState, ui: &UIStateGuard, rect: Rect) {
    let waiting = state.player.read().playback_last_updated_time.is_none();
    let lines = if waiting {
        const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let i = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            / 100) as usize
            % SPINNER.len();
        vec![Line::from(Span::styled(
            format!("{} connecting to Spotify…", SPINNER[i]),
            ui.theme.playback_metadata(),
        ))]
    } else {
        vec![
            Line::from(Span::styled("Nothing playing", ui.theme.playback_track())),
            Line::default(),
            Line::from(Span::styled(
                "pick a playlist or Liked Songs on the left, or press S to search",
                ui.theme.playlist_desc(),
            )),
            Line::from(Span::styled(
                "enter plays · space pauses · ctrl+r repeat · ctrl+s shuffle · f or ? every key",
                ui.theme.playlist_desc(),
            )),
        ]
    };
    let [mid] = Layout::vertical([Constraint::Length(lines.len() as u16)])
        .flex(ratatui::layout::Flex::Center)
        .areas(rect);
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), mid);
}

/// The slim now-playing strip under a page; returns the space above it.
fn render_now_playing_strip(
    frame: &mut Frame,
    state: &SharedState,
    ui: &mut UIStateGuard,
    rect: Rect,
) -> Rect {
    let Some(np) = now_playing(state) else {
        #[cfg(feature = "image")]
        {
            ui.last_cover_image_render_info = ImageRenderInfo::default();
        }
        return rect;
    };
    let wave_rows: u16 = if wave_active(state) { 2 } else { 0 };
    let height = 2 + wave_rows + 2; // borders, title, progress
    let [page, strip] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(height)]).areas(rect);
    let inner = panel(frame, ui, "", ACCENT_ROSE, strip);

    #[allow(unused_mut)]
    let mut text = inner;
    #[cfg(feature = "image")]
    {
        let rows = inner.height;
        let font = ui.picker.font_size();
        let cols = (u32::from(rows) * u32::from(font.height) / u32::from(font.width.max(1))) as u16;
        if rows >= 2 && inner.width > cols + 20 {
            let [cover, rest] = Layout::horizontal([Constraint::Length(cols), Constraint::Fill(1)])
                .spacing(1)
                .areas(inner);
            render_cover(frame, state, ui, np.cover_url.as_deref(), cover);
            text = rest;
        }
    }
    let rows = Layout::vertical([
        Constraint::Length(wave_rows),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(text);
    render_wave(frame, state, ui, rows[0]);
    let status = if np.playing { "▶ " } else { "▌▌ " };
    let mut line = vec![
        Span::styled(status, accent(ui, ACCENT_ROSE).add_modifier(Modifier::BOLD)),
        Span::styled(np.title.clone(), ui.theme.playback_track()),
    ];
    if np.liked {
        line.push(Span::styled(
            format!(" {}", config::get_config().app_config.liked_icon),
            ui.theme.like(),
        ));
    }
    line.push(Span::styled("  ·  ", ui.theme.playlist_desc()));
    line.push(Span::styled(
        np.artists.clone(),
        ui.theme.playback_artists(),
    ));
    frame.render_widget(Paragraph::new(Line::from(line)), rows[1]);
    render_progress(frame, ui, &np, rows[2]);
    page
}

/// `2:31 ━━━━━━━━●──────── 5:07`; the bar part is clickable to seek.
fn render_progress(frame: &mut Frame, ui: &mut UIStateGuard, np: &NowPlaying, rect: Rect) {
    let elapsed = crate::utils::format_duration(&np.progress);
    let total = crate::utils::format_duration(&np.duration);
    let label_w = (elapsed.chars().count() + 2) as u16;
    let total_w = (total.chars().count() + 2) as u16;
    if rect.width <= label_w + total_w + 4 {
        return;
    }
    let bar_w = rect.width - label_w - total_w;
    let ratio = if np.duration.num_milliseconds() > 0 {
        (np.progress.num_milliseconds() as f64 / np.duration.num_milliseconds() as f64)
            .clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled = ((f64::from(bar_w) * ratio) as u16).min(bar_w.saturating_sub(1));
    // the played part fades foam → iris → rose across the whole bar
    let stops = [
        ui.theme.accent(ACCENT_FOAM),
        ui.theme.accent(ACCENT_IRIS),
        ui.theme.accent(ACCENT_ROSE),
    ];
    let at = |x: u16| gradient(&stops, f32::from(x) / f32::from(bar_w.max(1)));
    let mut spans = vec![Span::styled(
        format!("{elapsed}  "),
        accent(ui, ACCENT_FOAM),
    )];
    spans.extend((0..filled).map(|x| Span::styled("━", Style::default().fg(at(x)))));
    spans.push(Span::styled(
        "●",
        Style::default().fg(at(filled)).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        "─".repeat(bar_w.saturating_sub(filled + 1) as usize),
        ui.theme.playback_progress_bar_unfilled(),
    ));
    spans.push(Span::styled(format!("  {total}"), accent(ui, ACCENT_ROSE)));
    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line), rect);
    ui.playback_progress_bar_rect = Rect {
        x: rect.x + label_w,
        width: bar_w,
        height: 1,
        ..rect
    };
}

/// A colour `t` (0..=1) of the way along evenly spaced `stops`. Needs RGB
/// colours (as in Rosé Pine); for named terminal colours, the nearest stop.
fn gradient(stops: &[ratatui::style::Color], pos: f32) -> ratatui::style::Color {
    use ratatui::style::Color;
    let scaled = pos.clamp(0.0, 1.0) * (stops.len() - 1) as f32;
    let seg = (scaled as usize).min(stops.len() - 2);
    let frac = scaled - seg as f32;
    match (stops[seg], stops[seg + 1]) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let mix = |from: u8, to: u8| {
                (f32::from(from) + (f32::from(to) - f32::from(from)) * frac).round() as u8
            };
            Color::Rgb(mix(r1, r2), mix(g1, g2), mix(b1, b2))
        }
        (from, to) => {
            if frac < 0.5 {
                from
            } else {
                to
            }
        }
    }
}

/// Whether the wave has audio to show (music playing through this app).
fn wave_active(state: &SharedState) -> bool {
    #[cfg(feature = "streaming")]
    {
        config::get_config().app_config.enable_audio_visualization
            && state.is_local_streaming_active()
    }
    #[cfg(not(feature = "streaming"))]
    {
        let _ = state;
        false
    }
}

/// The audio wave, or a quiet line when there's no local audio to show.
fn render_wave(frame: &mut Frame, state: &SharedState, ui: &UIStateGuard, rect: Rect) {
    if rect.height == 0 || rect.width == 0 {
        return;
    }
    #[cfg(feature = "streaming")]
    if wave_active(state) {
        super::streaming::render_audio_visualization(frame, state, &ui.theme, rect);
        return;
    }
    let _ = state;
    let last = Rect {
        y: rect.y + rect.height - 1,
        height: 1,
        ..rect
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            "▁".repeat(rect.width as usize),
            ui.theme.playback_progress_bar_unfilled(),
        )),
        last,
    );
}

/// Draw the cover image, re-encoding it only when the track or the area changes.
#[cfg(feature = "image")]
fn render_cover(
    frame: &mut Frame,
    state: &SharedState,
    ui: &mut UIStateGuard,
    url: Option<&str>,
    area: Rect,
) {
    let Some(url) = url else {
        return;
    };
    let data = state.data.read();
    let Some(img) = data.caches.images.get(url) else {
        return; // still downloading
    };
    if ui.last_cover_image_render_info.url != url
        || ui.last_cover_image_render_info.render_area != area
    {
        let cover = match crate::ui::cover_image::CoverImage::new(&ui.picker, img, area) {
            Ok(cover) => Some(cover),
            Err(err) => {
                tracing::error!("Failed to encode cover image: {err:#}");
                None
            }
        };
        ui.last_cover_image_render_info = ImageRenderInfo {
            url: url.to_string(),
            render_area: area,
            state: cover,
        };
    }
    if let Some(cover) = ui.last_cover_image_render_info.state.as_mut() {
        cover.render(frame, area);
    }
}

// ---------- up next ----------

/// An upcoming track, as shown.
struct QueueRow {
    name: String,
    by: String,
    duration: Option<chrono::Duration>,
    /// added with "add to queue", not just next in the playlist
    queued: bool,
}

/// The integrated player's upcoming tracks, when playing here.
fn local_queue_rows(state: &SharedState) -> Option<Vec<QueueRow>> {
    let player = state.player.read();
    let queue = player.local_queue.as_ref()?;
    Some(
        queue
            .iter()
            .map(|item| match player.queue_names.get(&item.uri) {
                Some(n) => QueueRow {
                    name: n.name.clone(),
                    by: n.artists.clone(),
                    duration: Some(n.duration),
                    queued: item.queued,
                },
                None => QueueRow {
                    name: "…".to_string(),
                    by: String::new(),
                    duration: None,
                    queued: item.queued,
                },
            })
            .collect(),
    )
}

/// The queue page (`z`) while playing here: selectable and editable.
fn render_local_queue_page(
    frame: &mut Frame,
    state: &SharedState,
    ui: &mut UIStateGuard,
    rect: Rect,
) {
    let inner = panel(frame, ui, "Queue", ACCENT_FOAM, rect);
    let rows = local_queue_rows(state).unwrap_or_default();
    let [hint, list] = Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(inner);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" enter", accent(ui, ACCENT_GOLD)),
            Span::styled(" play from here  ", ui.theme.playlist_desc()),
            Span::styled("X", accent(ui, ACCENT_LOVE)),
            Span::styled(" remove  ", ui.theme.playlist_desc()),
            Span::styled("ctrl+k/j", accent(ui, ACCENT_IRIS)),
            Span::styled(" move up/down  ", ui.theme.playlist_desc()),
            Span::styled("Z", accent(ui, ACCENT_FOAM)),
            Span::styled(" (on any song) add", ui.theme.playlist_desc()),
        ])),
        hint,
    );
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(" Nothing coming up", ui.theme.playlist_desc())),
            list,
        );
        return;
    }
    let PageState::Queue { scroll_offset } = ui.current_page_mut() else {
        return;
    };
    *scroll_offset = (*scroll_offset).min(rows.len() - 1);
    let selected = *scroll_offset;
    let width = list.width as usize;
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let num = format!(" {:>3}  ", i + 1);
            let tag = if row.queued { "+ " } else { "  " };
            let time = row
                .duration
                .map(|d| crate::utils::format_duration(&d))
                .unwrap_or_default();
            let used = num.chars().count() + tag.chars().count() + time.chars().count() + 4;
            let name_w = (width.saturating_sub(used) * 3 / 5).max(8);
            let by_w = width.saturating_sub(used + name_w);
            let color = accent(
                ui,
                [ACCENT_ROSE, ACCENT_GOLD, ACCENT_FOAM, ACCENT_IRIS][i % 4],
            );
            ListItem::new(Line::from(vec![
                Span::styled(num, color.add_modifier(Modifier::BOLD)),
                Span::styled(tag, accent(ui, ACCENT_GOLD).add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("{:<name_w$}", truncate(&row.name, name_w)),
                    ui.theme.playback_track().remove_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(format!("{:<by_w$}", truncate(&row.by, by_w)), color),
                Span::styled(format!("  {time}"), ui.theme.playlist_desc()),
            ]))
        })
        .collect();
    let mut list_state = ratatui::widgets::ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(
        List::new(items).highlight_style(ui.theme.selection(true)),
        list,
        &mut list_state,
    );
}

fn render_queue(frame: &mut Frame, state: &SharedState, ui: &mut UIStateGuard, rect: Rect) {
    let inner = panel(frame, ui, "Up Next", ACCENT_FOAM, rect);
    if let Some(rows) = local_queue_rows(state) {
        render_queue_rows(frame, ui, &rows, inner);
        return;
    }
    let player = state.player.read();
    let Some(queue) = player.queue.as_ref() else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                " Nothing queued yet",
                ui.theme.playlist_desc(),
            )),
            inner,
        );
        return;
    };
    let rows: Vec<QueueRow> = queue
        .queue
        .iter()
        .filter_map(|item| match item {
            PlayableItem::Track(t) => Some(QueueRow {
                name: t.name.clone(),
                by: crate::utils::map_join(&t.artists, |a| &a.name, ", "),
                duration: Some(t.duration),
                queued: false,
            }),
            #[allow(deprecated)]
            PlayableItem::Episode(e) => Some(QueueRow {
                name: e.name.clone(),
                by: e.show.publisher.clone(),
                duration: Some(e.duration),
                queued: false,
            }),
            PlayableItem::Unknown(_) => None,
        })
        .collect();
    drop(player);
    render_queue_rows(frame, ui, &rows, inner);
}

/// The Up Next panel's list.
fn render_queue_rows(frame: &mut Frame, ui: &UIStateGuard, rows: &[QueueRow], inner: Rect) {
    let width = inner.width.saturating_sub(2) as usize;
    let mut items = vec![];
    for (n, row) in rows.iter().take(inner.height as usize / 3 + 1).enumerate() {
        let time = row
            .duration
            .map(|d| crate::utils::format_duration(&d))
            .unwrap_or_default();
        let num = format!(" {:>2} ", n + 1);
        let indent = num.chars().count();
        let by_w = width.saturating_sub(time.chars().count() + 3 + indent);
        let color = accent(
            ui,
            [ACCENT_ROSE, ACCENT_GOLD, ACCENT_FOAM, ACCENT_IRIS][n % 4],
        );
        let mut first = vec![Span::styled(num, color.add_modifier(Modifier::BOLD))];
        let mut name_w = width.saturating_sub(indent);
        if row.queued {
            first.push(Span::styled(
                "+ ",
                accent(ui, ACCENT_GOLD).add_modifier(Modifier::BOLD),
            ));
            name_w = name_w.saturating_sub(2);
        }
        first.push(Span::styled(
            truncate(&row.name, name_w),
            ui.theme.playback_track().remove_modifier(Modifier::BOLD),
        ));
        items.push(ListItem::new(vec![
            Line::from(first),
            Line::from(vec![
                Span::raw(" ".repeat(indent)),
                Span::styled(truncate(&row.by, by_w), color),
                Span::styled(
                    if time.is_empty() {
                        String::new()
                    } else {
                        format!(" · {time}")
                    },
                    ui.theme.playlist_desc(),
                ),
            ]),
            Line::default(),
        ]));
    }
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(" Nothing queued", ui.theme.playlist_desc())),
            inner,
        );
        return;
    }
    frame.render_widget(List::new(items), inner);
}

/// Cut `s` to `w` characters, with an ellipsis.
fn truncate(s: &str, w: usize) -> String {
    if s.chars().count() <= w {
        return s.to_string();
    }
    let mut out: String = s.chars().take(w.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greets_by_time_of_day() {
        assert_eq!(greeting(8), "Good morning");
        assert_eq!(greeting(13), "Good afternoon");
        assert_eq!(greeting(19), "Good evening");
        assert_eq!(greeting(2), "Hey");
    }

    #[test]
    fn gradient_blends_between_stops() {
        use ratatui::style::Color;
        let stops = [
            Color::Rgb(0, 0, 0),
            Color::Rgb(200, 100, 0),
            Color::Rgb(200, 100, 200),
        ];
        assert_eq!(gradient(&stops, 0.0), Color::Rgb(0, 0, 0));
        assert_eq!(gradient(&stops, 0.25), Color::Rgb(100, 50, 0));
        assert_eq!(gradient(&stops, 0.5), Color::Rgb(200, 100, 0));
        assert_eq!(gradient(&stops, 1.0), Color::Rgb(200, 100, 200));
        assert_eq!(gradient(&stops, 7.0), Color::Rgb(200, 100, 200)); // clamped
                                                                      // named terminal colours: nearest stop
        assert_eq!(gradient(&[Color::Red, Color::Blue], 0.9), Color::Blue);
    }

    #[test]
    fn truncates_with_ellipsis() {
        assert_eq!(truncate("Motion Sickness", 20), "Motion Sickness");
        assert_eq!(truncate("Motion Sickness", 7), "Motion…");
    }
}
