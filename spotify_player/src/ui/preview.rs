//! Renders the screenshots in `assets/screenshots` with made-up music, no
//! Spotify account or terminal needed: `scripts/screenshots` runs this and
//! turns its output into pictures.
//!
//! Each screen is written to `$PREVIEW_OUT/<name>.json`: the cells, and where
//! the cover image goes (it is drawn by the terminal, not in cells).

use std::{collections::VecDeque, sync::Arc};

use ratatui::{backend::TestBackend, layout::Rect, style::Color, Terminal};
use rspotify::model::{AlbumId, ArtistId, Id, PlaylistId, TrackId, UserId};

use crate::{
    config,
    state::{
        Album, Artist, Context, ContextId, ContextPageType, ContextPageUIState, LocalNowPlaying,
        LocalQueueItem, Lyrics, Mutex, PageState, PlaybackMetadata, Playlist, PlaylistFolderItem,
        QueueName, SearchCacheEntry, SearchPageUIState, SearchResults, SharedState, State, Track,
        TTL_CACHE_DURATION,
    },
    ui::single_line_input::LineInput,
};

const WIDTH: u16 = 160;
const HEIGHT: u16 = 44;

/// A made-up 22-character Spotify id.
fn id(n: usize) -> String {
    format!("{:0>22}", format!("{n}x"))
}

fn artist(name: &str, n: usize) -> Artist {
    Artist {
        id: ArtistId::from_id(id(n)).unwrap(),
        name: name.to_string(),
    }
}

fn album(name: &str, by: &str, n: usize) -> Album {
    Album {
        id: AlbumId::from_id(id(n)).unwrap(),
        release_date: "2024".to_string(),
        name: name.to_string(),
        artists: vec![artist(by, n + 500)],
        typ: None,
        added_at: 0,
    }
}

fn playlist(name: &str, desc: &str, n: usize) -> Playlist {
    Playlist {
        id: PlaylistId::from_id(id(n)).unwrap(),
        collaborative: false,
        name: name.to_string(),
        owner: ("you".to_string(), UserId::from_id("you").unwrap()),
        desc: desc.to_string(),
        current_folder_id: 0,
        snapshot_id: String::new(),
    }
}

/// Songs: title, artist, album, length in seconds.
const SONGS: &[(&str, &str, &str, u64)] = &[
    ("Glass Harbour", "Low Tide Club", "Salt & Neon", 247),
    ("Paper Moons", "Ivy Calder", "Night Swimming", 213),
    ("Static Bloom", "The Velvet Hours", "Static Bloom", 198),
    ("Slow Burn City", "Marlow Reyes", "Afterglow FM", 231),
    ("Cassette Summer", "Low Tide Club", "Salt & Neon", 186),
    ("Lanterns", "Nadia Moss", "Lanterns", 264),
    ("Rooftop Weather", "Ivy Calder", "Night Swimming", 202),
    ("Northbound", "Echo Avenue", "Mile Markers", 239),
    ("Soft Machines", "The Velvet Hours", "Static Bloom", 221),
    ("Honey & Smoke", "Marlow Reyes", "Afterglow FM", 194),
    ("Ferris Wheel", "Nadia Moss", "Lanterns", 256),
    ("Undertow", "Echo Avenue", "Mile Markers", 228),
    ("Blue Hour", "Low Tide Club", "Salt & Neon", 233),
    ("Silver Lining Motel", "Ivy Calder", "Night Swimming", 247),
    ("Wildflower Static", "The Velvet Hours", "Static Bloom", 209),
    ("Last Train Home", "Marlow Reyes", "Afterglow FM", 262),
];

fn track(i: usize) -> Track {
    let (name, by, on, secs) = SONGS[i % SONGS.len()];
    Track {
        id: TrackId::from_id(id(1000 + i)).unwrap(),
        name: name.to_string(),
        artists: vec![artist(by, 2000 + i)],
        album: Some(album(on, by, 3000 + i)),
        duration: std::time::Duration::from_secs(secs),
        explicit: false,
        added_at: 0,
    }
}

const PLAYLISTS: &[(&str, &str)] = &[
    ("Late Night Drive", "Neon, rain and empty roads"),
    ("Deep Focus", "Instrumentals for long sessions"),
    ("Sunday Slow", "Coffee and soft guitars"),
    ("Running Mix", "160 bpm and up"),
    ("Indie Gold", "The ones that stuck"),
    ("Lo-fi Study", "Beats to think to"),
    ("Road Trip 2024", ""),
    ("Rainy Day Jazz", ""),
    ("Gym Bangers", ""),
    ("Old Favourites", ""),
];

/// A state as if logged in and playing the first song of "Late Night Drive".
fn sample_state() -> SharedState {
    let config_dir = tempdir("config");
    let cache_dir = tempdir("cache");
    let mut configs = config::Configs::new(&config_dir, &cache_dir).unwrap();
    configs.app_config.theme = "rose-pine".to_string();
    config::set_config(configs);

    let state = Arc::new(State::new(false, Arc::new(Mutex::new(VecDeque::new()))));
    state
        .connected
        .store(true, std::sync::atomic::Ordering::Relaxed);

    let playing = track(0);
    let cover = std::env::var("PREVIEW_COVER").ok();
    {
        let mut data = state.data.write();
        let user = serde_json::json!({
            "id": "you", "display_name": "Alex", "uri": "spotify:user:you",
            "external_urls": {}, "href": "", "followers": null, "images": null,
            "country": null, "email": null, "explicit_content": null, "product": null
        });
        data.user_data.user = serde_json::from_value(user).ok();
        data.user_data.playlists = PLAYLISTS
            .iter()
            .enumerate()
            .map(|(i, (name, desc))| PlaylistFolderItem::Playlist(playlist(name, desc, 100 + i)))
            .collect();
        for i in [0, 2, 5, 7, 12] {
            let t = track(i);
            data.user_data.saved_tracks.insert(t.id.uri(), t);
        }
        if let Some(path) = &cover {
            let img = image::open(path).expect("PREVIEW_COVER is an image");
            data.caches
                .images
                .insert(path.clone(), img, *TTL_CACHE_DURATION);
        }
        let pl = playlist(PLAYLISTS[0].0, PLAYLISTS[0].1, 100);
        let ctx = ContextId::Playlist(pl.id.clone());
        data.caches.context.insert(
            ctx.uri(),
            Context::Playlist {
                playlist: pl,
                tracks: (0..SONGS.len()).map(track).collect(),
            },
            *TTL_CACHE_DURATION,
        );
        data.caches.search.insert(
            "low tide".to_string(),
            SearchCacheEntry::Ready(SearchResults {
                tracks: [0, 4, 12, 1, 7, 2].into_iter().map(track).collect(),
                artists: vec![
                    artist("Low Tide Club", 1),
                    artist("Low Tide Orchestra", 2),
                    artist("Tidewater", 3),
                ],
                albums: vec![
                    album("Salt & Neon", "Low Tide Club", 10),
                    album("Low Tide EP", "Low Tide Club", 11),
                    album("High Water", "Tidewater", 12),
                ],
                playlists: vec![
                    playlist("Low Tide Club Radio", "", 20),
                    playlist("This Is Low Tide Club", "", 21),
                    playlist("Coastal Indie", "", 22),
                ],
                shows: vec![],
                episodes: vec![],
            }),
            *TTL_CACHE_DURATION,
        );
        data.caches.lyrics.insert(
            playing.id.uri(),
            Some(Lyrics {
                lines: [
                    "Harbour lights are blinking out",
                    "One by one along the bay",
                    "You said the tide would bring you back",
                    "But the water moves the other way",
                    "",
                    "And I'm still standing on the pier",
                    "Counting ships that never land",
                    "Glass harbour, hold me here",
                    "Like the shoreline holds the sand",
                    "",
                    "Neon signs in salted air",
                    "Every window paints the sea",
                    "If you're out there anywhere",
                    "Leave a light on, leave it for me",
                ]
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    (
                        chrono::Duration::seconds(8 * i as i64 + 4),
                        (*l).to_string(),
                    )
                })
                .collect(),
            }),
            *TTL_CACHE_DURATION,
        );
    }
    {
        let mut player = state.player.write();
        player.local = Some(LocalNowPlaying {
            uri: playing.id.uri(),
            name: playing.name.clone(),
            artists: "Low Tide Club".to_string(),
            album: "Salt & Neon".to_string(),
            cover_url: cover,
            duration: chrono::Duration::seconds(247),
            is_playing: true,
            position: chrono::Duration::seconds(52),
            updated_at: std::time::Instant::now(),
        });
        player.buffered_playback = Some(PlaybackMetadata {
            device_name: "spotify-tui".to_string(),
            device_id: None,
            volume: Some(70),
            is_playing: true,
            repeat_state: rspotify::model::RepeatState::Context,
            shuffle_state: true,
            mute_state: None,
        });
        let next: Vec<usize> = (1..12).collect();
        player.local_queue = Some(
            next.iter()
                .map(|&i| LocalQueueItem {
                    uri: track(i).id.uri(),
                    queued: i == 3,
                })
                .collect(),
        );
        for i in next {
            let t = track(i);
            player.queue_names.insert(
                t.id.uri(),
                QueueName {
                    name: t.name.clone(),
                    artists: t.artists[0].name.clone(),
                    duration: chrono::Duration::from_std(t.duration).unwrap(),
                },
            );
        }
    }
    set_wave(&state, 0.0);
    state
}

/// The wave at time `t` (seconds): strong lows, soft highs, moving texture.
fn set_wave(state: &SharedState, t: f32) {
    let Some(bands) = &state.vis_bands else {
        return;
    };
    let mut b = bands.lock();
    b.is_active = true;
    let n = b.values.len();
    for (i, v) in b.values.iter_mut().enumerate() {
        let x = i as f32 / n as f32;
        let texture = ((x * 41.0 + t * 5.0).sin() * (x * 13.0 - t * 3.0).cos()
            + (x * 97.0 + t * 11.0).sin() * 0.5)
            .abs();
        let beat = 0.8 + 0.2 * (t * 7.5).sin().abs();
        let bass = (-((x - 0.08) * 9.0).powi(2)).exp() * beat;
        *v = (0.25 + 0.75 * bass + 0.35 * (1.0 - x)) * (0.35 + 0.65 * texture.min(1.0));
    }
    b.peak_envelope = 1.0;
    b.updated_at = std::time::Instant::now();
}

/// Show `page` (home when `None`) and set the song's position.
fn show(state: &SharedState, page: Option<PageState>, position: f32) {
    let mut ui = state.ui.lock();
    ui.history.truncate(1);
    if let Some(page) = page {
        ui.new_page(page);
    }
    drop(ui);
    if let Some(local) = state.player.write().local.as_mut() {
        local.position = chrono::Duration::milliseconds((position * 1000.0) as i64);
        local.updated_at = std::time::Instant::now();
    }
}

fn tempdir(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("spotify-tui-preview-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn color(c: Color) -> serde_json::Value {
    match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}").into(),
        Color::Reset => serde_json::Value::Null,
        other => format!("{other:?}").into(),
    }
}

/// Render the current page and save it as `name`.
fn shoot(state: &SharedState, name: &str, out: &std::path::Path) {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    let mut cover = None;
    // twice: the first frame loads the cover
    for _ in 0..2 {
        terminal
            .draw(|frame| {
                let mut ui = state.ui.lock();
                super::shell::render(frame, state, &mut ui, Rect::new(0, 0, WIDTH, HEIGHT));
                let info = &ui.last_cover_image_render_info;
                cover = (info.state.is_some()).then(|| {
                    let r = info.render_area;
                    serde_json::json!({"x": r.x, "y": r.y, "w": r.width, "h": r.height})
                });
            })
            .unwrap();
    }
    let buf = terminal.backend().buffer();
    let rows: Vec<Vec<serde_json::Value>> = (0..HEIGHT)
        .map(|y| {
            (0..WIDTH)
                .map(|x| {
                    let c = &buf[(x, y)];
                    let m = c.modifier;
                    serde_json::json!([
                        c.symbol(),
                        color(c.fg),
                        color(c.bg),
                        m.contains(ratatui::style::Modifier::BOLD),
                        m.contains(ratatui::style::Modifier::ITALIC),
                        m.contains(ratatui::style::Modifier::REVERSED),
                        m.contains(ratatui::style::Modifier::DIM),
                    ])
                })
                .collect()
        })
        .collect();
    let json = serde_json::json!({"width": WIDTH, "height": HEIGHT, "rows": rows, "cover": cover});
    std::fs::write(out.join(format!("{name}.json")), json.to_string()).unwrap();
}

#[test]
#[ignore = "renders the README screenshots: scripts/screenshots"]
fn render_screenshots() {
    let out = std::path::PathBuf::from(std::env::var("PREVIEW_OUT").expect("PREVIEW_OUT"));
    std::fs::create_dir_all(&out).unwrap();
    let state = sample_state();

    let pages: [(&str, fn() -> Option<PageState>); 5] = [
        ("home", || None),
        ("playlist", playlist_page),
        ("search", search_page),
        ("lyrics", lyrics_page),
        ("queue", || Some(PageState::Queue { scroll_offset: 0 })),
    ];
    for (name, page) in pages {
        show(&state, page(), 52.0);
        set_wave(&state, 0.0);
        shoot(&state, name, &out);
    }

    // the animated preview: a few seconds on each page, 12 frames a second
    let frames = out.join("frames");
    std::fs::create_dir_all(&frames).unwrap();
    let mut n = 0;
    for (_, page) in &pages[..4] {
        for _ in 0..24 {
            let t = n as f32 / 12.0;
            show(&state, page(), 52.0 + t);
            set_wave(&state, t);
            shoot(&state, &format!("{n:03}"), &frames);
            n += 1;
        }
    }
}

fn playlist_page() -> Option<PageState> {
    let pl = ContextId::Playlist(PlaylistId::from_id(id(100)).unwrap());
    let mut page = ContextPageUIState::new_playlist();
    if let ContextPageUIState::Playlist { track_table } = &mut page {
        track_table.select(Some(3));
    }
    Some(PageState::Context {
        id: Some(pl.clone()),
        context_page_type: ContextPageType::Browsing(pl),
        state: Some(page),
    })
}

fn search_page() -> Option<PageState> {
    let mut search = SearchPageUIState::new();
    search.track_list.select(Some(0));
    search.focus = crate::state::SearchFocusState::Tracks;
    Some(PageState::Search {
        line_input: LineInput::new("low tide".chars().collect()),
        current_query: "low tide".to_string(),
        state: search,
    })
}

fn lyrics_page() -> Option<PageState> {
    let t = track(0);
    Some(PageState::Lyrics {
        track_uri: t.id.uri(),
        track: t.name,
        artists: "Low Tide Club".to_string(),
    })
}
