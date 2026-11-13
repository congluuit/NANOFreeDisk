//! Turning the bytes of a picture file into upright brightness pixels, with hard
//! limits so a hostile or broken file costs little and never takes the app down.
//!
//! * JPEG (most photos) goes through `zune-jpeg` straight to brightness: no chroma
//!   upsampling and no colour conversion, one byte per pixel. None of the pure-Rust
//!   JPEG decoders can scale while decoding (libjpeg's 1/8 IDCT), so the picture is
//!   decoded at full size and averaged down afterwards.
//! * PNG, WebP, GIF (first frame) and BMP go through the `image` crate with its
//!   `Limits` (size and allocation caps) and are reduced to brightness; transparent
//!   parts are laid over white, as a viewer shows them.
//! * The EXIF orientation is honoured (JPEG APP1, PNG `eXIf`, WebP `EXIF`), so a
//!   photo saved sideways with a rotation flag matches its upright copy.
//!
//! The caller reads at most [`super::MAX_FILE`] bytes and wraps this in
//! `catch_unwind`: a decoder bug on a crafted file is "unreadable", nothing more.

use std::io::Cursor;

use image::metadata::Orientation;
use image::{DynamicImage, GrayImage, ImageDecoder, ImageError, ImageFormat, ImageReader, Limits};

/// Longest side accepted (pixels).
pub const MAX_SIDE: usize = 30_000;
/// Largest JPEG decoded (pixels; one byte each): 100 MP covers 108 MP phone modes
/// but not 200 MP ones.
pub const JPEG_MAX_PIXELS: usize = 100_000_000;
/// Largest picture of another format decoded (pixels; up to 4 bytes each).
pub const OTHER_MAX_PIXELS: usize = 40_000_000;
/// Memory one non-JPEG decode may allocate.
pub const MAX_ALLOC: u64 = 256 << 20;

/// Upright brightness pixels, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Luma {
    pub width: usize,
    pub height: usize,
    pub px: Vec<u8>,
}

/// Why a picture could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fail {
    /// Not a supported picture, damaged or cut short.
    Unreadable,
    /// More pixels (or memory) than the limits allow.
    TooLarge,
}

/// `true` for the JPEG start-of-image marker.
fn is_jpeg(data: &[u8]) -> bool {
    data.starts_with(&[0xFF, 0xD8, 0xFF])
}

/// Decode `data` (whatever its extension says: the content decides).
///
/// # Errors
/// [`Fail`] for anything but a picture within the limits.
pub fn decode(data: &[u8]) -> Result<Luma, Fail> {
    if is_jpeg(data) {
        jpeg(data)
    } else {
        other(data)
    }
}

fn jpeg(data: &[u8]) -> Result<Luma, Fail> {
    use zune_jpeg::JpegDecoder;
    use zune_jpeg::zune_core::bytestream::ZCursor;
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;

    let options = DecoderOptions::default()
        .set_max_width(MAX_SIDE)
        .set_max_height(MAX_SIDE)
        .set_strict_mode(false)
        .jpeg_set_out_colorspace(ColorSpace::Luma);
    let mut d = JpegDecoder::new_with_options(ZCursor::new(data), options);
    d.decode_headers().map_err(|_| Fail::Unreadable)?;
    let (w, h) = d.dimensions().ok_or(Fail::Unreadable)?;
    if w == 0 || h == 0 {
        return Err(Fail::Unreadable);
    }
    if w.saturating_mul(h) > JPEG_MAX_PIXELS {
        return Err(Fail::TooLarge);
    }
    let orientation = d
        .exif()
        .and_then(|e| Orientation::from_exif_chunk(e))
        .unwrap_or(Orientation::NoTransforms);
    let px = d.decode().map_err(|_| Fail::Unreadable)?;
    if px.len() != w * h {
        return Err(Fail::Unreadable);
    }
    upright(w, h, px, orientation)
}

fn limits() -> Limits {
    let mut l = Limits::default();
    l.max_image_width = u32::try_from(MAX_SIDE).ok();
    l.max_image_height = u32::try_from(MAX_SIDE).ok();
    l.max_alloc = Some(MAX_ALLOC);
    l
}

fn fail(e: &ImageError) -> Fail {
    match e {
        ImageError::Limits(_) => Fail::TooLarge,
        _ => Fail::Unreadable,
    }
}

fn other(data: &[u8]) -> Result<Luma, Fail> {
    let mut reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|_| Fail::Unreadable)?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Png | ImageFormat::Gif | ImageFormat::WebP | ImageFormat::Bmp)
    ) {
        return Err(Fail::Unreadable);
    }
    reader.limits(limits());
    let mut decoder = reader.into_decoder().map_err(|e| fail(&e))?;
    let (w, h) = decoder.dimensions();
    let (w, h) = (w as usize, h as usize);
    if w == 0 || h == 0 {
        return Err(Fail::Unreadable);
    }
    if w.saturating_mul(h) > OTHER_MAX_PIXELS {
        return Err(Fail::TooLarge);
    }
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let img = DynamicImage::from_decoder(decoder).map_err(|e| fail(&e))?;
    let px = brightness(img);
    if px.len() != w * h {
        return Err(Fail::Unreadable);
    }
    upright(w, h, px, orientation)
}

/// Brightness of each pixel (BT.601 weights, as JPEG's own luma), transparent
/// pixels laid over white.
fn brightness(img: DynamicImage) -> Vec<u8> {
    match img {
        DynamicImage::ImageLuma8(b) => b.into_raw(),
        other => other
            .into_rgba8()
            .pixels()
            .map(|p| {
                let [r, g, b, a] = p.0;
                let l = (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)) / 1000;
                let a = u32::from(a);
                ((l * a + 255 * (255 - a)) / 255) as u8
            })
            .collect(),
    }
}

/// Apply the EXIF orientation.
fn upright(w: usize, h: usize, px: Vec<u8>, o: Orientation) -> Result<Luma, Fail> {
    if o == Orientation::NoTransforms {
        return Ok(Luma {
            width: w,
            height: h,
            px,
        });
    }
    let buf = GrayImage::from_raw(
        u32::try_from(w).map_err(|_| Fail::TooLarge)?,
        u32::try_from(h).map_err(|_| Fail::TooLarge)?,
        px,
    )
    .ok_or(Fail::Unreadable)?;
    let mut img = DynamicImage::ImageLuma8(buf);
    img.apply_orientation(o);
    let (width, height) = (img.width() as usize, img.height() as usize);
    Ok(Luma {
        width,
        height,
        px: img.into_luma8().into_raw(),
    })
}

/// A colour preview of the picture in `data`, upright and at most `max_side`
/// pixels on its longer side, as PNG bytes (for files too large to show as they
/// are). Same limits as [`decode`].
///
/// # Errors
/// [`Fail`] for anything but a picture within the limits.
pub fn preview(data: &[u8], max_side: u32) -> Result<Vec<u8>, Fail> {
    let (img, orientation) = if is_jpeg(data) {
        use zune_jpeg::JpegDecoder;
        use zune_jpeg::zune_core::bytestream::ZCursor;
        use zune_jpeg::zune_core::colorspace::ColorSpace;
        use zune_jpeg::zune_core::options::DecoderOptions;

        let options = DecoderOptions::default()
            .set_max_width(MAX_SIDE)
            .set_max_height(MAX_SIDE)
            .set_strict_mode(false)
            .jpeg_set_out_colorspace(ColorSpace::RGB);
        let mut d = JpegDecoder::new_with_options(ZCursor::new(data), options);
        d.decode_headers().map_err(|_| Fail::Unreadable)?;
        let (w, h) = d.dimensions().ok_or(Fail::Unreadable)?;
        // three bytes per pixel here: the smaller limit of the other formats
        if w.saturating_mul(h) > OTHER_MAX_PIXELS {
            return Err(Fail::TooLarge);
        }
        let orientation = d
            .exif()
            .and_then(|e| Orientation::from_exif_chunk(e))
            .unwrap_or(Orientation::NoTransforms);
        let px = d.decode().map_err(|_| Fail::Unreadable)?;
        let buf = image::RgbImage::from_raw(
            u32::try_from(w).map_err(|_| Fail::TooLarge)?,
            u32::try_from(h).map_err(|_| Fail::TooLarge)?,
            px,
        )
        .ok_or(Fail::Unreadable)?;
        (DynamicImage::ImageRgb8(buf), orientation)
    } else {
        let mut reader = ImageReader::new(Cursor::new(data))
            .with_guessed_format()
            .map_err(|_| Fail::Unreadable)?;
        if !matches!(
            reader.format(),
            Some(ImageFormat::Png | ImageFormat::Gif | ImageFormat::WebP | ImageFormat::Bmp)
        ) {
            return Err(Fail::Unreadable);
        }
        reader.limits(limits());
        let mut decoder = reader.into_decoder().map_err(|e| fail(&e))?;
        let (w, h) = decoder.dimensions();
        if (w as usize).saturating_mul(h as usize) > OTHER_MAX_PIXELS {
            return Err(Fail::TooLarge);
        }
        let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
        (
            DynamicImage::from_decoder(decoder).map_err(|e| fail(&e))?,
            orientation,
        )
    };
    let small = img.thumbnail(max_side.max(1), max_side.max(1));
    drop(img);
    let mut small = DynamicImage::ImageRgba8(small.into_rgba8());
    small.apply_orientation(orientation);
    let mut out = Cursor::new(Vec::new());
    small
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|_| Fail::Unreadable)?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_and_truncated_files_are_unreadable() {
        assert_eq!(decode(b"").err(), Some(Fail::Unreadable));
        assert_eq!(
            decode(b"hello, not a picture").err(),
            Some(Fail::Unreadable)
        );
        // a JPEG start marker and nothing else
        assert_eq!(
            decode(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 4]).err(),
            Some(Fail::Unreadable)
        );
        // a PNG signature and nothing else
        assert_eq!(
            decode(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]).err(),
            Some(Fail::Unreadable)
        );
    }

    #[test]
    fn previews_are_small_upright_pngs() {
        let img = image::RgbImage::from_fn(300, 100, |x, _| image::Rgb([(x % 256) as u8, 40, 90]));
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(img)
            .write_to(&mut png, ImageFormat::Png)
            .expect("png");
        let out = preview(png.get_ref(), 60).expect("preview");
        let back = image::load_from_memory_with_format(&out, ImageFormat::Png).expect("load");
        assert_eq!((back.width(), back.height()), (60, 20));
        assert_eq!(preview(b"nothing", 60).err(), Some(Fail::Unreadable));
    }

    #[test]
    fn rotation_swaps_the_sides() {
        let px: Vec<u8> = (0..6).collect();
        let l = upright(3, 2, px, Orientation::Rotate90).expect("rotate");
        assert_eq!((l.width, l.height), (2, 3));
        // first row of the rotated picture: the left column, bottom to top
        assert_eq!(&l.px[..2], &[3, 0]);
    }

    #[test]
    fn transparency_is_laid_over_white() {
        let img = DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(2, 1, vec![0, 0, 0, 0, 0, 0, 0, 255]).expect("buf"),
        );
        assert_eq!(brightness(img), [255, 0]);
    }
}
