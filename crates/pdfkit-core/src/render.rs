//! PDF pages to JPG or PNG, rasterized in pure Rust by `hayro`.

use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::{LoadPdfError, Pdf};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{render, PixmapSettings, RenderCache, RenderSettings};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat, RgbaImage};

use crate::doc::{file_name, file_stem, unique_path};
use crate::error::{fail, Result};
use crate::pages::parse_page_set;

/// Rendered pages are capped to this many pixels on their longest side.
const MAX_SIDE: f32 = 10_000.0;

#[derive(Clone, Copy)]
pub(crate) enum ImageKind {
    Jpg,
    Png,
}

pub(crate) fn pdf_to_images(
    input: &Path,
    out_dir: &Path,
    spec: &str,
    dpi: f32,
    kind: ImageKind,
) -> Result<Vec<PathBuf>> {
    let pdf = match Pdf::new(fs::read(input)?) {
        Ok(pdf) => pdf,
        Err(LoadPdfError::Decryption(_)) => {
            return fail(format!(
                "\"{}\" terkunci password. Buka dulu dengan alat \"Buka kunci PDF\".",
                file_name(input)
            ))
        }
        Err(_) => return fail(format!("\"{}\" tidak bisa dibaca sebagai PDF.", file_name(input))),
    };
    let pages = pdf.pages();
    let wanted = parse_page_set(spec, pages.len() as u32)?;

    let settings = InterpreterSettings::default();
    let cache = RenderCache::new();
    let stem = file_stem(input);
    let ext = match kind {
        ImageKind::Jpg => "jpg",
        ImageKind::Png => "png",
    };

    let mut outputs = Vec::with_capacity(wanted.len());
    for (index, page) in pages.iter().enumerate() {
        let number = index as u32 + 1;
        if !wanted.contains(&number) {
            continue;
        }
        let (w, h) = page.render_dimensions();
        let scale = (dpi / 72.0).min(MAX_SIDE / w.max(h).max(1.0));
        let pixmap = render(
            page,
            &cache,
            &settings,
            &RenderSettings::default(),
            &PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE },
        );
        let (pw, ph) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
        // The background is opaque white, so the premultiplied pixels equal plain RGBA.
        let Some(rgba) = RgbaImage::from_raw(pw, ph, pixmap.data_as_u8_slice().to_vec()) else {
            return fail(format!("Halaman {number} gagal dirender."));
        };
        let rgb = DynamicImage::ImageRgba8(rgba).to_rgb8();

        let path = unique_path(out_dir, &format!("{stem}_hal-{number:03}"), Some(ext));
        let mut out = BufWriter::new(fs::File::create(&path)?);
        match kind {
            ImageKind::Jpg => JpegEncoder::new_with_quality(&mut out, 90).encode_image(&rgb)?,
            ImageKind::Png => rgb.write_to(&mut out, ImageFormat::Png)?,
        }
        out.flush()?;
        outputs.push(path);
    }
    Ok(outputs)
}
