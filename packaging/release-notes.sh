#!/usr/bin/env bash
#
# Builds what a GitHub release page says, out of CHANGELOG.md.
#
# It lives here as a script rather than inline in the workflow so it can be
# run and read locally, which is the only way to find out what a release will
# look like without cutting one.
#
#   release-notes.sh <tag> title   the release's name
#   release-notes.sh <tag> body    the release's body, as markdown
#   release-notes.sh <tag> check   fails if the changelog has nothing to say
#
# Every release gets a changelog entry, and `check` is what makes that true
# rather than merely intended: CI runs it against the version in Cargo.toml,
# so a bump without a note fails long before anything is tagged.
#
# A name after the version in the heading is optional and belongs only on a
# feature release, an x.y.0. Patch releases carry the bare version, so the
# names stay rare enough to be worth reading. A changelog entry is not
# optional anywhere.

set -euo pipefail

TAG="${1:?usage: release-notes.sh <tag> title|body}"
WHAT="${2:?usage: release-notes.sh <tag> title|body}"
VERSION="${TAG#v}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHANGELOG="$ROOT/CHANGELOG.md"

# Dots are regex in a pattern and there are three of them in every version, so
# they are escaped rather than left to match anything.
ESCAPED="${VERSION//./\\.}"
HEADING_RE="^## ${ESCAPED}( |$)"

heading() {
    [ -f "$CHANGELOG" ] || return 0
    grep -m1 -E "$HEADING_RE" "$CHANGELOG" || true
}

# Everything under this version's heading, up to the next one.
#
# Matched by comparison rather than by regex: awk rewrites the escapes in a
# regex passed through -v, which both warns and quietly turns the escaped dots
# back into "any character".
entries() {
    [ -f "$CHANGELOG" ] || return 0
    awk -v h="## $VERSION" '
        $0 == h || index($0, h " ") == 1 { found = 1; next }
        found && /^## / { exit }
        found { print }
    ' "$CHANGELOG"
}

# The part of the heading after the version, if a name was given at all.
name_of_heading() {
    heading | sed -E "s/^## ${ESCAPED} *-? *//"
}

case "$WHAT" in
title)
    # "## 1.2.2 - The Narcissistic Update" gives "v1.2.2: The Narcissistic
    # Update". A heading with no name after it just gives the tag.
    name="$(name_of_heading)"
    if [ -n "$name" ]; then
        echo "$TAG: $name"
    else
        echo "$TAG"
    fi
    ;;

body)
    notes="$(entries | sed -e '/./,$!d' | tac | sed -e '/./,$!d' | tac)"
    if [ -n "$notes" ]; then
        echo "## What changed"
        echo
        echo "$notes"
        echo
        echo "---"
        echo
    fi

    cat <<'GUIDE'
## Windows

- **`Snag-*-windows-setup.exe`**: the installer. Installs for you or for all
  users, adds a Start Menu entry, and uninstalls cleanly.
- **`Snag-*-windows-portable.zip`**: portable. Unzip and run. It keeps
  everything in a `data` folder beside the executable and writes nothing else
  to the machine.
- **`snag-windows-x86_64.exe`**: the bare executable.

## Linux

- **`Snag-*-x86_64.AppImage`**: the recommended Linux build. Runs on any distro
  with glibc 2.35 or newer and integrates with your application menu. `chmod
  +x` it and run it.
- **`snag-linux-x86_64`**: the bare binary. Needs glibc 2.39 or newer.

## macOS

`snag-macos-aarch64`, a bare binary. Builds in CI but has not been run by
anyone yet, so treat it as untested.

---

Snag does not ship yt-dlp. On first launch it offers to download the current
yt-dlp release from the yt-dlp project's own GitHub releases. ffmpeg is
required separately and is not installed by Snag.
GUIDE
    ;;

check)
    # A name on a patch release is a mistake rather than a preference, so it
    # fails here rather than quietly publishing as "v1.2.2: Some Joke".
    case "$VERSION" in
    *.*.0)
        ;;
    *)
        if [ -n "$(name_of_heading)" ]; then
            echo "$VERSION is a patch release and should not carry a name." >&2
            echo "names belong on feature releases (x.y.0) only." >&2
            echo "change the heading to just '## $VERSION'." >&2
            exit 1
        fi
        ;;
    esac

    if [ -z "$(entries | tr -d '[:space:]')" ]; then
        echo "no changelog entry for $VERSION." >&2
        echo "add a '## $VERSION' section to CHANGELOG.md saying what changed." >&2
        echo "a name after the version is optional: '## $VERSION - Some Joke'." >&2
        exit 1
    fi
    echo "changelog entry for $VERSION is present"
    ;;

*)
    echo "unknown output '$WHAT': expected title, body or check" >&2
    exit 2
    ;;
esac
