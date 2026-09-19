//! The Snag mark, decoded from PNGs baked into the binary.
//!
//! Two variants are embedded. The window and taskbar icon is the logo on the
//! brand's near-black rounded square, so it stays legible against any taskbar
//! colour. The in-app mark is the bare white logo, which the UI tints with the
//! current theme's text colour.

/// The launcher icon: the mark on its own background.
const ICON_PNG: &[u8] = include_bytes!("../../assets/icon.png");

/// The bare mark, drawn inside the app and tinted at runtime.
const MARK_PNG: &[u8] = include_bytes!("../../assets/logo-mark.png");

/// Decode a PNG into `(width, height, rgba)`.
fn decode(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());

    // Everything we ship is 8-bit, but normalise the channel count so a future
    // re-export without an alpha channel cannot silently break the icon.
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => return None,
    };

    Some((info.width, info.height, rgba))
}

/// The window icon, or None if the embedded PNG will not decode.
pub fn icon_data() -> Option<egui::IconData> {
    let (width, height, rgba) = decode(ICON_PNG)?;
    Some(egui::IconData {
        rgba,
        width,
        height,
    })
}

/// The launcher icon as raw RGBA, for the system tray. Unused where there is
/// no tray to put it in.
#[cfg_attr(
    not(any(windows, target_os = "macos")),
    allow(dead_code, reason = "no tray on this platform")
)]
pub fn tray_rgba() -> Option<(u32, u32, Vec<u8>)> {
    decode(ICON_PNG)
}

/// The bare mark as an egui image, ready to be tinted.
pub fn mark_image() -> Option<egui::ColorImage> {
    let (width, height, rgba) = decode(MARK_PNG)?;
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        &rgba,
    ))
}
