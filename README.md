# Snag

A small, fast, native desktop video downloader. Rust + egui, no webview, no
bundled runtime: one ~5 MB binary that starts instantly.

Snag is a front end for **yt-dlp**. It builds the yt-dlp command from your
settings, runs it, and reads the progress back. Merging, remuxing and audio
conversion are handled by **ffmpeg**, which yt-dlp calls on its own.

## First run

Snag asks three things once, then never again:

1. **where settings live** &mdash; defaults to `%APPDATA%\Snag\config`, or pick any
   folder (a portable install next to the binary, say). The choice is recorded in
   a one-line pointer file at the default location, so Snag can find it next time.
2. **where downloads go** &mdash; defaults to your Downloads folder.
3. **yt-dlp** &mdash; Snag looks for it on `PATH`. If it is not there, one click
   downloads the current release from the yt-dlp project's own GitHub releases
   into `<config>/bin`.

**Snag does not ship yt-dlp.** It is a separate project on its own release
cadence, and a bundled copy would be stale the week after release. ffmpeg is
also not installed by Snag: get it from ffmpeg.org or your package manager.

## What it does

**save** &mdash; paste a link, pick a mode, download.

| mode | what you get |
| --- | --- |
| `auto` | video + audio, merged into one file |
| `audio` | audio track only, converted to your chosen format |
| `mute` | video only, audio track dropped |

Paste several links at once (one per line) and they all queue up.

**queue** &mdash; live progress, speed, ETA and size per job, with cancel, retry,
open, show-in-folder, copy-link and a per-job log. Runs several downloads at
once, up to the limit you set.

**remux** &mdash; work on a file you already have, without re-downloading:

- change container (mp4 / mkv / webm / mov), stream-copied, near-instant
- extract audio (copy, or re-encode to aac / mp3 / opus / flac)
- mute, dropping the audio track and keeping the video untouched
- convert to GIF, with frame rate and width controls

Drag a file onto the window to load it straight into remux.

**updates** &mdash; sites break yt-dlp often, so Snag reads the latest release tag
from GitHub and compares it to your installed version. Check never, on launch,
daily or weekly, and either install on a click or let it install automatically.
The install runs yt-dlp's own self-update.

## Settings

- **appearance** &mdash; dark / dim / light, six accents, interface scale, compact queue
- **video** &mdash; quality up to 8k, codec (h264+aac / av1+opus / vp9+opus), container,
  h265 toggle, prefer free formats, frame rate cap
- **audio** &mdash; format (best / mp3 / ogg / wav / opus), bitrate, prefer better
  quality, loudness normalisation, preferred dub language
- **metadata** &mdash; embed metadata, thumbnail, chapters and subtitles; save the
  thumbnail separately; SponsorBlock removal; keep the original upload date
- **local processing** &mdash; download folder, output template with presets, restrict
  file names, overwrite policy, keep source after remux, downloads at once,
  fragments per download
- **network** &mdash; proxy, speed limit, retries, socket timeout, cookies from a
  browser profile or a cookies.txt, custom user agent
- **advanced** &mdash; yt-dlp and ffmpeg paths, ignore playlists, verbose log,
  extra yt-dlp arguments, and a live preview of the exact command Snag runs

Settings are saved automatically to `settings.json` in the platform config
directory (`%APPDATA%\Snag` on Windows).

## Requirements

- **yt-dlp** &mdash; on `PATH`, installed by Snag on first run, or pointed at in
  settings &gt; advanced
- **ffmpeg** &mdash; on `PATH` (needed for merging, remuxing and audio conversion).
  Snag does not install this one.

## Build

```bash
cargo build --release
```

The binary lands at `snag/target/release/snag.exe`. Debug builds keep a console
window; release builds do not.

## Command line

```bash
snag --view=settings                 # open straight to a screen
snag --print-command <link>          # print the yt-dlp invocation and exit
snag --print-command <link> --mode=audio
snag --install-ytdlp                 # install yt-dlp headlessly and exit
snag --install-ytdlp /some/dir       # ...into a specific folder
```

`--print-command` prints one argument per line using the current settings, which
is the quickest way to see exactly what Snag would run.

## Layout

```
src/
  main.rs        entry point, CLI flags, window setup
  app.rs         application state, event pumps, view routing
  theme.rs       palette and the custom widgets (pills, toggles, bars)
  settings.rs    every setting, with serde persistence
  ytdlp.rs       argument construction and progress-line parsing
  jobs.rs        the download queue and its worker threads
  remux.rs       ffmpeg operations
  updater.rs     version check and self-update
  bootstrap.rs   resolves where the config lives
  installer.rs   fetches yt-dlp from its GitHub releases
  icon.rs        the window icon, rasterized at startup
  ui/            one module per screen
```

Progress is read through a custom `--progress-template`, so parsing does not
depend on yt-dlp's human-readable output format. Every job runs on its own
thread with a killable child process, and nothing blocks the render loop.

## Builds

CI runs `cargo fmt --check`, `cargo clippy -D warnings` and a release build on
Windows, Linux and macOS for every push. Pushing a `v*` tag builds binaries for
all three and publishes them to a GitHub release.

## Credit

Snag's interface is **heavily inspired by [cobalt.tools](https://cobalt.tools)**:
the mode pills, the single link field, the plain-language settings copy, and the
remux idea all come from there.

Snag is **not affiliated with, endorsed by, or connected to cobalt or its
developers in any way.** It is a separate project that borrows their design
ideas, and any faults in it are Snag's own.

The downloading is done by [yt-dlp](https://github.com/yt-dlp/yt-dlp) and the
media work by [ffmpeg](https://ffmpeg.org). Both are separate projects; Snag
simply drives them.

## A note

You are responsible for what you download. Respect the terms of the sites you
use and the rights of the people who made what you are saving.
