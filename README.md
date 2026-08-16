# myradio

A terminal UI (TUI) for listening to internet radio. Searches stations via
[Radio Browser](https://radio-browser.info), plays them with rodio
(MP3, AAC, OGG/Vorbis, FLAC, WAV), and shows the volume level in real time
(dBFS meter + level history as a sparkline). Current version: **1.4.0**.

It is fully usable with the keyboard **and the mouse** (click to focus/search or
to select and play a station, scroll wheel to navigate), ships with an animated
splash screen (ASCII-art logo + FM tuner) and renders station artwork with
half-blocks so it works in any terminal.

## Build

- **Linux**: `pkg-config` + ALSA development headers (rodio's ALSA backend)
  ```sh
  # Debian/Ubuntu
  sudo apt install -y pkg-config libasound2-dev
  # Arch Linux (and derivatives)
  sudo pacman -S --needed pkgconf alsa-lib
  # Fedora
  sudo dnf install -y pkgconf-pkg-config alsa-lib-devel
  ```
- **Windows**: no system dependencies (WASAPI); TLS via rustls
- **macOS**: native build only (no system dependencies)

```sh
cargo build --release
```

On startup the app shows a splash screen with an ASCII-art `myradio` logo, the
version, and an animated FM tuner scanning stations. The splash is dismissed by
any key or after 10 seconds.

## Demo

<video src="demo/myradio_demo.mp4" controls width="640" height="480"></video>

### Screenshots

![Demo 1](images/demo1.png)
![Demo 2](images/demo2.png)
![Demo 3](images/demo3.png)

## Release builds for Linux, Windows and macOS

`scripts/build-release.sh` builds and copies the final binary into `dist/`:

```sh
scripts/build-release.sh          # native build for the current OS
scripts/build-release.sh --all    # native + cross-compiled Windows (from Linux)
scripts/build-release.sh --windows
```

- Linux: requires `pkg-config` + ALSA development headers (see the
  distribution-specific commands in the Build section above).
- Windows: from Linux needs `mingw-w64` (`sudo apt install -y mingw-w64`);
  on Windows a native build is enough.
- macOS: native build only; cross-compilation from other hosts is unsupported.

## Usage

```sh
cargo run --release
```

| Key | Action |
|---|---|
| `/` or `i` | focus the name search box |
| `t` | focus the tag filter |
| `Tab` | next field |
| `Enter` | run search / play the selected station |
| `↑`/`↓` or `j`/`k` | move selection |
| `p` or `Space` | pause/resume (or start selection) |
| `s` | stop |
| `+` / `-` | volume |
| `f` | add/remove the selected station as favorite |
| `F` | show the favorites list |
| `v` | toggle the audio visualizer |
| `q` or `Ctrl-C` | quit |

Mouse support is enabled automatically:

| Mouse | Action |
|---|---|
| left-click on a search box | focus that field |
| left-click on a result row | select and play that station |
| scroll wheel over the results | move selection up/down |

## Favorites

Press `f` on a station to add or remove it from your favorites (a `★` marks
favorites in the list, and the station info panel shows "★ Preferito"). `F`
shows the favorites list. On startup the app shows your favorites in the list;
if you have no favorites yet the results panel stays empty.

Favorites are saved as JSON in the user data directory:

- **Linux**: `$XDG_DATA_HOME/myradio/favorites.json` (`~/.local/share/myradio/…` by default)
- **macOS**: `~/Library/Application Support/myradio/favorites.json`
- **Windows**: `%APPDATA%\myradio\favorites.json`

A missing or corrupted file is ignored and treated as an empty list.

## Logs

Logs are written to `logs/` (never to the TUI): one file per day
(`myradio.<date>.log`), at most 7 files are kept, and only `WARN`/errors are
recorded.

## Architecture

- `src/radio.rs` — Radio Browser provider and `Station` model
- `src/audio.rs` — rodio engine on a dedicated thread, interruptible/rewindable
  network reader, `LevelSource` for real-time level sampling
- `src/levels.rs` — shared dBFS↔percentage levels
- `src/artwork.rs` — async favicon download, disk + memory cache, and
  half-block rendering of station artwork (works in any terminal)
- `src/app.rs` — state machine and input handling (keyboard + mouse)
- `src/favorites.rs` — favorites persistence (JSON in the user data directory)
- `src/ui.rs` — ratatui rendering (header, search, results, station info, audio
  meter/history, status bar, help, animated startup splash)
- `src/main.rs` — entry point and logging setup

## Verification

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```