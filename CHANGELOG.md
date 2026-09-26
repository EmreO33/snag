# Changelog

Each release's notes are taken from here automatically, so this file is the
one place a change gets written down. Every release gets an entry, and CI
fails a version bump that arrives without one.

A heading may also carry a name after the version, and that name becomes the
release title on GitHub. Those belong on feature releases, the x.y.0 ones, and
never on a patch. They are optional even then.

## 1.5.9

- **Background mode on Linux.** Snag gets a tray icon on Linux too, with
  the same menu as on Windows: close the window and Snag keeps going in the
  tray, left click the icon to bring it back, right click for "Download
  what I copied" and Quit. "Start in the tray" and "start when you log in"
  work as well, the second through your desktop's startup apps (for the
  Flatpak, the desktop is asked, and says whether it allows it).
- It needs a desktop with a tray: KDE, Cinnamon, XFCE, Budgie and Ubuntu's
  GNOME have one, and plain GNOME gets one from the AppIndicator extension.
  Without one, Snag says so and keeps its window. Started at login before
  the panel is up, it waits for the tray rather than giving up.
- A window cannot hide on Wayland and keep running, so with background mode
  on, Snag runs through XWayland, from the next time it starts.
- Starting in the tray no longer shows the window for a moment first: it
  starts minimised, out of the taskbar, until you open it.
- **Quit in the tray menu no longer turns background mode off**, on Windows
  as well. Quit switched the setting off to get past closing to the tray,
  and that was saved on the way out, so the next launch had background mode
  off.

## 1.5.8

- A machine-wide install no longer gets a second "Snag" in the Start menu.
  Snag keeps a Start menu shortcut because Windows will not show its
  notifications without one, but it only looked in the user's own Start
  menu, missed the installer's in the all-users one, and made another. The
  installer's now counts, and the extra one is removed.

## 1.5.7

- **The tray menu works on the first click.** With Snag sitting in the
  tray, "Show Snag", "Quit" and "Download what I copied" only took effect
  on the second click. The click was received, but the wake-up it sent was
  lost inside the menu, so Snag slept on until something else happened.
  This was there since the tray arrived; earlier tests sent the click in a
  way that woke Snag by accident. Found and checked with real clicks.
- The same wake-up now reaches Snag whenever it is minimised, so a
  download that finishes in the background starts the next one and says
  so straight away, rather than when the window is next touched.
- **Closing to the tray just closes**, the way Discord does: no minimise
  animation, the window is simply gone. It comes back in place too.
  Recorded at 40 frames a second, 1.5.6 shrank away over 170ms; this is
  gone from one frame to the next.
- A left click on the tray icon opens Snag. The menu is on the right
  button; before, a left click opened the menu too.

## 1.5.6

- **The snap's ffmpeg did not start**, so every snap download that needed
  merging or converting failed, and setup said ffmpeg was missing. It
  needed two libraries that Ubuntu reaches through links a snap does not
  get. The snap points at them directly now. Found by installing 1.5.5
  from the Snap Store and using it.
- The snap keeps its settings in the folder snapd keeps across updates.
  They were in the folder for one revision, which snapd deletes a few
  updates later, taking the path to Snag's own yt-dlp with it.
- **Remuxing a file picked from outside Downloads in the Flatpak** could
  not write its result: the picker shares only the file that was picked,
  not the folder it is in. The result goes to the download folder instead.
- In the Flatpak, a download folder that is not shared with the sandbox
  now says so on the save screen, and a download there fails at once with
  the reason. It used to finish, report done, and vanish when Snag closed.
  That happens when no Downloads folder is registered with the desktop.
- A Flatpak copy of Snag now updates from the bundle on the releases page:
  it downloads the next one and gives you the command that installs it.
- The snap is in the Snap Store: `sudo snap install snag`.

## 1.5.5

- **Linux packages.** Alongside the AppImage there is now a .deb for
  Ubuntu, Debian and Mint, an .rpm for Fedora and openSUSE, and a Flatpak.
  Each was installed and used on a real system before this went out:
  Ubuntu 26.04, Fedora 44, openSUSE Tumbleweed and Arch, with the Flatpak
  built and run on Ubuntu. A packaged copy knows it is one, and updates the
  way its package does rather than trying to overwrite a file it does not
  own.
- **The AppImage starts on a current Linux.** It needed libfuse2, which
  Ubuntu stopped installing in 22.04 and Fedora and Arch never do, so on
  most fresh systems it would not open at all. It is built with the new
  runtime now, which needs nothing extra.
- **"No such file or directory" on every download**, after installing
  yt-dlp during setup and closing Snag before finishing it. Setup showed the
  new yt-dlp as found, but only remembered where it was if both happened in
  one sitting. Snag now always uses the yt-dlp it installed. This was on
  Windows too.
- **Converting an mkv Snag downloaded to mp4 or mov always failed.** Every
  mkv carries its thumbnail as an attached file, which those containers
  cannot hold, and the whole remux failed over it. Attachments are now left
  out when the container cannot take them, and subtitles are converted to
  the kind it can. On every platform.
- A webm remux that cannot work now says why: webm only holds vp8, vp9 or
  av1 with opus or vorbis, and ffmpeg's own message named neither.
- **A yt-dlp that cannot update itself** (from apt, pacman, or inside the
  Flatpak) used to end "update now" with an error telling you to go and
  update it elsewhere. Snag now installs its own current copy instead, and
  uses that from then on.
- On Linux, "show in folder" opens the file manager with the file selected,
  as it does on Windows, rather than just opening the folder.
- On Linux, the download folder defaulted to "." wherever the system had no
  Downloads folder registered, which is most minimal installs. It is
  ~/Downloads now.
- Setup's ffmpeg card said to "look again" and had no button to do it with.
  It has one now.
- The error under "snag" on the updates screen could run underneath the
  check and update buttons. It wraps now, like the others.
- Update checks wait 30 seconds for GitHub rather than 15, which a slow
  connection or a VPN could take just to connect.
- Fedora's ffmpeg is `ffmpeg-free`; the command setup offered there asked
  for a package that only exists once RPM Fusion is added.
- `snag --version` says which version this is and how it was installed.

## 1.5.4

- **Pick the format before downloading.** A new format row under the mode
  buttons sets the file type for the next download only, so an mp3 today
  does not mean changing what every download is from now on. It shows what
  the settings would have given ("from settings (mp4)") so there is no
  going to look. Asked for by a user who was downloading mp4s and converting
  them afterwards.
- **More formats**, both there and in settings: mov for video, and m4a and
  flac for audio. Every format was downloaded for real and checked with
  ffprobe before this went out: mp4, webm, mkv and mov, plus mov and webm
  with no sound, and best, mp3, m4a, ogg, opus, flac and wav.
- **webm and wav downloads no longer fail at the very end.** Embedding the
  thumbnail is on by default, and yt-dlp cannot put a picture in either of
  those, so every webm or wav download finished downloading and then
  failed. The thumbnail is now left out for those two and nothing else.
- **webm with h264 selected no longer fails either.** webm cannot hold h264,
  and mov cannot hold vp9, av1 or opus, so picking one of those containers
  now downloads a codec it can hold instead of downloading the whole video
  and then failing to remux it.
- **Search in history.** Type any words from a title, link or file name, in
  any order, and the list narrows to what matches. Escape clears it.
- The queue says which format a download was told to take, when it is not
  the one from the settings.

## 1.5.3

- **Starting in the tray now starts in the tray.** Snag opened its window and
  then dropped it to the tray on the first frame, which is a flash of a
  window nobody asked for. The window is now created out of sight and parked
  before it can be drawn: measured at 80ms to the tray, against 225ms and a
  30ms flash before. It cannot simply be created hidden, as a hidden window
  gets no redraws and Snag runs from redraws, so it would sit there starting
  no downloads and answering no tray clicks.
- If the tray icon cannot be made at all, a Snag that was told to start there
  shows its window rather than becoming a process with no way to reach it.

## 1.5.2

- **The tray menu works.** "Show Snag" and "download what I copied" did
  nothing whenever Snag had been sitting still, which is most of the time
  a tray icon exists for. A click on a tray menu sends nothing the window's
  event loop is listening for, so the click sat in a queue that only gets
  read when something else wakes the app: a download running, or a window
  being dragged. The tray now wakes Snag itself. Reproduced on 1.5.1 and
  checked again after fixing, both for the menu and for a click on the icon.
- A single left click on the tray icon opens Snag too, rather than only a
  double click.
- "Download what I copied" now says what it did, since the window it would
  normally say it in is not on screen: a notification for the link it
  queued, for one already in the queue, and for a clipboard with no link
  in it.
- Snag could also lose track of its own window while that window was
  minimised, which left "show snag" with nothing to show. It looks for the
  window by the size it would be when restored now, not the 237x39 strip
  Windows parks a minimised window in.

## 1.5.1

Four things users reported, all fixed.

- **Updating no longer runs the installer wizard.** An installed copy now
  updates quietly: Snag closes, the new version installs itself, and Snag
  comes back. A machine-wide install (one in Program Files) asks for
  permission once, because Windows will not let it be replaced otherwise,
  and if anything refuses, the installer is shown rather than leaving you
  with a closed Snag. Note that the update *to* this version is still done
  by the old code, so there is one more wizard before there are none.
- **ffmpeg stays found after an update.** It was only ever looked for on
  PATH, and PATH is inherited: a Snag started by the installer carried the
  environment of the Snag that started it, which predates the winget
  install that put ffmpeg there. Snag now looks in the places ffmpeg is
  actually put, including winget's package folder and its own settings
  folder, and remembers the path rather than trusting PATH to have it. yt-dlp
  is told that path too, so merging works even when nothing else can find it.
- **"show in folder" opens the folder again.** It opened Documents instead,
  for any file whose path contained a space: the whole argument was being
  quoted where only the path should have been, and Explorer answers a
  command line it cannot parse by opening your Documents folder.
- **Closing to the tray now closes the window** instead of minimising it to
  the taskbar. The window and its taskbar button both go; the app keeps
  running and the tray icon brings it back where it was. (It is minimised
  rather than hidden behind the scenes, because a hidden window gets no
  redraws and a Snag that cannot redraw stops pumping its queue.)
- The label in front of a row of buttons ("quality", "take", "preset") now
  sits the same distance from the first button as the buttons sit from each
  other.

## 1.5.0

- **Presets.** A name for the mode, quality and extras you use often:
  "music", "phone", "archive". They sit under the mode buttons on the save
  screen, one click sets them, and settings > presets is where you rename,
  reorder, update and delete them. Picking one sets the actual settings, so
  what the settings screens say is always what the next download will be.
  - A download keeps the preset it was queued with. Queue five things as
    music, switch to archive, and the ones still waiting are still music.
    That also fixes a quieter old bug: changing any setting used to reach
    back into jobs that had not started yet.
  - `snag --preset="music" <link> --download` for a shortcut or a script.
- **Snag can live in the tray.** It can start with Windows and go straight
  there, the tray menu has "download what I copied" for when you do not want
  a window at all, and an opt-in mode downloads every link you copy the
  moment you copy it. All of it off by default, under settings > background.
- **The interface moves.** Hovers fade, buttons sink when pressed, the mark
  in the nav rail slides to the screen you picked, screens arrive instead of
  appearing, the progress bar catches up smoothly instead of jumping, and
  messages slide in. Nothing takes longer than a fifth of a second. Off in
  settings > appearance, where off means instant.
- Fixed: the animations setting, and anything else under appearance, was
  only applied when it changed rather than at launch.

## 1.4.1

- The ffmpeg card on the updates screen says "ffmpeg 9.0.1" rather than
  quoting ffmpeg's whole two-line banner, copyright notice and all.
- Its "check" button could not be clicked: the long text took the whole
  width of the card, and the buttons were laid out in the nothing left over,
  where clicks do not land. The text now stops short of the buttons, on
  both this card and the yt-dlp one.

## 1.4.0 - The Movie Update

- **Pick from a playlist.** Paste a playlist and, next to "just this one"
  and "all", there is now "pick": the items are listed with their titles
  and lengths, you tick the ones you want, and each becomes its own download
  in the queue, separately cancellable, retried and remembered. "all" and
  "none" buttons for the long ones. A site that only counts its items
  without naming them offers the first two choices as before.
- The save page scrolls now, so a long list of picks or a long recent list
  never pushes the download button off the bottom of a small window.

## 1.3.8

- **Notifications on Windows, for real this time.** 1.3.7 registered Snag
  the way the documentation describes, with a registry key, and this
  Windows 11 ignores that: a toast sent under that id is filed in the
  notification centre and never drawn. What Windows does honour is a Start
  Menu shortcut carrying the app id, which is what Discord, Chrome and every
  Electron app keep for themselves. So now the installer's shortcuts carry
  it, a Scoop copy stamps the shortcut Scoop made, and a portable copy keeps
  one shortcut of its own in your Start Menu (removed when notifications are
  turned off). Verified by eye this time, not by asking the database.
- `snag --notify-test` waits a moment before and after sending, because a
  shortcut made just now takes the shell a beat to notice, and a toast whose
  sender has already exited is dropped. Both were hiding the real result.

## 1.3.7

- **Notifications on Windows work now.** They never did: Windows only shows a
  notification from an app it has been introduced to, and Snag never
  introduced itself, so every one it sent was accepted and quietly filed
  away. It now registers its app id at startup the way other unpackaged apps
  do (one key under `HKCU\Software\Classes\AppUserModelId`), the installer
  stamps the same id on its shortcuts, and uninstalling or turning
  notifications off removes the key.
- **A download that finishes while you are elsewhere tells you.** Done or
  failed, if Snag is hidden, minimised or behind another window, a desktop
  notification says which one and what it was. In front, the message in the
  corner was already enough, so nothing is said twice. On by default and off
  in settings > background.
- The copied-link notification now respects the same switch, and stays
  silent; a finished download plays the desktop's default sound.
- `snag --notify-test` registers first and sends the finished-download
  notification, so it is a real test of what a user would see.

## 1.3.6

- **Deno comes with yt-dlp.** yt-dlp now wants a JavaScript runtime to handle
  YouTube's player, and has deprecated working without one: every YouTube
  download has been printing a warning about it, and the fallback it uses
  instead will be removed at some point. Rather than make that a second thing
  to know about, Snag fetches Deno from its own GitHub releases whenever it
  installs or updates yt-dlp, keeps it beside yt-dlp, and points yt-dlp at it.
  Nothing to do, and the warning is gone.
  - A yt-dlp you installed yourself is left alone. Give it a Deno on `PATH`
    and yt-dlp finds it by itself.
  - The updates screen says whether Deno is there.

## 1.3.5

- **Hardware video encoding**, under settings > local processing. A clip cut
  exactly is the one time Snag re-encodes video, and it can now do that on an
  NVIDIA, Intel or AMD GPU instead of the CPU. Only the encoders your ffmpeg
  was built with are offered. Software stays the default: it works everywhere
  and gives the smallest file for the quality, while the hardware encoders are
  much faster on most machines and produce files two to three times larger.
  Measured before shipping, on real 1080p60 footage: a two minute cut took 13
  seconds on NVENC against 19 on x264 with 32 CPU threads, and most machines
  have far fewer threads and the same NVENC.
  - A hardware encoder the build carries but the machine cannot run fails
    with a message that says so and points back at the setting.

## 1.3.4

- **ffmpeg has a place on the updates screen**, beside Snag and yt-dlp, with
  its version, or "not found" and an install button when it is missing. On
  Linux and macOS, where installing needs root, the button copies the package
  manager command instead. A user asked for this: the installer existed but
  only appeared during first-run setup, so anyone who skipped it then had no
  way back to it.
- **Snag says when ffmpeg is missing before you find out the hard way.** A
  line on the save screen, a note on the remux screen with the run button
  disabled, and a red "ffmpeg: not found" in the status bar that goes to the
  installer when clicked. Previously the first sign was a failed download.
- ffmpeg is now looked for at every launch and whenever its path is changed
  in settings, not only during setup.

## 1.3.3

- **Fixed: Snag would not start on a clean Windows.** Every Windows build so
  far needed the Visual C++ runtime, `VCRUNTIME140.dll`, which a fresh Windows
  does not have. Without it Snag exited with `STATUS_DLL_NOT_FOUND` before
  drawing a window. Most machines have the runtime from other software, which
  is why it went unnoticed until the winget validator ran Snag on a machine
  that did not. The runtime is now built into the binary, at a cost of about
  150 KB, and the release checks the finished file for the dependency so it
  cannot come back.

## 1.3.2

- **A failed download says why, where you are standing.** The recent list on
  the save screen showed the word "failed" and nothing else, so finding out
  what went wrong meant knowing to go and look in the queue. It now carries
  the reason underneath the title.
- **A "copy details" button** on a failed job in the queue. It copies the
  error together with the Snag and yt-dlp versions, the operating system, the
  mode and the link, so a bug report arrives complete rather than as a
  photograph of the word "failed".

## 1.3.1

- **Fixed: Snag was invisible to screen readers.** Two things were wrong. The accessibility integration had been
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
