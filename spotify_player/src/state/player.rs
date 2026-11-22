use super::model::{
    AlbumId, ArtistId, ContextId, Device, PlaybackMetadata, PlaylistId, ShowId, TracksId,
};
use super::queue::CustomQueue;

/// Player state
#[derive(Default, Debug)]
pub struct PlayerState {
    pub devices: Vec<Device>,

    pub playback: Option<rspotify::model::CurrentPlaybackContext>,
    pub playback_last_updated_time: Option<std::time::Instant>,
    /// A buffered state to speedup the feedback of playback metadata update to user
    // Related issue: https://github.com/aome510/spotify-player/issues/109
    pub buffered_playback: Option<PlaybackMetadata>,

    pub queue: Option<rspotify::model::CurrentUserQueue>,

    /// The currently playing Tracks context (for contexts not tracked by Spotify's playback, e.g. liked/top tracks)
    pub currently_playing_tracks_id: Option<TracksId>,

    /// App-managed custom queue for full playlist/album playback.
    /// Active when the integrated librespot player is streaming and the user
    /// started playback from a track-table context.
    pub custom_queue: Option<CustomQueue>,

    /// What the integrated player (librespot) is playing, straight from its
    /// events. The Web API's view (`playback`) can lag seconds behind, or not
    /// update at all while requests are rate limited; the now-playing display
    /// prefers this one.
    pub local: Option<LocalNowPlaying>,

    /// The song the integrated player was on when it stopped (Spotify
    /// deactivates an idle paused player), to resume it from where it was.
    pub parked: Option<LocalNowPlaying>,
    /// The integrated player's shuffle, repeat-context and repeat-track.
    #[cfg_attr(not(feature = "streaming"), allow(dead_code))]
    pub local_options: (bool, bool, bool),

    /// What's coming up on the integrated player, straight from librespot
    /// (editable: remove, move, play from). `None` when not playing here.
    pub local_queue: Option<Vec<LocalQueueItem>>,
    /// Names for queue entries (librespot only gives uris), by uri.
    pub queue_names: std::collections::HashMap<String, QueueName>,
}

/// An upcoming track on the integrated player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalQueueItem {
    pub uri: String,
    /// added with "add to queue" (not just the next song of the playlist)
    pub queued: bool,
}

/// The display name of a queued track.
#[derive(Clone, Debug)]
pub struct QueueName {
    pub name: String,
    pub artists: String,
    pub duration: chrono::Duration,
}

/// The integrated player's current track and position.
#[derive(Clone, Debug)]
pub struct LocalNowPlaying {
    pub uri: String,
    pub name: String,
    pub artists: String,
    pub album: String,
    pub cover_url: Option<String>,
    pub duration: chrono::Duration,
    pub is_playing: bool,
    /// the position at `updated_at`
    pub position: chrono::Duration,
    pub updated_at: std::time::Instant,
}

impl LocalNowPlaying {
    /// The position now, advancing while it plays.
    pub fn progress(&self) -> chrono::Duration {
        let elapsed = if self.is_playing {
            chrono::Duration::from_std(self.updated_at.elapsed()).unwrap_or_default()
        } else {
            chrono::Duration::zero()
        };
        (self.position + elapsed).clamp(chrono::Duration::zero(), self.duration)
    }
}

impl PlayerState {
    /// Get the current playback
    ///
    /// # Note
    /// Because playback metadata stored inside the player state is buffered,
    /// the returned playback is estimated based on the available data.
    pub fn current_playback(&self) -> Option<rspotify::model::CurrentPlaybackContext> {
        let mut playback = self.playback.clone()?;

        // update the playback's progress based on the `playback_last_updated_time`
        playback.progress = playback.progress.map(|d| {
            d + if playback.is_playing {
                chrono::Duration::from_std(self.playback_last_updated_time.unwrap().elapsed())
                    .unwrap()
            } else {
                chrono::Duration::zero()
            }
        });

        // update the playback's metadata based on the `buffered_playback` metadata
        if let Some(ref p) = self.buffered_playback {
            playback.device.name.clone_from(&p.device_name);
            playback.device.id.clone_from(&p.device_id);
            playback.is_playing = p.is_playing;
            playback.device.volume_percent = p.volume;
            playback.repeat_state = p.repeat_state;
            playback.shuffle_state = p.shuffle_state;
        }

        Some(playback)
    }

    pub fn currently_playing(&self) -> Option<&rspotify::model::PlayableItem> {
        self.playback.as_ref().and_then(|p| p.item.as_ref())
    }

    /// The position of what's playing as shown on screen: the integrated
    /// player's own when it plays here (the Web API's can be stale or
    /// missing, which made seeking do nothing), else the Web API's.
    pub fn current_progress(&self) -> Option<chrono::Duration> {
        match &self.local {
            Some(local) => Some(local.progress().min(local.duration)),
            None => self.playback_progress(),
        }
    }

    pub fn playback_progress(&self) -> Option<chrono::Duration> {
        match self.playback {
            None => None,
            Some(ref playback) => {
                let progress = playback.progress.unwrap()
                    + if playback.is_playing {
                        chrono::Duration::from_std(
                            self.playback_last_updated_time.unwrap().elapsed(),
                        )
                        .ok()?
                    } else {
                        chrono::Duration::zero()
                    };
                Some(progress)
            }
        }
    }

    pub fn playing_context_id(&self) -> Option<ContextId> {
        match self.playback {
            Some(ref playback) => match playback.context {
                Some(ref context) => {
                    let uri = crate::utils::parse_uri(&context.uri);
                    match context._type {
                        rspotify::model::Type::Playlist => Some(ContextId::Playlist(
                            PlaylistId::from_uri(&uri).ok()?.into_static(),
                        )),
                        rspotify::model::Type::Album => Some(ContextId::Album(
                            AlbumId::from_uri(&uri).ok()?.into_static(),
                        )),
                        rspotify::model::Type::Artist => Some(ContextId::Artist(
                            ArtistId::from_uri(&uri).ok()?.into_static(),
                        )),
                        rspotify::model::Type::Show => {
                            Some(ContextId::Show(ShowId::from_uri(&uri).ok()?.into_static()))
                        }
                        _ => None,
                    }
                }
                None => self
                    .custom_queue
                    .as_ref()
                    .and_then(|q| q.source_context().cloned())
                    .or_else(|| {
                        self.currently_playing_tracks_id
                            .clone()
                            .map(ContextId::Tracks)
                    }),
            },
            None => None,
        }
    }
}

#[cfg(test)]
mod local_now_playing_tests {
    use super::LocalNowPlaying;
    use std::time::{Duration, Instant};

    fn local(is_playing: bool, position_s: i64, ago: Duration) -> LocalNowPlaying {
        LocalNowPlaying {
            uri: "spotify:track:x".into(),
            name: "Nights".into(),
            artists: "Frank Ocean".into(),
            album: "Blonde".into(),
            cover_url: None,
            duration: chrono::Duration::seconds(307),
            is_playing,
            position: chrono::Duration::seconds(position_s),
            updated_at: Instant::now() - ago,
        }
    }

    #[test]
    fn progress_advances_only_while_playing() {
        let ten = Duration::from_secs(10);
        assert_eq!(local(true, 60, ten).progress().num_seconds(), 70);
        assert_eq!(local(false, 60, ten).progress().num_seconds(), 60);
        // never past the end
        assert_eq!(local(true, 300, ten).progress().num_seconds(), 307);
    }
}
