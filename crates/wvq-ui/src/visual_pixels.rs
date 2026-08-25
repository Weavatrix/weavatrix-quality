//! Exact RGBA crops. Not a perceptual distance.

use std::io::Cursor;

use png::{BitDepth, ColorType, Decoder, Encoder, Transformations};

use crate::snapshot::Rect;

/// Ceiling on pixels examined in one crop. Larger crops stay unmeasured.
pub const MAX_CROP_PIXELS: u64 = 4_000_000;

/// Decoded 8-bit RGBA bitmap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelFrame {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Packed RGBA, `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

impl PixelFrame {
    /// Decode a PNG into 8-bit RGBA. Expanding palettes and grey is allowed;
    /// a truncated or malformed file is a limitation, not a visual change.
    ///
    /// # Errors
    ///
    /// The bytes are not a PNG, or they do not expand to 8-bit RGBA.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut decoder = Decoder::new(Cursor::new(bytes));
        decoder.set_transformations(Transformations::EXPAND | Transformations::ALPHA);
        let mut reader = decoder
            .read_info()
            .map_err(|error| format!("png decode failed: {error}"))?;
        let mut rgba = vec![0_u8; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut rgba)
            .map_err(|error| format!("png frame failed: {error}"))?;
        if info.color_type != ColorType::Rgba || info.bit_depth != BitDepth::Eight {
            return Err(format!(
                "png expanded to {:?}/{:?}, not 8-bit RGBA",
                info.color_type, info.bit_depth
            ));
        }
        rgba.truncate(info.buffer_size());
        Ok(Self {
            width: info.width,
            height: info.height,
            rgba,
        })
    }
}

/// Encode packed RGBA8 as a PNG. Used by tests and crop diagnostics.
///
/// # Errors
///
/// Buffer length does not match `width * height * 4`, or the encoder fails.
pub fn encode_rgba_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let expected = (u64::from(width).saturating_mul(u64::from(height))).saturating_mul(4);
    if u64::try_from(rgba.len()).unwrap_or(u64::MAX) != expected {
        return Err(format!(
            "rgba buffer is {} bytes, expected {expected} for {width}x{height}",
            rgba.len()
        ));
    }
    let mut out = Vec::new();
    {
        let mut encoder = Encoder::new(&mut out, width, height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| format!("png encode header: {error}"))?;
        writer
            .write_image_data(rgba)
            .map_err(|error| format!("png encode data: {error}"))?;
    }
    Ok(out)
}

/// Exact pixel compare of two crops. `css_width` is the viewport width that
/// produced both screenshots, so a 2× PNG maps CSS pixels through
/// `frame.width / css_width`.
pub fn crop_matches(
    base: &PixelFrame,
    head: &PixelFrame,
    base_rect: &Rect,
    head_rect: &Rect,
    css_width: f64,
) -> Result<(u64, u64), String> {
    if base.width != head.width {
        return Err(format!(
            "screenshot widths differ ({} vs {}); crops are unaligned",
            base.width, head.width
        ));
    }
    let base_crop = bitmap_crop(base, base_rect, css_width)?;
    let head_crop = bitmap_crop(head, head_rect, css_width)?;
    if base_crop.width != head_crop.width || base_crop.height != head_crop.height {
        return Err(format!(
            "crop sizes differ ({}x{} vs {}x{})",
            base_crop.width, base_crop.height, head_crop.width, head_crop.height
        ));
    }
    let compared = u64::from(base_crop.width).saturating_mul(u64::from(base_crop.height));
    if compared == 0 {
        return Err("crop has no pixels".into());
    }
    if compared > MAX_CROP_PIXELS {
        return Err(format!(
            "crop is {compared} pixels, above the {MAX_CROP_PIXELS} ceiling"
        ));
    }
    let mut mismatched = 0_u64;
    for (left, right) in base_crop
        .rgba
        .chunks_exact(4)
        .zip(head_crop.rgba.chunks_exact(4))
    {
        if left != right {
            mismatched = mismatched.saturating_add(1);
        }
    }
    Ok((compared, mismatched))
}

struct Crop {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn bitmap_crop(frame: &PixelFrame, rect: &Rect, css_width: f64) -> Result<Crop, String> {
    if css_width <= 0.0 || !css_width.is_finite() {
        return Err("cannot scale a crop without a viewport width".into());
    }
    let pixel_scale = f64::from(frame.width) / css_width;
    let x0 = to_px(rect.x, pixel_scale);
    let y0 = to_px(rect.y, pixel_scale);
    let x1 = to_px(rect.right(), pixel_scale).min(frame.width);
    let y1 = to_px(rect.bottom(), pixel_scale).min(frame.height);
    if x1 <= x0 || y1 <= y0 {
        return Err("crop is outside the screenshot".into());
    }
    let width = x1.saturating_sub(x0);
    let height = y1.saturating_sub(y0);
    let row_bytes = usize::try_from(u64::from(width).saturating_mul(4)).unwrap_or(0);
    let mut rgba = Vec::new();
    for row in y0..y1 {
        let start = usize::try_from(
            (u64::from(row) * u64::from(frame.width) + u64::from(x0)).saturating_mul(4),
        )
        .map_err(|_| "crop offset does not fit")?;
        let end = start.saturating_add(row_bytes);
        let Some(slice) = frame.rgba.get(start..end) else {
            return Err("crop walked off the decoded bitmap".into());
        };
        rgba.extend_from_slice(slice);
    }
    Ok(Crop {
        width,
        height,
        rgba,
    })
}

fn to_px(css: f64, scale: f64) -> u32 {
    if !css.is_finite() || !scale.is_finite() || css <= 0.0 {
        return 0;
    }
    let scaled = (css * scale).floor();
    if !scaled.is_finite() || scaled <= 0.0 {
        return 0;
    }
    if scaled >= f64::from(u32::MAX) {
        return u32::MAX;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        scaled as u32
    }
}
