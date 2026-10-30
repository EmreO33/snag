# Snag privacy policy

Last updated: 30 October 2026. Applies to Snag 1.5.10 and later, on every platform it is offered for.

Snag is a desktop app that downloads video and audio using [yt-dlp](https://github.com/yt-dlp/yt-dlp) and [ffmpeg](https://ffmpeg.org). It is made by one developer, EmreO33, and published at <https://github.com/EmreO33/snag>.

## The short version

- Snag has no accounts, no telemetry, no analytics, no ads and no crash reporting. The developer receives nothing about you or what you download.
- What Snag keeps (your settings, your download history, your unfinished queue) stays in files on your own computer.
- Snag talks to the sites you download from, and to GitHub to update itself and its tools. Nothing else.

## What Snag stores on your computer

| What | What it contains | Kept until |
|---|---|---|
| Settings (`settings.json`) | Your choices in Snag: download folder, formats, network options, and for YouTube sign-in, *which* browser to borrow a session from or the *path* to a `cookies.txt` file. Never the cookies themselves. | You change or delete them |
| Download history (`history.json`) | For each finished download: the link, the title, the mode, where the file was saved, its size and when it finished. | The 500 most recent are kept; older ones drop off. **clear history** on the history screen deletes it all |
| Queue (`queue.json`) | Downloads that had not finished when Snag closed: the link, the title and the settings to use, so they can carry on next time. | The download finishes, fails, or you remove it |
| yt-dlp and Deno | Copies of these tools, downloaded by Snag so it can work. | You delete them or uninstall |

The job logs shown in the queue (yt-dlp's own output) are kept in memory only and are gone when Snag closes.

These files live in Snag's config folder: `%APPDATA%\Snag\config` on Windows, `~/.config/snag` on Linux (inside the sandbox's own folder for the Snap and Flatpak versions), a `data` folder beside a portable copy, or wherever you chose on first run. **settings → open config folder** shows it. They are plain files, protected by your operating system account like the rest of your files, and are not encrypted. Uninstalling Snag may leave this folder behind; delete it to remove everything Snag kept.

## Who Snag connects to, and why

- **The sites you download from.** When you paste a link, yt-dlp asks that site what the link is (title, formats), Snag fetches its thumbnail image to show you, and yt-dlp then downloads it. The site sees your IP address and whatever it normally sees from a visitor, exactly as if you visited it. If you set a proxy in Snag, these requests go through it.
- **GitHub**, to check for and download new versions of Snag (from github.com/EmreO33/snag), yt-dlp (from github.com/yt-dlp/yt-dlp) and Deno (from github.com/denoland/deno). You can turn automatic update checks off in Snag's settings. Snag's own updates are checked against the checksums published with each release before they are installed.
- **winget** (Windows only), when you ask Snag to install ffmpeg: Snag runs `winget install Gyan.FFmpeg`, which uses Microsoft's package sources as any winget install does.

Snag sends nothing to the developer or to any other party.

## YouTube sign-in and cookies

Signing in is optional and off until you set it up. Snag has no login form and never asks for or sees a password. If you choose a browser, or a `cookies.txt` file, in **settings → youtube**, yt-dlp reads the session cookies from it at the moment a download starts and sends them to the site being downloaded from, as your browser would. By default they are only sent to YouTube; a setting lets you allow other sites. **check sign-in** contacts YouTube to see whether the session works. Snag does not copy, store or transmit the cookies anywhere else. A `cookies.txt` file is as sensitive as a password: keep it somewhere private.

## Clipboard watching

Off by default. When you turn it on, Snag reads the clipboard's text about once a second to notice when you copy a link. It keeps only the last thing it saw, in memory, to avoid offering the same link twice, and never stores or sends clipboard contents. A copied link only goes anywhere if you download it, and then it is treated like any other download.

## Notifications

Desktop notifications are shown by your operating system's own notification service, on your computer. They can be turned off in **settings → background**.

## Your controls

Everything above is in your hands: change or reset any setting, clear the history, remove items from the queue, turn clipboard watching, notifications, sign-in and update checks off, or delete the config folder. Because the developer holds no data about you, there is nothing to request, export or delete from the developer's side.

## Children

Snag is a general-purpose tool and collects no information from anyone, of any age.

## Changes to this policy

Changes are made in this file, in the public repository, so every past version is visible in its history. The date at the top says when it last changed.

## Contact

Questions about privacy in Snag: open an issue at <https://github.com/EmreO33/snag/issues>.
