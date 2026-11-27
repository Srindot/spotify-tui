# Keys, actions and CLI

Every key below is a default; change any of them in `keymap.toml` (see
[Keymaps](config.md#keymaps)). Press `?` in the app for the same list,
searchable with `/`.

Keys written `g y` are pressed one after the other. `C-` is Ctrl, so `C-s` is
Ctrl+S. Movement keys take a count like in Vim: `5j` moves down five rows.

## Playback

| Key            | Command          | Does                                       |
| -------------- | ---------------- | ------------------------------------------ |
| `space`        | `ResumePause`    | play / pause                               |
| `n`            | `NextTrack`      | next song                                  |
| `p`            | `PreviousTrack`  | previous song                              |
| `.`            | `PlayRandom`     | play a random song from the current list   |
| `C-s`          | `Shuffle`        | shuffle on / off                           |
| `C-r`          | `Repeat`         | repeat: off, playlist, song                |
| `+` `-`        | `VolumeChange`   | volume up / down by 5%                     |
| `_`            | `Mute`           | mute / unmute                              |
| `>` `<`        | `SeekForward` `SeekBackward` | seek 5 s (`3>` seeks 15 s)     |
| `^`            | `SeekStart`      | back to the start of the song              |
| `D`            | `SwitchDevice`   | play on another device (Spotify Connect)   |
| `R`            | `RestartIntegratedClient` | reconnect the built-in player     |
| `r`            | `RefreshPlayback` | refresh what's playing                    |

## Going places

| Key                  | Command                        | Opens                                  |
| -------------------- | ------------------------------ | -------------------------------------- |
| `S`, `g s`           | `SearchPage`                   | search                                 |
| `g l`                | `LibraryPage`                  | home: now playing and your library     |
| `g y`                | `LikedTrackPage`               | liked songs                            |
| `g r`                | `RecentlyPlayedTrackPage`      | recently played                        |
| `g t`                | `TopTrackPage`                 | your top tracks                        |
| `g b`                | `BrowsePage`                   | browse: categories and genres          |
| `l`, `g L`           | `LyricsPage`                   | synced lyrics of the playing song      |
| `z`                  | `Queue`                        | the queue (editable, see below)        |
| `g space`            | `CurrentlyPlayingContextPage`  | the playlist or album that's playing   |
| `g c`                | `JumpToCurrentTrackInContext`  | the playing song in its list           |
| `u p` `u a` `u A`    | `BrowseUser...`                | pick a playlist / artist / album       |
| `O`                  | `OpenSpotifyLinkFromClipboard` | the Spotify link you copied            |
| `backspace`, `C-q`   | `PreviousPage`                 | back                                   |
| `?`, `f`, `C-h`      | `OpenCommandHelp`              | every key                              |
| `g o`                | `OpenLogs`                     | the log                                |
| `q`, `C-c`           | `Quit`                         | quit                                   |

## Lists

| Key                     | Command                         | Does                              |
| ----------------------- | ------------------------------- | --------------------------------- |
| `j` `k`, `down` `up`    | `SelectNext...` `SelectPrevious...` | move                          |
| `C-f` `C-b`             | `PageSelect...`                 | move a page                       |
| `g g`, `G`              | `SelectFirst...` `SelectLast...` | top / bottom                     |
| `enter`                 | `ChooseSelected`                | play the song, open the item      |
| `/`                     | `Search`                        | filter the current list           |
| `tab`, `backtab`        | `FocusNextWindow` ...           | next / previous box               |
| `C-g`                   | `JumpToHighlightTrackInContext` | open a search result in its album |
| `s t` `s a` `s A` `s d` `s D` | `SortTrackBy...`          | sort by title, artist, album, length, date added |
| `s r`                   | `ReverseTrackOrder`             | reverse the order                 |
| `s l a`, `s l r`        | `SortLibrary...`                | sort the library A–Z / by recent  |
| `T`                     | `SwitchTheme`                   | pick a theme                      |

## Songs and playlists

| Key          | Does                                                     |
| ------------ | -------------------------------------------------------- |
| `L`          | like / unlike the selected song                          |
| `C-l`        | like / unlike the playing song                           |
| `P`          | add the selected song to a playlist                      |
| `X`          | remove the selected song from the playlist               |
| `Z`, `C-z`   | add the selected song (or album, playlist) to the queue  |
| `N`          | new playlist                                             |
| `C-k` `C-j`  | move the selected song up / down in your playlist        |
| `C-space`, `g a` | every action on the selected item                    |
| `a`          | every action on the playing song                         |
| `A`          | every action on the open playlist or album               |

The queue page (`z`), while playing in the app: `enter` plays from that song,
`X` removes it, `C-k` / `C-j` move it.

## Actions

`C-space` and `a` open a menu of what can be done with an item. Any of these
can also be bound to a key in `keymap.toml` (see [Actions](config.md#actions)):

`GoToArtist`, `GoToAlbum`, `GoToRadio`, `AddToLibrary`, `AddToPlaylist`,
`AddToQueue`, `AddToLiked`, `DeleteFromLiked`, `DeleteFromLibrary`,
`DeleteFromPlaylist`, `ShowActionsOnAlbum`, `ShowActionsOnArtist`,
`ShowActionsOnShow`, `ToggleLiked`, `CopyLink`, `Follow`, `Unfollow`.

## Command line

`spotify_tui` with a subcommand talks to the running app (or starts a
background one), which makes it scriptable:

| Command                     | Does                                                    |
| --------------------------- | ------------------------------------------------------- |
| `spotify_tui authenticate`  | log in again                                            |
| `spotify_tui playback ...`  | play, pause, next, seek, volume, shuffle, start a list  |
| `spotify_tui like`          | like the playing song                                   |
| `spotify_tui search QUERY`  | search, printed as JSON                                 |
| `spotify_tui get ...`       | your playlists, albums, a playlist's songs, ... as JSON |
| `spotify_tui playlist ...`  | create, delete, import, fork playlists                  |
| `spotify_tui connect ...`   | move playback to a device                               |
| `spotify_tui lyrics`        | print the playing song's lyrics                         |
| `spotify_tui generate SHELL`| shell completions                                       |

`spotify_tui <command> -h` shows the options. For example, bind media keys in
your window manager to `spotify_tui playback play-pause`, or play the first
search result:

```sh
spotify_tui playback start track --id "$(spotify_tui search "$1" | jq -r '.tracks[0].id')"
```

The app listens for these on port `8080` (`client_port` in `app.toml`).
