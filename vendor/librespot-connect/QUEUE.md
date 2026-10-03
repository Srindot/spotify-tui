# librespot-connect, vendored

librespot-connect 0.8.0 (MIT, https://github.com/librespot-org/librespot),
with public queue commands added to `Spirc` for spotify-tui:

- `queue()`: the upcoming tracks (`next_tracks`)
- `queue_remove(index)`, `queue_move(from, to)`: edit them
- `queue_play(index)`: skip ahead to one
- `queue_add(uri)`: add a track to the queue

Everything else is unchanged. The additions are marked `spotify-tui:`.
To update librespot, copy the new release over this folder and re-apply them.

Also fixed: turning shuffle off didn't restore the playlist order (upstream
only reset the position, so the rest kept playing shuffled). See
`handle_shuffle` in `src/state/handle.rs`.

Also added `Spirc::is_active()`: librespot ignores commands while it isn't
the active device (after reconnecting), so the app checks first and falls back
to the Web API, which re-activates it.
