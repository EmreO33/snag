//! Updating Snag itself.
//!
//! How an update should be applied depends entirely on how Snag was installed.
//! A copy managed by Scoop must be left alone and updated through Scoop, or the
//! two fight over the same files. An installed copy is best handed back to its
//! own installer. Only a portable or loose binary is ours to replace directly.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

const REPO: &str = "EmreO33/snag";
const LATEST_RELEASE_API: &str = "https://api.github.com/repos/EmreO33/snag/releases/latest";

/// How this copy of Snag got onto the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallKind {
    /// Managed by Scoop: hands off, tell the user the command.
    Scoop,
    /// Put here by the Inno installer, which is what should upgrade it.
    Installed,
    /// A portable copy or a loose binary: safe to replace in place.
    Portable,
}

impl InstallKind {
    pub fn label(&self) -> &'static str {
        match self {
            InstallKind::Scoop => "installed through scoop",
            InstallKind::Installed => "installed with the windows installer",
            InstallKind::Portable => "portable or standalone binary",
        }
    }

    /// How an update will be applied for this kind of install.
    pub fn update_note(&self) -> &'static str {
        match self {
            InstallKind::Scoop => "this copy is managed by scoop, so it updates with 'scoop update snag' rather than replacing itself.",
            InstallKind::Installed => "this copy was installed with the windows installer, so an update downloads the new installer and runs it.",
            InstallKind::Portable => "this copy replaces its own binary in place. the previous one is kept alongside until the next launch.",
        }
    }
}

pub fn detect_install_kind() -> InstallKind {
    let exe = std::env::current_exe().unwrap_or_default();
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();

    // Scoop lays apps out as <root>\apps\<name>\<version>\, so match on that
    // rather than on the user's chosen scoop root.
    let lowered = exe.to_string_lossy().to_lowercase().replace('/', "\\");
    if lowered.contains("\\scoop\\apps\\") {
        return InstallKind::Scoop;
    }

    // The Inno installer leaves its uninstaller beside the executable.
    if dir.join("unins000.exe").is_file() {
        return InstallKind::Installed;
    }

    InstallKind::Portable
}

/// The release asset that matches this platform's bare binary.
pub fn binary_asset_name() -> &'static str {
    if cfg!(windows) {
        "snag-windows-x86_64.exe"
    } else if cfg!(target_os = "macos") {
        "snag-macos-aarch64"
    } else {
        "snag-linux-x86_64"
    }
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn releases_url() -> String {
    format!("https://github.com/{REPO}/releases/latest")
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum SelfUpdateState {
    #[default]
    Unknown,
    Checking,
    UpToDate,
    Available {
        latest: String,
    },
    Downloading {
        got: u64,
        total: u64,
    },
    /// The new binary is in place; Snag has to be restarted to run it.
    RestartRequired,
    /// The installer was handed the update and Snag is getting out of its way.
    HandedOff,
    Error(String),
}

impl SelfUpdateState {
    pub fn busy(&self) -> bool {
        matches!(
            self,
            SelfUpdateState::Checking | SelfUpdateState::Downloading { .. }
        )
    }
    pub fn fraction(&self) -> Option<f32> {
        match self {
            SelfUpdateState::Downloading { got, total } if *total > 0 => {
                Some((*got as f32 / *total as f32).clamp(0.0, 1.0))
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum SelfUpdateEvent {
    State(SelfUpdateState),
    Log(String),
}

/// Ask GitHub for the newest release tag.
fn latest_version() -> Result<String, String> {
    let resp = ureq::get(LATEST_RELEASE_API)
        .set("User-Agent", "Snag")
        .set("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(15))
        .call()
        .map_err(|e| format!("could not reach github: {e}"))?;

    let json: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("unexpected response from github: {e}"))?;

    json.get("tag_name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim_start_matches('v').to_string())
        .ok_or_else(|| "github response had no release tag".to_string())
}

fn asset_url(version: &str, asset: &str) -> String {
    format!("https://github.com/{REPO}/releases/download/v{version}/{asset}")
}

/// Fetch the release's checksum file, if it published one.
fn published_hash(version: &str, asset: &str) -> Option<String> {
    let resp = ureq::get(&asset_url(version, "SHA256SUMS.txt"))
        .set("User-Agent", "Snag")
        .timeout(std::time::Duration::from_secs(15))
        .call()
        .ok()?;
    let body = resp.into_string().ok()?;
    body.lines()
        .find(|l| l.trim_end().ends_with(asset))
        .and_then(|l| l.split_whitespace().next())
        .map(|h| h.to_lowercase())
}

fn sha256_hex(bytes: &[u8]) -> String {
    // A small, self-contained SHA-256 so updating does not pull in a crypto
    // dependency for one hash.
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut msg = bytes.to_vec();
    let bit_len = (bytes.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, word) in chunk.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(v);
        }
    }

    h.iter().map(|w| format!("{w:08x}")).collect()
}

/// Download a release asset into memory, reporting progress as it goes.
fn download(url: &str, tx: &Sender<SelfUpdateEvent>) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url)
        .set("User-Agent", "Snag")
        .timeout(std::time::Duration::from_secs(120))
        .call()
        .map_err(|e| format!("could not download the update: {e}"))?;

    let total: u64 = resp
        .header("Content-Length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let mut reader = resp.into_reader();
    let mut out: Vec<u8> = Vec::with_capacity(total as usize);
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("download interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        let _ = tx.send(SelfUpdateEvent::State(SelfUpdateState::Downloading {
            got: out.len() as u64,
            total,
        }));
    }

    if out.is_empty() {
        return Err("github returned an empty file".into());
    }
    Ok(out)
}

/// The path a replaced binary is parked at until the next launch.
pub fn stale_binary_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(PathBuf::from(format!("{}.old", exe.display())))
}

/// Remove the previous binary left behind by an earlier self-update.
pub fn clean_stale_binary() {
    if let Some(old) = stale_binary_path() {
        if old.exists() {
            let _ = std::fs::remove_file(old);
        }
    }
}

/// Swap the running executable for `bytes`, keeping the old one until restart.
fn replace_self(bytes: &[u8]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate snag: {e}"))?;
    replace_binary(&exe, bytes)
}

/// The file dance behind a self-update, split out from `replace_self` so it can
/// be exercised against an ordinary file rather than the running process.
fn replace_binary(exe: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = exe
        .parent()
        .ok_or_else(|| "snag is not in a directory".to_string())?;

    // Stage in the same directory so the final move is a rename, not a copy
    // across volumes.
    let staged = dir.join("snag-update.part");
    {
        let mut f = std::fs::File::create(&staged)
            .map_err(|e| format!("could not write next to snag: {e}"))?;
        f.write_all(bytes)
            .map_err(|e| format!("could not write the update: {e}"))?;
        f.flush().map_err(|e| e.to_string())?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755));
    }

    // A running executable cannot be deleted on Windows, but it can be renamed,
    // which is what makes replacing ourselves possible at all.
    let old = PathBuf::from(format!("{}.old", exe.display()));
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old)
        .map_err(|e| format!("could not move the running snag aside: {e}"))?;

    if let Err(e) = std::fs::rename(&staged, exe) {
        // Put the working binary back rather than leaving nothing behind.
        let _ = std::fs::rename(&old, exe);
        let _ = std::fs::remove_file(&staged);
        return Err(format!("could not put the new snag in place: {e}"));
    }

    Ok(())
}

/// Check GitHub for a newer Snag.
pub fn check(tx: Sender<SelfUpdateEvent>, repaint: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        let _ = tx.send(SelfUpdateEvent::State(SelfUpdateState::Checking));
        repaint();

        let state = match latest_version() {
            Ok(latest) => {
                if crate::util::version_is_newer(&latest, current_version()) {
                    SelfUpdateState::Available { latest }
                } else {
                    SelfUpdateState::UpToDate
                }
            }
            Err(e) => SelfUpdateState::Error(e),
        };
        let _ = tx.send(SelfUpdateEvent::State(state));
        repaint();
    });
}

/// Download and apply an update, in whichever way suits how Snag was installed.
pub fn install(
    version: String,
    kind: InstallKind,
    tx: Sender<SelfUpdateEvent>,
    repaint: impl Fn() + Send + 'static,
) {
    std::thread::spawn(move || {
        let asset = match kind {
            InstallKind::Installed => format!("Snag-{version}-windows-setup.exe"),
            _ => binary_asset_name().to_string(),
        };
        let url = asset_url(&version, &asset);
        let _ = tx.send(SelfUpdateEvent::Log(format!("downloading {asset}")));

        let bytes = match download(&url, &tx) {
            Ok(b) => b,
            Err(e) => {
                let _ = tx.send(SelfUpdateEvent::State(SelfUpdateState::Error(e)));
                repaint();
                return;
            }
        };

        // Verify against the release's own checksum file when it has one.
        match published_hash(&version, &asset) {
            Some(expected) => {
                let actual = sha256_hex(&bytes);
                if actual != expected {
                    let _ = tx.send(SelfUpdateEvent::State(SelfUpdateState::Error(
                        "the download did not match the published checksum, so it was discarded"
                            .into(),
                    )));
                    repaint();
                    return;
                }
                let _ = tx.send(SelfUpdateEvent::Log("checksum verified".into()));
            }
            None => {
                let _ = tx.send(SelfUpdateEvent::Log(
                    "this release published no checksums, so the download could not be verified"
                        .into(),
                ));
            }
        }

        let state = match kind {
            InstallKind::Installed => {
                // Hand the update to the installer and step aside.
                let tmp = std::env::temp_dir().join(&asset);
                match std::fs::write(&tmp, &bytes) {
                    Ok(()) => match crate::util::command(&tmp).spawn() {
                        Ok(_) => SelfUpdateState::HandedOff,
                        Err(e) => {
                            SelfUpdateState::Error(format!("could not start the installer: {e}"))
                        }
                    },
                    Err(e) => SelfUpdateState::Error(format!("could not save the installer: {e}")),
                }
            }
            _ => match replace_self(&bytes) {
                Ok(()) => SelfUpdateState::RestartRequired,
                Err(e) => SelfUpdateState::Error(e),
            },
        };

        let _ = tx.send(SelfUpdateEvent::State(state));
        repaint();
    });
}

#[cfg(test)]
mod tests {
    use super::{replace_binary, sha256_hex};

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // Long enough to span multiple blocks and exercise the padding.
        assert_eq!(
            sha256_hex(&vec![b'a'; 1000]),
            "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"
        );
    }

    /// A temporary directory that cleans up after itself.
    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("snag-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn replacing_a_binary_keeps_the_old_one_alongside() {
        let dir = temp_dir("replace");
        let target = dir.join("snag.exe");
        std::fs::write(&target, b"the old binary").unwrap();

        replace_binary(&target, b"the new binary").unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"the new binary");
        let old = std::path::PathBuf::from(format!("{}.old", target.display()));
        assert_eq!(
            std::fs::read(&old).unwrap(),
            b"the old binary",
            "the previous binary should be parked alongside, not discarded"
        );
        // No half-written staging file should survive a successful swap.
        assert!(!dir.join("snag-update.part").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn replacing_twice_does_not_trip_over_the_previous_backup() {
        let dir = temp_dir("replace-twice");
        let target = dir.join("snag.exe");
        std::fs::write(&target, b"v1").unwrap();

        replace_binary(&target, b"v2").unwrap();
        replace_binary(&target, b"v3").unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"v3");
        let old = std::path::PathBuf::from(format!("{}.old", target.display()));
        assert_eq!(std::fs::read(&old).unwrap(), b"v2");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
