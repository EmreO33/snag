//! Remux: change a local file's container, strip a track, or convert it,
//! without re-encoding wherever that is possible.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::settings::Settings;
use crate::util;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RemuxOp {
    Container,
    ExtractAudio,
    StripAudio,
    ToGif,
}

impl RemuxOp {
    pub const ALL: &'static [RemuxOp] = &[
        RemuxOp::Container,
        RemuxOp::ExtractAudio,
        RemuxOp::StripAudio,
        RemuxOp::ToGif,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            RemuxOp::Container => "change container",
            RemuxOp::ExtractAudio => "extract audio",
            RemuxOp::StripAudio => "mute",
            RemuxOp::ToGif => "to gif",
        }
    }
    pub fn note(&self) -> &'static str {
        match self {
            RemuxOp::Container => "rewraps the same streams into another container. no quality loss, near-instant.",
            RemuxOp::ExtractAudio => "pulls the audio track out into its own file, copied when the container allows it.",
            RemuxOp::StripAudio => "drops the audio track and keeps the video exactly as it was.",
            RemuxOp::ToGif => "re-encodes to an animated GIF. inefficient: the file may be obnoxiously big and low quality.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum RemuxState {
    #[default]
    Idle,
    Running,
    Done(PathBuf),
    Failed(String),
    Cancelled,
}

#[derive(Debug)]
pub enum RemuxEvent {
    Progress(f32),
    Log(String),
    State(RemuxState),
}

/// Best-effort media duration in seconds, used to turn ffmpeg's progress into a bar.
fn probe_duration(ffmpeg_bin: &str, input: &Path) -> Option<f64> {
    // ffprobe usually sits next to ffmpeg; fall back to PATH.
    let probe = Path::new(ffmpeg_bin)
        .parent()
        .map(|p| {
            p.join(if cfg!(windows) {
                "ffprobe.exe"
            } else {
                "ffprobe"
            })
        })
        .filter(|p| p.exists())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "ffprobe".to_string());

    util::run_capture(
        &probe,
        &[
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            &input.display().to_string(),
        ],
    )
    .ok()
    .and_then(|s| s.trim().parse::<f64>().ok())
    .filter(|d| *d > 0.0)
}

/// Where the result lands: same folder, stem tagged so the source is never clobbered.
pub fn output_path(input: &Path, op: RemuxOp, container: &str, audio_ext: &str) -> PathBuf {
    let dir = input.parent().unwrap_or(Path::new("."));
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".into());

    let (suffix, ext) = match op {
        RemuxOp::Container => ("remux", container.to_string()),
        RemuxOp::ExtractAudio => ("audio", audio_ext.to_string()),
        RemuxOp::StripAudio => ("mute", container.to_string()),
        RemuxOp::ToGif => ("gif", "gif".to_string()),
    };

    let mut candidate = dir.join(format!("{stem} ({suffix}).{ext}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem} ({suffix} {n}).{ext}"));
        n += 1;
    }
    candidate
}

fn build_args(
    input: &Path,
    output: &Path,
    op: RemuxOp,
    audio_codec: &str,
    gif_fps: u32,
    gif_width: u32,
) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-nostats".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-y".into(),
        "-i".into(),
        input.display().to_string(),
    ];

    match op {
        RemuxOp::Container => {
            a.extend(["-map".into(), "0".into(), "-c".into(), "copy".into()]);
        }
        RemuxOp::StripAudio => {
            a.extend([
                "-map".into(),
                "0:v".into(),
                "-an".into(),
                "-c".into(),
                "copy".into(),
            ]);
        }
        RemuxOp::ExtractAudio => {
            a.extend(["-vn".into(), "-map".into(), "0:a:0".into()]);
            if audio_codec == "copy" {
                a.extend(["-c:a".into(), "copy".into()]);
            } else {
                a.extend(["-c:a".into(), audio_codec.into()]);
            }
        }
        RemuxOp::ToGif => {
            a.extend([
                "-vf".into(),
                format!(
                    "fps={gif_fps},scale={gif_width}:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse"
                ),
                "-loop".into(),
                "0".into(),
            ]);
        }
    }

    // mp4 needs the index up front to be seekable straight away.
    if output
        .extension()
        .map(|e| e.eq_ignore_ascii_case("mp4"))
        .unwrap_or(false)
    {
        a.extend(["-movflags".into(), "+faststart".into()]);
    }

    a.push(output.display().to_string());
    a
}

#[allow(clippy::too_many_arguments)]
pub fn spawn(
    input: PathBuf,
    output: PathBuf,
    op: RemuxOp,
    audio_codec: String,
    gif_fps: u32,
    gif_width: u32,
    settings: Settings,
    child_slot: Arc<Mutex<Option<Child>>>,
    cancel_flag: Arc<AtomicBool>,
    tx: Sender<RemuxEvent>,
    repaint: impl Fn() + Send + 'static,
) {
    std::thread::spawn(move || {
        let _ = tx.send(RemuxEvent::State(RemuxState::Running));
        repaint();

        let bin = settings.ffmpeg_bin();
        let duration = probe_duration(&bin, &input);
        let args = build_args(&input, &output, op, &audio_codec, gif_fps, gif_width);

        let spawned = util::command(&bin)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .spawn();

        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(RemuxEvent::State(RemuxState::Failed(format!(
                    "could not start {bin}: {e}. set the ffmpeg path in settings > advanced."
                ))));
                repaint();
                return;
            }
        };

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        if let Ok(mut slot) = child_slot.lock() {
            *slot = Some(child);
        }

        let err_tx = tx.clone();
        let err_handle = stderr.map(|se| {
            std::thread::spawn(move || {
                let mut collected = Vec::new();
                for line in BufReader::new(se).lines().map_while(Result::ok) {
                    let t = line.trim().to_string();
                    if t.is_empty() {
                        continue;
                    }
                    collected.push(t.clone());
                    let _ = err_tx.send(RemuxEvent::Log(t));
                }
                collected
            })
        });

        if let Some(so) = stdout {
            for line in BufReader::new(so).lines().map_while(Result::ok) {
                if cancel_flag.load(Ordering::Relaxed) {
                    break;
                }
                // `-progress pipe:1` emits key=value lines; out_time_us drives the bar.
                if let Some(v) = line.trim().strip_prefix("out_time_us=") {
                    if let (Ok(us), Some(dur)) = (v.trim().parse::<f64>(), duration) {
                        let frac = ((us / 1_000_000.0) / dur).clamp(0.0, 1.0) as f32;
                        let _ = tx.send(RemuxEvent::Progress(frac));
                        repaint();
                    }
                }
            }
        }

        let status = {
            let mut guard = child_slot.lock().ok();
            match guard.as_mut().and_then(|g| g.as_mut()) {
                Some(c) => c.wait().ok(),
                None => None,
            }
        };
        if let Ok(mut slot) = child_slot.lock() {
            *slot = None;
        }
        let stderr_tail = err_handle.and_then(|h| h.join().ok()).unwrap_or_default();

        let state = if cancel_flag.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(&output);
            RemuxState::Cancelled
        } else if status.map(|s| s.success()).unwrap_or(false) {
            if !settings.processing.keep_source_after_remux {
                let _ = std::fs::remove_file(&input);
            }
            RemuxState::Done(output)
        } else {
            let _ = std::fs::remove_file(&output);
            RemuxState::Failed(
                stderr_tail
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "ffmpeg exited with an error".into()),
            )
        };

        let _ = tx.send(RemuxEvent::State(state));
        repaint();
    });
}
