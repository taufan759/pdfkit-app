//! Shrinks PDFs: recompresses and downsamples images, drops thumbnails and unused
//! objects, and packs objects into compressed object streams.

use std::fs;
use std::io::Cursor;
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, GrayImage, ImageFormat, RgbImage};
use lopdf::{Document, Object, ObjectId, Stream};
use rayon::prelude::*;

use crate::doc::{load, save_compact};
use crate::error::Result;

#[derive(Clone, Copy)]
pub(crate) enum Level {
    Low,
    Medium,
    High,
}

struct Profile {
    /// Longest side an image may keep, in pixels.
    max_side: u32,
    jpeg_quality: u8,
    /// Whether losslessly stored images (often screenshots) may become JPEG.
    recode_lossless: bool,
}

impl Level {
    fn profile(self) -> Profile {
        match self {
            Level::Low => Profile { max_side: 2400, jpeg_quality: 85, recode_lossless: false },
            Level::Medium => Profile { max_side: 1600, jpeg_quality: 72, recode_lossless: true },
            Level::High => Profile { max_side: 1000, jpeg_quality: 50, recode_lossless: true },
        }
    }
}

/// Images smaller than this (in pixels) are icons and rules; not worth touching.
const MIN_PIXELS: u64 = 128 * 128;

/// Returns the sizes before and after. When nothing helps, the output is a copy of the input.
pub(crate) fn compress(input: &Path, output: &Path, level: Level) -> Result<(u64, u64)> {
    let before = fs::metadata(input)?.len();
    let mut doc = load(input)?;
    let profile = level.profile();

    let jobs: Vec<(ObjectId, Stream, u8)> = doc
        .objects
        .iter()
        .filter_map(|(id, object)| match object {
            Object::Stream(stream) if is_plain_image(stream) => {
                channels(&doc, stream).map(|c| (*id, stream.clone(), c))
            }
            _ => None,
        })
        .collect();
    let replaced: Vec<(ObjectId, Stream)> = jobs
        .into_par_iter()
        .filter_map(|(id, stream, channels)| recompress(&stream, channels, &profile).map(|s| (id, s)))
        .collect();
    for (id, stream) in replaced {
        doc.objects.insert(id, Object::Stream(stream));
    }

    for page in doc.page_iter().collect::<Vec<_>>() {
        if let Ok(dict) = doc.get_dictionary_mut(page) {
            dict.remove(b"Thumb");
            dict.remove(b"PieceInfo");
        }
    }
    doc.prune_objects();

    let after = save_compact(&mut doc, output)?;
    if after >= before {
        fs::copy(input, output)?;
        return Ok((before, before));
    }
    Ok((before, after))
}

/// An 8-bit image without masks or decode arrays that would change meaning when re-encoded.
fn is_plain_image(stream: &Stream) -> bool {
    let dict = &stream.dict;
    let name_is = |key: &[u8], value: &[u8]| dict.get(key).and_then(Object::as_name).is_ok_and(|n| n == value);
    name_is(b"Subtype", b"Image")
        && !dict.get(b"ImageMask").and_then(Object::as_bool).unwrap_or(false)
        && !dict.has(b"Decode")
        && !matches!(dict.get(b"Mask"), Ok(Object::Array(_)))
        && dict.get(b"BitsPerComponent").and_then(Object::as_i64).is_ok_and(|b| b == 8)
}

/// Number of color channels for gray and RGB color spaces; `None` for anything else.
fn channels(doc: &Document, stream: &Stream) -> Option<u8> {
    let space = stream.dict.get(b"ColorSpace").ok()?;
    let space = doc.dereference(space).ok()?.1;
    match space {
        Object::Name(name) => match name.as_slice() {
            b"DeviceGray" | b"G" => Some(1),
            b"DeviceRGB" | b"RGB" => Some(3),
            _ => None,
        },
        Object::Array(parts) => {
            let kind = parts.first()?.as_name().ok()?;
            match kind {
                b"CalGray" => Some(1),
                b"CalRGB" => Some(3),
                b"ICCBased" => {
                    let profile = doc.dereference(parts.get(1)?).ok()?.1.as_stream().ok()?;
                    match profile.dict.get(b"N").and_then(Object::as_i64).ok()? {
                        1 => Some(1),
                        3 => Some(3),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn filters(stream: &Stream) -> Vec<Vec<u8>> {
    match stream.dict.get(b"Filter") {
        Ok(Object::Name(name)) => vec![name.clone()],
        Ok(Object::Array(list)) => list.iter().filter_map(|f| f.as_name().ok().map(<[u8]>::to_vec)).collect(),
        _ => Vec::new(),
    }
}

/// Re-encodes one image as JPEG, downsampled to the profile; `None` when that would not help.
fn recompress(stream: &Stream, channels: u8, profile: &Profile) -> Option<Stream> {
    let width = u32::try_from(stream.dict.get(b"Width").and_then(Object::as_i64).ok()?).ok()?;
    let height = u32::try_from(stream.dict.get(b"Height").and_then(Object::as_i64).ok()?).ok()?;
    if u64::from(width) * u64::from(height) < MIN_PIXELS {
        return None;
    }

    let image = match filters(stream).as_slice() {
        [f] if f == b"DCTDecode" => {
            image::load_from_memory_with_format(&stream.content, ImageFormat::Jpeg).ok()?
        }
        [f] if f == b"FlateDecode" && profile.recode_lossless => {
            let raw = stream.decompressed_content().ok()?;
            if raw.len() != (width * height * u32::from(channels)) as usize {
                return None;
            }
            match channels {
                1 => DynamicImage::ImageLuma8(GrayImage::from_raw(width, height, raw)?),
                _ => DynamicImage::ImageRgb8(RgbImage::from_raw(width, height, raw)?),
            }
        }
        [] if profile.recode_lossless => {
            let raw = &stream.content;
            if raw.len() != (width * height * u32::from(channels)) as usize {
                return None;
            }
            match channels {
                1 => DynamicImage::ImageLuma8(GrayImage::from_raw(width, height, raw.clone())?),
                _ => DynamicImage::ImageRgb8(RgbImage::from_raw(width, height, raw.clone())?),
            }
        }
        _ => return None,
    };

    let image = if width.max(height) > profile.max_side {
        image.resize(profile.max_side, profile.max_side, FilterType::Triangle)
    } else {
        image
    };
    let image = match channels {
        1 => DynamicImage::ImageLuma8(image.to_luma8()),
        _ => DynamicImage::ImageRgb8(image.to_rgb8()),
    };

    let mut encoded = Vec::new();
    JpegEncoder::new_with_quality(Cursor::new(&mut encoded), profile.jpeg_quality)
        .encode_image(&image)
        .ok()?;
    // Only keep the new version when it is clearly smaller.
    if encoded.len() as f64 > stream.content.len() as f64 * 0.9 {
        return None;
    }

    let mut dict = stream.dict.clone();
    dict.set("Width", i64::from(image.width()));
    dict.set("Height", i64::from(image.height()));
    dict.set("BitsPerComponent", 8);
    dict.set("Filter", "DCTDecode");
    dict.remove(b"DecodeParms");
    let mut stream = Stream::new(dict, encoded);
    stream.allows_compression = false;
    Some(stream)
}
