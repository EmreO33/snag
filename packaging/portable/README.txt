Snag (portable)
===============

Run snag.exe. There is nothing to install.

This is the portable build: everything Snag stores lives in the "data" folder
next to the executable, and nothing else is written to the machine, apart from
the one Start Menu shortcut Windows needs before it will show Snag's
notifications.
See portable.txt for the details, and for how to turn that off.

First run asks you three things: where downloads go, and whether to fetch
yt-dlp. Snag does not ship yt-dlp, so it offers to download the current release
from the yt-dlp project's own GitHub releases. ffmpeg is not installed by Snag
and is needed for merging and conversion: get it from https://ffmpeg.org.

Source, releases and bug reports: https://github.com/EmreO33/snag
