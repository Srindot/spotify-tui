use crate::{client::AppClient, config, state::SharedState};
use anyhow::Context;
use librespot_connect::{ConnectConfig, Spirc};
use librespot_core::authentication::Credentials;
use librespot_core::config::DeviceType;
use librespot_core::{spotify_uri, Session, SpotifyUri};
use librespot_playback::audio_backend::Sink;
use librespot_playback::mixer::MixerConfig;
use librespot_playback::{
    audio_backend,
    config::{AudioFormat, Bitrate, PlayerConfig},
    mixer::{self, Mixer},
    player,
};
use rspotify::model::{EpisodeId, Id, PlayableId, TrackId};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Whether the next streaming connection is the first one of the process.
///
/// Used to scope `pause_on_startup` to application startup only, so that
/// reconnecting mid-session (e.g. via `RestartIntegratedClient`) does not
/// pause an intentionally playing track.
static IS_FIRST_CONNECTION: AtomicBool = AtomicBool::new(true);

#[cfg(not(any(
    feature = "rodio-backend",
    feature = "alsa-backend",
    feature = "pulseaudio-backend",
    feature = "portaudio-backend",
    feature = "jackaudio-backend",
    feature = "rodiojack-backend",
    feature = "sdl-backend",
    feature = "gstreamer-backend"
)))]
compile_error!("Streaming feature is enabled but no audio backend has been selected. Consider adding one of the following features:
    rodio-backend,
    alsa-backend,
    pulseaudio-backend,
    portaudio-backend,
    jackaudio-backend,
    rodiojack-backend,
    sdl-backend,
    gstreamer-backend
For more information, visit https://github.com/aome510/spotify-player?tab=readme-ov-file#streaming
");

#[derive(Debug, Serialize)]
enum PlayerEvent {
    Changed {
        playable_id: PlayableId<'static>,
    },
    Playing {
        playable_id: PlayableId<'static>,
        position_ms: u32,
    },
    Paused {
        playable_id: PlayableId<'static>,
        position_ms: u32,
    },
    EndOfTrack {
        playable_id: PlayableId<'static>,
    },
}

impl PlayerEvent {
    /// gets the event's arguments
    pub fn args(&self) -> Vec<String> {
        match self {
            PlayerEvent::Changed { playable_id } => {
                vec!["Changed".to_string(), playable_id.uri()]
            }
            PlayerEvent::Playing {
                playable_id,
                position_ms,
            } => vec![
                "Playing".to_string(),
                playable_id.uri(),
                position_ms.to_string(),
            ],
            PlayerEvent::Paused {
                playable_id,
                position_ms,
            } => vec![
                "Paused".to_string(),
                playable_id.uri(),
                position_ms.to_string(),
            ],
            PlayerEvent::EndOfTrack { playable_id } => {
                vec!["EndOfTrack".to_string(), playable_id.uri()]
            }
        }
    }
}

fn spotify_id_to_playable_id(uri: &spotify_uri::SpotifyUri) -> anyhow::Result<PlayableId<'static>> {
    match uri {
        SpotifyUri::Track { .. } => {
            let uri = uri.to_uri()?;
            Ok(TrackId::from_uri(&uri)?.into_static().into())
        }
        SpotifyUri::Episode { .. } => {
            let uri = uri.to_uri()?;
            Ok(EpisodeId::from_uri(&uri)?.into_static().into())
        }
        _ => anyhow::bail!("unexpected spotify_id {uri:?}"),
    }
}

impl PlayerEvent {
    pub fn from_librespot_player_event(e: player::PlayerEvent) -> anyhow::Result<Option<Self>> {
        Ok(match e {
            player::PlayerEvent::TrackChanged { audio_item } => Some(PlayerEvent::Changed {
                playable_id: spotify_id_to_playable_id(&audio_item.track_id)?,
            }),
            player::PlayerEvent::Playing {
                track_id,
                position_ms,
                ..
            } => Some(PlayerEvent::Playing {
                playable_id: spotify_id_to_playable_id(&track_id)?,
                position_ms,
            }),
            player::PlayerEvent::Paused {
                track_id,
                position_ms,
                ..
            } => Some(PlayerEvent::Paused {
                playable_id: spotify_id_to_playable_id(&track_id)?,
                position_ms,
            }),
            player::PlayerEvent::EndOfTrack { track_id, .. } => Some(PlayerEvent::EndOfTrack {
                playable_id: spotify_id_to_playable_id(&track_id)?,
            }),
            _ => None,
        })
    }
}

fn execute_player_event_hook_command(
    cmd: &config::Command,
    event: &PlayerEvent,
) -> anyhow::Result<()> {
    cmd.execute(Some(event.args()))?;

    Ok(())
}

/// Create a new streaming connection
/// Keep `PlayerState::local` in step with the integrated player, so the
/// now-playing display updates at once instead of waiting for the Web API.
fn track_local_playback(client: &AppClient, state: &SharedState, event: &player::PlayerEvent) {
    use crate::state::LocalNowPlaying;
    use librespot_metadata::audio::UniqueFields;
    let ms = |ms: u32| chrono::Duration::milliseconds(i64::from(ms));
    let mut player = state.player.write();
    match event {
        player::PlayerEvent::TrackChanged { audio_item } => {
            let (artists, album) = match &audio_item.unique_fields {
                UniqueFields::Track { artists, album, .. } => (
                    artists
                        .iter()
                        .map(|a| a.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    album.clone(),
                ),
                UniqueFields::Episode { show_name, .. } => (String::new(), show_name.clone()),
                UniqueFields::Local { artists, album, .. } => (
                    artists.clone().unwrap_or_default(),
                    album.clone().unwrap_or_default(),
                ),
            };
            // the largest cover (they come smallest first)
            let cover_url = audio_item
                .covers
                .iter()
                .max_by_key(|c| c.width)
                .map(|c| c.url.clone());
            let is_playing = player.local.as_ref().is_some_and(|l| l.is_playing);
            if player
                .parked
                .as_ref()
                .is_some_and(|p| p.uri != audio_item.uri)
            {
                player.parked = None;
            }
            player.local = Some(LocalNowPlaying {
                uri: audio_item.uri.clone(),
                name: audio_item.name.clone(),
                artists,
                album,
                cover_url: cover_url.clone(),
                duration: ms(audio_item.duration_ms),
                is_playing,
                position: chrono::Duration::zero(),
                updated_at: std::time::Instant::now(),
            });
            drop(player);
            if let Some(url) = cover_url {
                let (client, state) = (client.clone(), state.clone());
                tokio::spawn(async move {
                    if let Err(err) = client.load_cover_image(&state, &url).await {
                        tracing::warn!("Failed to load the cover image: {err:#}");
                    }
                });
            }
        }
        player::PlayerEvent::Playing { position_ms, .. }
        | player::PlayerEvent::Paused { position_ms, .. }
        | player::PlayerEvent::Seeked { position_ms, .. }
        | player::PlayerEvent::PositionCorrection { position_ms, .. }
        | player::PlayerEvent::PositionChanged { position_ms, .. } => {
            if matches!(event, player::PlayerEvent::Playing { .. }) {
                player.parked = None;
            }
            if let Some(local) = player.local.as_mut() {
                match event {
                    player::PlayerEvent::Playing { .. } => local.is_playing = true,
                    player::PlayerEvent::Paused { .. } => local.is_playing = false,
                    _ => {}
                }
                local.position = ms(*position_ms);
                local.updated_at = std::time::Instant::now();
            }
        }
        player::PlayerEvent::ShuffleChanged { shuffle } => {
            player.local_options.0 = *shuffle;
            if let Some(p) = player.buffered_playback.as_mut() {
                p.shuffle_state = *shuffle;
            }
        }
        player::PlayerEvent::RepeatChanged { context, track } => {
            (player.local_options.1, player.local_options.2) = (*context, *track);
            if let Some(p) = player.buffered_playback.as_mut() {
                p.repeat_state = if *track {
                    rspotify::model::RepeatState::Track
                } else if *context {
                    rspotify::model::RepeatState::Context
                } else {
                    rspotify::model::RepeatState::Off
                };
            }
        }
        // stopped here, e.g. playback moved to another device
        player::PlayerEvent::Stopped { .. } | player::PlayerEvent::SessionDisconnected { .. } => {
            if let Some(mut stopped) = player.local.take() {
                stopped.position = stopped.progress().min(stopped.duration);
                stopped.is_playing = false;
                // Spotify's takeover after a reconnect loads the song at 0:00
                // and drops it again; keep where it really was
                let same = player.parked.as_ref().is_some_and(|p| p.uri == stopped.uri);
                if !same || stopped.position > chrono::Duration::zero() {
                    player.parked = Some(stopped);
                }
            }
        }
        _ => {}
    }
}

pub async fn new_connection(
    client: AppClient,
    state: SharedState,
    session: Session,
    creds: Credentials,
) -> anyhow::Result<(Spirc, Arc<player::Player>)> {
    let configs = config::get_config();
    let device = &configs.app_config.device;

    // `librespot` volume is a u16 number ranging from 0 to 65535,
    // while a percentage volume value (from 0 to 100) is used for the device configuration.
    // So we need to convert from one format to another
    let volume = (f64::from(std::cmp::min(device.volume, 100_u8)) / 100.0 * 65535.0).round() as u16;

    let connect_config = ConnectConfig {
        name: device.name.clone(),
        device_type: device.device_type.parse::<DeviceType>().unwrap_or_default(),
        initial_volume: volume,

        // non-configurable fields, use default values.
        // We may allow users to configure these fields in a future release
        is_group: false,
        disable_volume: false,
        volume_steps: 64,
    };

    tracing::info!("Application's connect configurations: {:?}", connect_config);

    let mixer = Arc::new(
        mixer::softmixer::SoftMixer::open(MixerConfig::default()).context("opening softmixer")?,
    );
    mixer.set_volume(volume);

    let backend = audio_backend::find(None).expect("should be able to find an audio backend");
    let player_config = PlayerConfig {
        bitrate: device
            .bitrate
            .to_string()
            .parse::<Bitrate>()
            .unwrap_or_default(),
        normalisation: device.normalization,
        ..Default::default()
    };

    tracing::info!(
        "Initializing a new integrated player with device_id={}",
        session.device_id()
    );

    let player = {
        // Clone the Option<Arc<...>> so the factory closure can move it.
        // vis_bands is Some iff enable_audio_visualization is true.
        let vis_bands = state.vis_bands.as_ref().map(Arc::clone);
        player::Player::new(
            player_config,
            session.clone(),
            mixer.get_soft_volume(),
            move || -> Box<dyn Sink> {
                let real = backend(None, AudioFormat::default());
                if let Some(ref bands) = vis_bands {
                    Box::new(crate::ui::streaming::VisualizationSink::new(
                        real,
                        Arc::clone(bands),
                        // librespot defaults to 44100 Hz; adjust here if
                        // PlayerConfig::sample_rate is changed in the future.
                        44_100.0,
                    ))
                } else {
                    real
                }
            },
        )
    };

    // When `pause_on_startup` is enabled, suppress Spotify's auto-resume of the
    // previous session by pausing the first auto-started playback. Scoped to the
    // first connection of the process so mid-session reconnects are unaffected.
    let pause_on_startup =
        configs.app_config.pause_on_startup && IS_FIRST_CONNECTION.swap(false, Ordering::SeqCst);

    let player_event_task = tokio::task::spawn({
        let mut channel = player.get_player_event_channel();
        async move {
            let mut pause_armed = pause_on_startup;
            while let Some(event) = channel.recv().await {
                // Suppress Spotify's auto-resume of the previous session on
                // startup. The `librespot` connect transfer finalizes the
                // play state asynchronously, so a single reactive pause is not
                // reliable on its own:
                if pause_armed {
                    match &event {
                        // Best-effort: pause as the track starts loading, before
                        // the audio sink starts, so no audible blip occurs. This
                        // is a no-op if the transfer has not set the play state
                        // yet, so we do NOT disarm here.
                        player::PlayerEvent::Loading { .. } => {
                            client.pause_streaming_on_startup();
                        }
                        // Authoritative: playback actually started (the transfer
                        // finalized into "playing"). Pause and stop interfering.
                        player::PlayerEvent::Playing { .. } => {
                            if client.pause_streaming_on_startup() {
                                pause_armed = false;
                            }
                        }
                        // The track finished loading already paused, i.e. the
                        // `Loading` pause above took effect and no audio played.
                        player::PlayerEvent::Paused { .. } => {
                            pause_armed = false;
                        }
                        _ => {}
                    }
                }

                track_local_playback(&client, &state, &event);

                match PlayerEvent::from_librespot_player_event(event) {
                    Err(err) => {
                        tracing::warn!("Failed to convert a `librespot` player event into `spotify_player` player event: {err:#}");
                    }
                    Ok(Some(event)) => {
                        tracing::info!("Got a new player event: {event:?}");
                        match event {
                            PlayerEvent::Playing { .. } => {
                                let mut player = state.player.write();
                                if let Some(playback) = player.buffered_playback.as_mut() {
                                    playback.is_playing = true;
                                }
                                if let Some(ref bands) = state.vis_bands {
                                    bands.lock().is_active = true;
                                }
                            }
                            PlayerEvent::Paused { .. } => {
                                let mut player = state.player.write();
                                if let Some(playback) = player.buffered_playback.as_mut() {
                                    playback.is_playing = false;
                                }
                                if let Some(ref bands) = state.vis_bands {
                                    bands.lock().is_active = false;
                                }
                            }
                            _ => {}
                        }
                        client.update_playback_non_blocking(&state);

                        // execute a player event hook command
                        if let Some(ref cmd) = configs.app_config.player_event_hook_command {
                            if let Err(err) = execute_player_event_hook_command(cmd, &event) {
                                tracing::warn!(
                                    "Failed to execute player event hook command: {err:#}"
                                );
                            }
                        }
                    }
                    Ok(None) => {}
                }
            }
        }
    });

    tracing::info!("Starting an integrated Spotify player using librespot's spirc protocol");

    let player_handle = Arc::clone(&player);
    let (spirc, spirc_task) = Spirc::new(connect_config, session, creds, player, mixer)
        .await
        .context("initialize spirc")?;

    tokio::task::spawn(async move {
        tokio::select! {
            () = spirc_task => {},
            _ = player_event_task => {}
        }
    });

    tracing::info!("New streaming connection has been established!");

    Ok((spirc, player_handle))
}
