//! Movement, and the one switch that turns all of it off.
//!
//! Snag's widgets are painted by hand, so nothing animates unless it is asked
//! to. These are the askings: a hover that fades rather than flicks, a
//! selection mark that slides to where you clicked, a screen that arrives
//! instead of appearing. The rules kept to throughout:
//!
//! - Nothing moves for longer than a fifth of a second. Anything slower is
//!   the app making you wait to watch it.
//! - Nothing moves position by more than a few pixels, so an animation can
//!   never be mistaken for the layout changing.
//! - Everything eases out, never in: the motion starts at full speed and
//!   settles, which is what makes it feel like a response rather than a
//!   performance.
//! - Progress is the exception that is allowed to be slow, because it is
//!   smoothing over yt-dlp's coarse reporting rather than decorating.
//!
//! When the setting is off, every helper here returns the final value
//! immediately and nothing requests another frame.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;
use egui::{Color32, Id, Rgba};

/// Read on every painted frame and written when the setting changes, which
/// is why it is not simply passed down: every hand painted widget in the app
/// would need the settings threaded through it for one boolean.
static ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// A hover or press: fast enough to feel like the cursor is causing it.
pub const HOVER: f32 = 0.09;
/// A selection moving, a panel swapping: slow enough to be followed.
pub const SWITCH: f32 = 0.16;
/// Something arriving on screen for the first time.
pub const ENTER: f32 = 0.2;

/// 0 to 1 for a state that is either on or off, eased out.
pub fn on_off(ctx: &egui::Context, id: Id, on: bool, seconds: f32) -> f32 {
    if !enabled() {
        return on as u8 as f32;
    }
    ctx.animate_bool_with_time_and_easing(id, on, seconds, egui::emath::easing::cubic_out)
}

/// A number that follows `target` rather than jumping to it.
pub fn toward(ctx: &egui::Context, id: Id, target: f32, seconds: f32) -> f32 {
    if !enabled() {
        return target;
    }
    ctx.animate_value_with_time(id, target, seconds)
}

/// How far into an animation that started at `since`, eased out. Used where
/// there is no boolean to track, such as a toast that appears once and is
/// then gone.
pub fn since(started: std::time::Instant, seconds: f32) -> f32 {
    ramp(started.elapsed().as_secs_f32(), seconds)
}

/// The same, for a caller that already knows how long ago it was.
pub fn ramp(elapsed: f32, seconds: f32) -> f32 {
    if !enabled() {
        return 1.0;
    }
    egui::emath::easing::cubic_out((elapsed / seconds).clamp(0.0, 1.0))
}

/// Blend two colours, alpha included, so a fade from nothing to a fill does
/// not pass through a grey.
pub fn mix(from: Color32, to: Color32, t: f32) -> Color32 {
    if t <= 0.0 {
        return from;
    }
    if t >= 1.0 {
        return to;
    }
    let a = Rgba::from(from);
    let b = Rgba::from(to);
    Color32::from(egui::lerp(a..=b, t))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The switch has to mean it: off is not "faster", it is "already
    /// there", or a machine that cannot afford the frames still pays for
    /// them.
    #[test]
    fn off_means_instant() {
        set_enabled(true);
        assert!(ramp(0.0, 0.2) < 0.01, "an animation starts where it was");
        assert!(ramp(0.1, 0.2) > 0.0 && ramp(0.1, 0.2) < 1.0);
        assert_eq!(ramp(5.0, 0.2), 1.0, "and finishes, rather than drifting");

        set_enabled(false);
        assert_eq!(ramp(0.0, 0.2), 1.0);
        assert_eq!(ramp(0.1, 0.2), 1.0);
        set_enabled(true);
    }

    #[test]
    fn mixing_keeps_both_ends_exact() {
        let a = Color32::from_rgb(10, 20, 30);
        let b = Color32::from_rgb(200, 100, 50);
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
        let half = mix(a, b, 0.5);
        assert!(half.r() > a.r() && half.r() < b.r());
        // A fade from nothing must not brighten on the way, which is what
        // blending a transparent colour without its alpha would do.
        let from_nothing = mix(Color32::TRANSPARENT, b, 0.5);
        assert!(from_nothing.a() > 0 && from_nothing.a() < 255);
    }
}
