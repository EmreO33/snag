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
    Clip,
    ExtractAudio,
    StripAudio,
    ToGif,
}

impl RemuxOp {
    pub const ALL: &'static [RemuxOp] = &[
        RemuxOp::Container,
        RemuxOp::Clip,
        RemuxOp::ExtractAudio,
        RemuxOp::StripAudio,
        RemuxOp::ToGif,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            RemuxOp::Container => "change container",
            RemuxOp::Clip => "clip",
            RemuxOp::ExtractAudio => "extract audio",
            RemuxOp::StripAudio => "mute",
            RemuxOp::ToGif => "to gif",
        }
    }
    pub fn note(&self) -> &'static str {
        match self {
            RemuxOp::Container => "rewraps the same streams into another container. no quality loss, near-instant.",
            RemuxOp::Clip => "keeps the part between two times and throws the rest away. the original file is left alone.",
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

/// Everything an operation needs beyond the two paths.
///
/// A struct rather than more arguments: this had already grown past the point
/// where a reader could tell the two gif numbers apart at the call site.
#[derive(Debug, Clone)]
pub struct Options {
    pub audio_codec: String,
    pub gif_fps: u32,
    pub gif_width: u32,
    /// Where a clip starts, in seconds from the beginning of the source.
    pub clip_start: f64,
    /// Where it ends. None runs to the end of the file.
    pub clip_end: Option<f64>,
    /// Cut exactly where asked rather than at the nearest keyframe, which
    /// means re-encoding.
    pub clip_exact: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            audio_codec: "copy".into(),
            gif_fps: 15,
            gif_width: 480,
            clip_start: 0.0,
            clip_end: None,
            clip_exact: false,
        }
    }
}

impl Options {
    /// How long the result runs for, when that is knowable without the file.
    fn clip_length(&self, source: Option<f64>) -> Option<f64> {
        let end = self.clip_end.or(source)?;
        Some((end - self.clip_start).max(0.0))
    }
}

/// Read a time written the way a person writes one: `90`, `1:30`, `1:02:03`,
/// or `1:30.5`.
///
/// Returns None for anything that is not a time, so the UI can refuse to start
/// rather than handing ffmpeg something it will interpret its own way.
pub fn parse_timecode(raw: &str) -> Option<f64> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }

    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() > 3 {
        return None;
    }

    let mut seconds = 0.0;
    for (i, part) in parts.iter().enumerate() {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        let value: f64 = part.parse().ok()?;
        if !value.is_finite() || value < 0.0 {
            return None;
        }
        // 1:75 is a typo, not 2:15. Only the leading field may run over,
        // because 90:00 is a legitimate way to write an hour and a half.
        if i > 0 && value >= 60.0 {
            return None;
        }
        seconds += value * 60f64.powi((parts.len() - 1 - i) as i32);
    }
    Some(seconds)
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

    let source_ext = input
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "mp4".into());

    let (suffix, ext) = match op {
        RemuxOp::Container => ("remux", container.to_string()),
        // A clip stays in whatever the source already was: the point is to
        // take a piece of it, not to change it into something else.
        RemuxOp::Clip => ("clip", source_ext),
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

fn build_args(input: &Path, output: &Path, op: RemuxOp, o: &Options) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-nostats".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-y".into(),
    ];

    // Seeking before -i is the fast kind: ffmpeg jumps straight there instead
    // of decoding its way through everything before it. The catch is that a
    // stream copy can only start on a keyframe, so an exact cut has to seek on
    // the output side and re-encode to get the frame actually asked for.
    let seek_on_input = op == RemuxOp::Clip && !o.clip_exact;
    if seek_on_input {
        a.extend(["-ss".into(), o.clip_start.to_string()]);
        if let Some(end) = o.clip_end {
            a.extend(["-to".into(), end.to_string()]);
        }
    }

    a.extend(["-i".into(), input.display().to_string()]);

    if op == RemuxOp::Clip && o.clip_exact {
        a.extend(["-ss".into(), o.clip_start.to_string()]);
        if let Some(end) = o.clip_end {
            a.extend(["-to".into(), end.to_string()]);
        }
    }

    let audio_codec = o.audio_codec.as_str();
    let (gif_fps, gif_width) = (o.gif_fps, o.gif_width);

    match op {
        RemuxOp::Container => {
            a.extend(["-map".into(), "0".into(), "-c".into(), "copy".into()]);
        }
        RemuxOp::Clip => {
            a.extend(["-map".into(), "0".into()]);
            if o.clip_exact {
                // The source could be anything, so the cut lands on codecs
                // that every container and player will take.
                a.extend([
                    "-c:v".into(),
                    "libx264".into(),
                    "-crf".into(),
                    "18".into(),
                    "-preset".into(),
                    "veryfast".into(),
                    "-c:a".into(),
                    "aac".into(),
                ]);
            } else {
                a.extend(["-c".into(), "copy".into()]);
            }
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

// The two halves of "stop this" and the two ends of the event channel are
// four of these on their own, and bundling them would hide what the caller is
// handing over rather than clarify it.
#[allow(clippy::too_many_arguments)]
pub fn spawn(
    input: PathBuf,
    output: PathBuf,
    op: RemuxOp,
    options: Options,
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
        let source_duration = probe_duration(&bin, &input);
        // A clip's progress runs against the length of the clip, not of the
        // file it came out of, or a ten second cut from an hour long video
        // would sit at one percent and then finish.
        let duration = if op == RemuxOp::Clip {
            options.clip_length(source_duration)
        } else {
            source_duration
        };
        let args = build_args(&input, &output, op, &options);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_ways_people_write_a_time() {
        assert_eq!(parse_timecode("90"), Some(90.0));
        assert_eq!(parse_timecode("1:30"), Some(90.0));
        assert_eq!(parse_timecode("0:01:30"), Some(90.0));
        assert_eq!(parse_timecode("1:02:03"), Some(3723.0));
        assert_eq!(parse_timecode("  2:00  "), Some(120.0));
        assert_eq!(parse_timecode("1:30.5"), Some(90.5));
        // An hour and a half is a fine thing to write as ninety minutes.
        assert_eq!(parse_timecode("90:00"), Some(5400.0));
    }

    #[test]
    fn refuses_anything_that_is_not_a_time() {
        for bad in [
            "", "   ", "abc", "1:", ":30", "1::30", "-5", "1:2:3:4", "1:75",
        ] {
            assert_eq!(parse_timecode(bad), None, "should be rejected: {bad:?}");
        }
    }

    #[test]
    fn a_copied_clip_seeks_before_the_input_and_an_exact_one_after() {
        let input = Path::new("in.mp4");
        let output = Path::new("out.mp4");
        let options = Options {
            clip_start: 10.0,
            clip_end: Some(20.0),
            ..Options::default()
        };

        let copied = build_args(input, output, RemuxOp::Clip, &options);
        let i = copied.iter().position(|a| a == "-i").unwrap();
        let ss = copied.iter().position(|a| a == "-ss").unwrap();
        assert!(ss < i, "a stream copy seeks the input: {copied:?}");
        assert!(copied.windows(2).any(|w| w == ["-c", "copy"]));

        let exact = build_args(
            input,
            output,
            RemuxOp::Clip,
            &Options {
                clip_exact: true,
                ..options
            },
        );
        let i = exact.iter().position(|a| a == "-i").unwrap();
        let ss = exact.iter().position(|a| a == "-ss").unwrap();
        assert!(ss > i, "an exact cut seeks the output: {exact:?}");
        assert!(!exact.windows(2).any(|w| w == ["-c", "copy"]));
    }

    #[test]
    fn an_open_ended_clip_runs_to_the_end_of_the_file() {
        let options = Options {
            clip_start: 5.0,
            clip_end: None,
            ..Options::default()
        };
        let args = build_args(
            Path::new("in.mkv"),
            Path::new("out.mkv"),
            RemuxOp::Clip,
            &options,
        );
        assert!(!args.iter().any(|a| a == "-to"), "{args:?}");
        assert_eq!(options.clip_length(Some(30.0)), Some(25.0));
        assert_eq!(options.clip_length(None), None);
    }
}
