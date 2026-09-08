# Changelog

Each release's notes are taken from here automatically, so this file is the
one place a change gets written down. Every release gets an entry, and CI
fails a version bump that arrives without one.

A heading may also carry a name after the version, and that name becomes the
release title on GitHub. Those belong on feature releases, the x.y.0 ones, and
never on a patch. They are optional even then.

## 1.2.2 - The Narcissistic Update

- The about screen says who made Snag, and links to their GitHub. It credited
  cobalt for the inspiration, yt-dlp for the work and the GPL for the terms,
  and never mentioned the author.

## 1.2.1

- The YouTube sign-in screen now says what using it costs you, before you pick
  a browser rather than after: downloading from YouTube is against their terms
  whether you are signed in or not, signing in ties that activity to your
  account in a way anonymous downloading does not, and an exported cookie file
  is as good as your password. Use a second Google account for it.

## 1.2.0 - Sign In

- Sign in to YouTube for age restricted, private and members-only videos.
  There is no password box: you sign in with your own browser as usual, pick
  that browser, and Snag borrows the session. The session is scoped to YouTube
  links by default, so it is never offered to other sites.
- A check sign-in button that says plainly whether it worked, and what went
  wrong when it did not.
- Chromium browsers on Windows are called out as unusable before you try them.
  Chrome, Edge, Brave, Opera and Vivaldi seal their cookie store so that only
  the browser can read it. Firefox works, and so does an exported cookies.txt.
- The link preview carries the sign-in too. It did not before, so a
  members-only link would fail to preview and then download perfectly well.

## 1.1.0 - Picture This

- The link preview shows the video's thumbnail beside the title, channel and
  duration. Fetched once per link, scaled down before it becomes a texture,
  and dropped when the link changes.

## 1.0.0

- Link preview: paste a link and Snag says what it is before downloading it.
- Playlists are recognised and you are asked whether you want the one item you
  linked or all of them.
- yt-dlp's errors are translated into something you can act on.
- History of finished downloads, remembered across restarts.
- Background mode: closing the window can leave Snag in the system tray.
- Clipboard watching: copy a link anywhere and Snag offers it. Opt in, and off
  by default.
- Fixed: the paste button had never worked on Windows in any release, because
  arboard was built without its default features.

## 0.6.0

- Shows what a link is before downloading it, and remembers what was
  downloaded.

## 0.5.0

- Installs ffmpeg through the platform's own package manager.

## 0.4.0

- Linux AppImage.

## 0.3.0

- Snag updates itself, and links to yt-dlp's list of supported services.

## 0.2.0

- The real logo, a Windows installer, and a portable build.

## 0.1.0

- First release.
