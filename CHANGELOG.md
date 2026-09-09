# Changelog

Each release's notes are taken from here automatically, so this file is the
one place a change gets written down. Every release gets an entry, and CI
fails a version bump that arrives without one.

A heading may also carry a name after the version, and that name becomes the
release title on GitHub. Those belong on feature releases, the x.y.0 ones, and
never on a patch. They are optional even then.

## 1.4.0

- **Screen reader support.** Snag was invisible to assistive software, and is
  not any more. Two things were wrong. The accessibility integration had been
  switched off along with everything else when eframe's default features were
  disabled to keep the binary small, which is a poor trade against being
  unusable. And Snag draws its own buttons, pills, switches and navigation
  straight onto the canvas rather than using egui's stock widgets, so even with
  the integration on they would have arrived as unlabelled shapes.
  - Every control now says what it is, whether it is on, and what it is called.
    The queue badge is part of the name, so it reads as "queue, 3".
  - Text boxes are named too. The link box announces itself as "link", and a
    settings field takes the name of its row rather than its placeholder.
  - Checked against the Windows UI Automation interface that screen readers
    actually consume, rather than assumed from the code.
  - Costs about 230 KB of binary.

## 1.3.0 - Snip Snip

- **Clip a section.** After a download finishes, a clip button on the queue and
  in history opens it ready to cut, so you choose the part you want having seen
  the whole thing rather than having to guess beforehand. It is also an
  operation on the remux screen for any file you already have. Times are
  written the way you would say them: `90`, `1:30` or `0:01:30`. Leave the
  start empty to begin at the beginning, and the end empty to run to the end of
  the file.
  - There is a **timeline** to drag, with frames pulled from the video along it
    so you can see where you are cutting. Drag either end; the part you are
    keeping stays bright and the rest dims. The times underneath follow the
    handles, and typing in them moves the handles, so the picture and the
    numbers always agree.
  - By default nothing is re-encoded, so a clip finishes almost instantly, at
    the cost of starting on the nearest keyframe before the time you asked for.
  - "Cut exactly where i asked" re-encodes to start on the exact frame instead.
  - The original file is never touched.

- **Keyboard shortcuts.** `ctrl + v` pastes a link and opens the save screen,
  `ctrl + enter` downloads what is in the box, and `1`, `2` and `3` switch
  between auto, audio and mute. On macOS that is `cmd`. The number keys stay
  out of the way while you are typing in a box, and `ctrl + v` inside the link
  box still means an ordinary paste. They are listed on the about screen,
  since a shortcut nobody knows about is not a feature.

- **The queue survives a restart.** Downloads that had not finished are
  remembered, so closing Snag mid-download no longer throws the work away.
  They come back marked interrupted rather than running: reopening Snag is not
  the same as asking it to download, so there is a "resume all" button and a
  retry on each one. Resuming carries on from what is already on disk instead
  of starting the file again. Finished, failed and cancelled downloads are not
  remembered, since those were answered already.

- **Split into chapters.** A video with chapters can be written out as one
  file per chapter alongside the whole thing, under settings > metadata. A
  video without chapters downloads exactly as before. Needs ffmpeg.

- **Fixed: the preferred dub language never worked.** Asking for a dubbed
  audio track produced a yt-dlp argument that translates titles and
  descriptions and leaves the audio alone, so the setting has done nothing
  since it was added. Dubs are now chosen by filtering the audio streams on
  their language, and a video that does not carry the language you asked for
  still downloads as usual.
  - Subtitles are chosen separately from the audio track, and both screens now
    say so. A Spanish dub does not imply Spanish subtitles: pick those under
    settings > metadata, where `all` takes every track the site offers.

- **Subtitles as files.** Subtitles can now be saved next to the media file
  rather than only embedded in it, as `.srt` since that is what every player
  takes. It works in audio mode too, where embedding does not.
  - Both this and the existing embed option now have an **include automatic
    captions** switch, on by default. Without it a site's machine transcript
    does not count as a subtitle, and since most videos have no hand written
    ones, embedding subtitles has until now quietly produced nothing on them.

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
