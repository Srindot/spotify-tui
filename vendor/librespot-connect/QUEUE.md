# librespot-connect, vendored

librespot-connect 0.8.0 (MIT, https://github.com/librespot-org/librespot),
with public queue commands added to `Spirc` for spotify-tui:

- `queue()`: the upcoming tracks (`next_tracks`)
- `queue_remove(index)`, `queue_move(from, to)`: edit them
- `queue_play(index)`: skip ahead to one
- `queue_add(uri)`: add a track to the queue

Everything else is unchanged. The additions are marked `spotify-tui:`.
To update librespot, copy the new release over this folder and re-apply them.
