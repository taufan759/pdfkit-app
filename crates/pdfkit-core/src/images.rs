//! Images to PDF: one page per image, JPEGs embedded as they are whenever possible.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

use crate::doc::{file_name, save};
use crate::error::{fail, Result};

/// Pixels are placed at 96 per inch when the page follows the image size.
const POINTS_PER_PIXEL: f32 = 72.0 / 96.0;

#[derive(Clone, Copy)]
pub(crate) enum PageSize {
    Fit,
    A4,
    Letter,
}

#[derive(Clone, Copy)]
pub(crate) enum Orient {
    Auto,
    Portrait,
    Landscape,
}

pub(crate) fn images_to_pdf(
    inputs: &[PathBuf],
    output: &Path,
    size: PageSize,
    orient: Orient,
    margin: f32,
) -> Result<u64> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::with_capacity(inputs.len());

    for path in inputs {
        let (image_id, px_w, px_h) = embed_image(&mut doc, path)?;
        let (img_w, img_h) = (px_w as f32 * POINTS_PER_PIXEL, px_h as f32 * POINTS_PER_PIXEL);
        let (page_w, page_h) = match size {
            PageSize::Fit => (img_w + 2.0 * margin, img_h + 2.0 * margin),
            PageSize::A4 | PageSize::Letter => {
                let (w, h) = if matches!(size, PageSize::A4) { (595.28, 841.89) } else { (612.0, 792.0) };
                let landscape = match orient {
                    Orient::Auto => img_w > img_h,
                    Orient::Portrait => false,
                    Orient::Landscape => true,
                };
                if landscape { (h, w) } else { (w, h) }
            }
        };
        let scale = ((page_w - 2.0 * margin) / img_w).min((page_h - 2.0 * margin) / img_h);
        let (draw_w, draw_h) = (img_w * scale, img_h * scale);
        let (x, y) = ((page_w - draw_w) / 2.0, (page_h - draw_h) / 2.0);

        let content = Content {
            operations: vec![
                Operation::new("q", vec![]),
                Operation::new("cm", vec![draw_w.into(), 0.into(), 0.into(), draw_h.into(), x.into(), y.into()]),
                Operation::new("Do", vec![Object::Name(b"Im0".to_vec())]),
                Operation::new("Q", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode()?));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), page_w.into(), page_h.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image_id } },
        });
        kids.push(Object::Reference(page_id));
    }

    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    save(&mut doc, output)
}

/// Adds the image as an XObject and returns its id with the pixel size.
fn embed_image(doc: &mut Document, path: &Path) -> Result<(ObjectId, u32, u32)> {
    let bytes = fs::read(path)?;
    let unreadable = || format!("\"{}\" bukan gambar yang bisa dibaca.", file_name(path));
    let Ok(format) = image::guess_format(&bytes) else {
        return fail(unreadable());
    };
    let Ok(mut decoder) = ImageReader::with_format(Cursor::new(&bytes), format).into_decoder() else {
        return fail(unreadable());
    };
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let (width, height) = decoder.dimensions();

    // An upright gray or RGB JPEG can go in byte for byte: no quality loss, no growth.
    if format == ImageFormat::Jpeg && orientation == Orientation::NoTransforms {
        if let Some(space) = match jpeg_components(&bytes) {
            Some(1) => Some("DeviceGray"),
            Some(3) => Some("DeviceRGB"),
            _ => None,
        } {
            return Ok((add_image(doc, width, height, space, "DCTDecode", bytes.clone()), width, height));
        }
    }

    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let gray = image.color().channel_count() <= 2;
    let image = flatten_on_white(image);
    let (width, height) = (image.width(), image.height());

    if format == ImageFormat::Jpeg {
        let mut encoded = Vec::new();
        JpegEncoder::new_with_quality(Cursor::new(&mut encoded), 92).encode_image(&image.to_rgb8())?;
        return Ok((add_image(doc, width, height, "DeviceRGB", "DCTDecode", encoded), width, height));
    }

    // Other formats are often screenshots or line art, so keep them lossless.
    let (space, raw) = if gray {
        ("DeviceGray", image.to_luma8().into_raw())
    } else {
        ("DeviceRGB", image.to_rgb8().into_raw())
    };
    let mut stream = Stream::new(image_dict(width, height, space), raw);
    stream.compress()?;
    Ok((doc.add_object(stream), width, height))
}

fn image_dict(width: u32, height: u32, space: &str) -> lopdf::Dictionary {
    dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => i64::from(width),
        "Height" => i64::from(height),
        "ColorSpace" => space,
        "BitsPerComponent" => 8,
    }
}

fn add_image(doc: &mut Document, width: u32, height: u32, space: &str, filter: &str, data: Vec<u8>) -> ObjectId {
    let mut dict = image_dict(width, height, space);
    dict.set("Filter", filter);
    let mut stream = Stream::new(dict, data);
    stream.allows_compression = false;
    doc.add_object(stream)
}

/// Transparent pixels would turn black in a PDF without a soft mask; paint them on white.
fn flatten_on_white(image: DynamicImage) -> DynamicImage {
    if !image.color().has_alpha() {
        return image;
    }
    let mut rgba = image.to_rgba8();
    for pixel in rgba.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for c in 0..3 {
            pixel[c] = ((u16::from(pixel[c]) * alpha + 255 * (255 - alpha)) / 255) as u8;
        }
        pixel[3] = 255;
    }
    DynamicImage::ImageRgba8(rgba)
}

/// Color component count from the JPEG frame header (1 gray, 3 YCbCr, 4 CMYK).
fn jpeg_components(data: &[u8]) -> Option<u8> {
    let mut i = 2;
    while i + 9 < data.len() {
        if data[i] != 0xFF {
            return None;
        }
        let marker = data[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        let length = usize::from(u16::from_be_bytes([data[i + 2], data[i + 3]]));
        let is_frame = matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_frame {
            return data.get(i + 9).copied();
        }
        i += 2 + length;
    }
    None
}
