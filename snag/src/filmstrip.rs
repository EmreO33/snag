//! Frames pulled out along a video, so the clip screen can show you what you
//! are cutting instead of asking you to type times at a file you cannot see.
//!
//! Each frame is a separate short ffmpeg run that seeks straight to its point
//! in the file. That is quicker than one pass sampling as it decodes, because
//! seeking to twelve places costs the same on an hour long video as on a one
//! minute one, and it means the strip arrives even if the file is huge.

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use crate::settings::Settings;
use crate::util;

/// How many frames make up the strip. Enough to recognise scenes at a glance,
/// few enough that the whole thing is ready in about a second.
const FRAMES: usize = 12;

/// How wide each extracted frame is. They are drawn a fraction of the window
/// wide, so anything larger is detail nobody sees.
const FRAME_WIDTH: u32 = 213;

/// What the background thread found out about a file.
pub struct Strip {
    /// The file this describes, so a late arrival for a file that is no longer
    /// loaded can be dropped.
    pub input: PathBuf,
    pub duration: Option<f64>,
    /// Frames in order, first to last. Empty when the file has no video, which
    /// is not an error: an audio file simply has nothing to show.
    pub frames: Vec<egui::ColorImage>,
}

/// Pull one frame at `at` seconds, as a decoded image.
fn frame_at(ffmpeg: &str, input: &std::path::Path, at: f64) -> Option<egui::ColorImage> {
    // Written to stdout rather than to a file, so there is nothing to name,
    // nothing to collide, and nothing to clean up afterwards.
    let out = util::command(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-ss",
            &at.to_string(),
            "-i",
            &input.display().to_string(),
            "-frames:v",
            "1",
            "-vf",
            &format!("scale={FRAME_WIDTH}:-2"),
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "-",
        ])
        .output()
        .ok()?;

    if !out.status.success() || out.stdout.is_empty() {
        return None;
    }

    let decoded = image::load_from_memory(&out.stdout).ok()?;
    let rgba = decoded.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        rgba.as_flat_samples().as_slice(),
    ))
}

/// Work out how long `input` runs for and pull a strip of frames from it.
///
/// The duration is sent as soon as it is known and the frames follow, because
/// the duration alone is enough to draw the scrubber and waiting for twelve
/// ffmpeg runs before showing anything would feel broken.
pub fn spawn(
    input: PathBuf,
    settings: Settings,
    tx: Sender<Strip>,
    repaint: impl Fn() + Send + 'static,
) {
    std::thread::spawn(move || {
        let ffmpeg = settings.ffmpeg_bin();
        let duration = crate::remux::probe_duration(&ffmpeg, &input);

        let _ = tx.send(Strip {
            input: input.clone(),
            duration,
            frames: Vec::new(),
        });
        repaint();

        let Some(duration) = duration.filter(|d| *d > 0.0) else {
            return;
        };

        let mut frames = Vec::with_capacity(FRAMES);
        for i in 0..FRAMES {
            // The middle of each slot rather than its edge: the very first and
            // last moments of a video are often black.
            let at = duration * (i as f64 + 0.5) / FRAMES as f64;
            match frame_at(&ffmpeg, &input, at) {
                Some(image) => frames.push(image),
                // One unreadable frame should not cost the whole strip, but a
                // file with no video at all fails on the first and there is no
                // sense grinding through eleven more.
                None if i == 0 => return,
                None => {}
            }
        }

        let _ = tx.send(Strip {
            input,
            duration: Some(duration),
            frames,
        });
        repaint();
    });
}
