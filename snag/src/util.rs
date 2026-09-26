use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Build a Command that never flashes a console window on Windows.
pub fn command<S: AsRef<std::ffi::OsStr>>(program: S) -> Command {
    // Only Windows mutates it, to set the no-console creation flag.
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

/// Run a command and return trimmed stdout, or an error string.
pub fn run_capture(program: &str, args: &[&str]) -> Result<String, String> {
    let out = command(program)
        .args(args)
        .output()
        .map_err(|e| format!("could not run {program}: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

pub fn human_bytes(b: f64) -> String {
    const U: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = b.max(0.0);
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{v:.0} {}", U[i])
    } else {
        format!("{v:.1} {}", U[i])
    }
}

pub fn human_speed(bytes_per_sec: f64) -> String {
    format!("{}/s", human_bytes(bytes_per_sec))
}

pub fn human_eta(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "--:--".into();
    }
    let s = secs.round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub fn looks_like_url(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty() && (s.starts_with("http://") || s.starts_with("https://"))
}

/// Reveal a file or folder in the system file manager.
pub fn reveal(path: &Path) {
    #[cfg(windows)]
    {
        // Explorer wants the quotes around the path only, not around the
        // whole argument. Rust quotes any argument containing a space,
        // which turns the select argument into one quoted lump, and
        // Explorer answers that by opening Documents. Hence raw_arg, which
        // passes the command line through exactly as written.
        use std::os::windows::process::CommandExt;

        // A path yt-dlp reported can come back with forward slashes, which
        // Explorer will not take either.
        let text = path.display().to_string().replace('/', "\\");
        if path.is_dir() {
            let _ = command("explorer").raw_arg(format!("\"{text}\"")).spawn();
        } else if path.is_file() {
            let _ = command("explorer")
                .raw_arg(format!("/select,\"{text}\""))
                .spawn();
        } else if let Some(parent) = path.parent().filter(|p| p.is_dir()) {
            // Moved, renamed or deleted since: its folder is the next best
            // thing, and better than Explorer's idea of a default.
            let parent = parent.display().to_string().replace('/', "\\");
            let _ = command("explorer").raw_arg(format!("\"{parent}\"")).spawn();
        }
    }
    #[cfg(target_os = "macos")]
    {
        let mut c = command("open");
        if path.is_dir() {
            c.arg(path);
        } else {
            c.arg("-R").arg(path);
        }
        let _ = c.spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Ask the file manager to open the folder with the file selected,
        // the way Explorer's /select does. Nautilus, Dolphin, Nemo, Caja and
        // Thunar all answer this; when nothing does, open the folder instead.
        // On a thread, because a file manager being started to answer can
        // take a second, and the window should not wait on it.
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            if path.is_file() {
                let shown = command("dbus-send")
                    .args([
                        "--session",
                        "--print-reply",
                        "--reply-timeout=3000",
                        "--dest=org.freedesktop.FileManager1",
                        "/org/freedesktop/FileManager1",
                        "org.freedesktop.FileManager1.ShowItems",
                    ])
                    .arg(format!("array:string:{}", file_uri(&path)))
                    .arg("string:")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .is_ok_and(|s| s.success());
                if shown {
                    return;
                }
            }
            let target = if path.is_dir() {
                path.clone()
            } else {
                path.parent().unwrap_or(&path).to_path_buf()
            };
            let _ = command("xdg-open").arg(target).spawn();
        });
    }
}

/// A `file://` URI for `path`, with everything but the unreserved characters
/// percent-encoded, as D-Bus file manager calls expect.
#[cfg(all(unix, not(target_os = "macos")))]
fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            uri.push(b as char);
        } else {
            uri.push_str(&format!("%{b:02X}"));
        }
    }
    uri
}

pub fn open_path(path: &Path) {
    #[cfg(windows)]
    let _ = command("cmd").args(["/C", "start", ""]).arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = command("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = command("xdg-open").arg(path).spawn();
}

pub fn default_download_dir() -> PathBuf {
    // A snap runs with $HOME pointed at its own private folder, so "the
    // home folder's Downloads" would be one nobody ever looks in. snapd
    // says where the real home is.
    if let Some(real_home) = std::env::var_os("SNAP_REAL_HOME") {
        return PathBuf::from(real_home).join("Downloads");
    }
    let Some(dirs) = directories::UserDirs::new() else {
        return PathBuf::from(".");
    };
    // Linux only has a downloads folder when xdg-user-dirs has been run and
    // written one down, which a minimal install, a server image or a fresh
    // WSL never does. "." was the fallback, which is wherever Snag happened
    // to be started from: often / from a launcher. ~/Downloads is where
    // everyone would look, and yt-dlp creates it if it is not there yet.
    dirs.download_dir()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| dirs.home_dir().join("Downloads"))
}

pub fn clipboard_text() -> Option<String> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut c| c.get_text().ok())
}

pub fn set_clipboard_text(s: &str) {
    if let Ok(mut c) = arboard::Clipboard::new() {
        let _ = c.set_text(s.to_string());
    }
}

/// Compare two yt-dlp style versions like `2025.09.05` or `2025.09.05.232734`.
pub fn version_is_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim()
            .trim_start_matches('v')
            .split(['.', '-'])
            .map(|p| p.trim().parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (a, b) = (parse(latest), parse(current));
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

#[cfg(all(test, unix, not(target_os = "macos")))]
mod tests {
    #[test]
    fn file_uris_encode_spaces_and_non_ascii() {
        let uri = super::file_uri(std::path::Path::new("/home/a b/Me at the zoo (1).mkv"));
        assert_eq!(uri, "file:///home/a%20b/Me%20at%20the%20zoo%20%281%29.mkv");
        let uri = super::file_uri(std::path::Path::new("/tmp/çay.mp3"));
        assert_eq!(uri, "file:///tmp/%C3%A7ay.mp3");
    }
}
