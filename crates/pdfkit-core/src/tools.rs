//! The one entry point the app calls: a tool id, input paths and the options form as JSON.

use std::fs;
use std::path::{Path, PathBuf};

use lopdf::Document;
use serde::Serialize;
use serde_json::Value;

use crate::compress::{compress, Level};
use crate::doc::{file_name, output_dir, output_file};
use crate::error::{fail, Result};
use crate::images::{images_to_pdf, Orient, PageSize};
use crate::organize::{keep_pages, merge, remove_pages, repair, rotate, split, SplitMode};
use crate::render::{pdf_to_images, ImageKind};
use crate::security::{protect, unlock};
use crate::stamp::{page_numbers, watermark, PageNumbers, Watermark};

#[derive(Debug, Serialize)]
pub struct ToolResult {
    pub outputs: Vec<String>,
    /// Total size of the inputs, in bytes.
    pub before: u64,
    /// Total size of the outputs, in bytes.
    pub after: u64,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct FileInfo {
    pub path: String,
    pub size: Option<u64>,
    pub pages: Option<u32>,
    pub encrypted: bool,
}

pub fn inspect_files(paths: &[String]) -> Vec<FileInfo> {
    paths
        .iter()
        .map(|path| {
            let size = fs::metadata(path).ok().map(|m| m.len());
            let is_pdf = Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
            let (pages, encrypted) = if is_pdf {
                // Files that open with an empty password list their pages; locked ones report none.
                match Document::load_metadata(path) {
                    Ok(meta) if meta.page_count > 0 => (Some(meta.page_count), false),
                    _ => (None, mentions_encryption(Path::new(path))),
                }
            } else {
                (None, false)
            };
            FileInfo { path: path.clone(), size, pages, encrypted }
        })
        .collect()
}

/// Whether the trailer names an /Encrypt dictionary. Trailers sit at the end of the file,
/// or also near the start in linearized files.
fn mentions_encryption(path: &Path) -> bool {
    let Ok(data) = fs::read(path) else { return false };
    let head = &data[..data.len().min(8 * 1024)];
    let tail = &data[data.len().saturating_sub(64 * 1024)..];
    [head, tail].iter().any(|part| part.windows(8).any(|w| w == b"/Encrypt"))
}

/// Options as sent by the form; numbers may arrive as numbers or strings.
struct Options<'a>(&'a Value);

impl Options<'_> {
    fn text(&self, key: &str) -> String {
        match self.0.get(key) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        }
    }

    fn number(&self, key: &str, default: f64) -> f64 {
        match self.0.get(key) {
            Some(Value::Number(n)) => n.as_f64().unwrap_or(default),
            Some(Value::String(s)) => s.trim().parse().unwrap_or(default),
            _ => default,
        }
    }
}

pub fn run_tool(tool: &str, inputs: &[String], options: &Value) -> Result<ToolResult> {
    let inputs: Vec<PathBuf> = inputs.iter().map(PathBuf::from).collect();
    if inputs.is_empty() {
        return fail("Pilih file dulu.");
    }
    for path in &inputs {
        if !path.is_file() {
            return fail(format!("File \"{}\" tidak ditemukan.", path.display()));
        }
    }
    let before: u64 = inputs.iter().filter_map(|p| fs::metadata(p).ok()).map(|m| m.len()).sum();
    let opts = Options(options);
    let first = &inputs[0];

    // Tools that turn every input into one output file next to it.
    let each = |suffix: &str, op: &dyn Fn(&Path, &Path) -> Result<u64>| -> Result<(Vec<PathBuf>, u64)> {
        let mut outputs = Vec::new();
        let mut after = 0;
        for input in &inputs {
            let output = output_file(input, suffix, "pdf");
            after += op(input, &output)?;
            outputs.push(output);
        }
        Ok((outputs, after))
    };
    let count = inputs.len();

    let (outputs, after, message) = match tool {
        "merge" => {
            if count < 2 {
                return fail("Pilih minimal 2 PDF untuk digabung.");
            }
            let output = output_file(first, "gabung", "pdf");
            let after = merge(&inputs, &output)?;
            (vec![output], after, format!("{count} PDF digabung menjadi satu file."))
        }
        "split" => {
            let ranges = opts.text("ranges");
            let mode = match opts.text("mode").as_str() {
                "every" => SplitMode::Every(opts.number("every", 1.0).max(1.0) as u32),
                "all" => SplitMode::All,
                _ => SplitMode::Ranges(&ranges),
            };
            let dir = output_dir(first, "pisah")?;
            let outputs = split(first, mode, &dir)?;
            let message = format!("PDF dipisah menjadi {} file di folder \"{}\".", outputs.len(), file_name(&dir));
            (outputs, 0, message)
        }
        "compress" => {
            let level = match opts.text("level").as_str() {
                "low" => Level::Low,
                "high" => Level::High,
                _ => Level::Medium,
            };
            let mut outputs = Vec::new();
            let (mut total_before, mut total_after) = (0, 0);
            for input in &inputs {
                let output = output_file(input, "kompres", "pdf");
                let (b, a) = compress(input, &output, level)?;
                total_before += b;
                total_after += a;
                outputs.push(output);
            }
            let message = if total_after < total_before {
                "PDF berhasil dikompres.".to_string()
            } else {
                "PDF sudah optimal; ukurannya tidak bisa diperkecil lagi tanpa merusak isi.".to_string()
            };
            return Ok(done(outputs, total_before, total_after, message));
        }
        "rotate" => {
            let angle = opts.number("angle", 90.0) as i64;
            let pages = opts.text("pages");
            let (outputs, after) = each("putar", &|i, o| rotate(i, o, angle, &pages))?;
            (outputs, after, format!("{count} PDF diputar {angle}°."))
        }
        "remove" => {
            let pages = opts.text("pages");
            let output = output_file(first, "hapus-halaman", "pdf");
            let after = remove_pages(first, &output, &pages)?;
            (vec![output], after, "Halaman yang dipilih sudah dihapus.".into())
        }
        "extract" => {
            let pages = opts.text("pages");
            let output = output_file(first, "ambil", "pdf");
            let after = keep_pages(first, &output, &pages)?;
            (vec![output], after, "Halaman yang dipilih disalin ke PDF baru.".into())
        }
        "reorder" => {
            let order = opts.text("order");
            let output = output_file(first, "urut", "pdf");
            let after = keep_pages(first, &output, &order)?;
            (vec![output], after, "Halaman sudah disusun ulang.".into())
        }
        "pdf_to_image" => {
            let kind = if opts.text("format") == "png" { ImageKind::Png } else { ImageKind::Jpg };
            let dpi = opts.number("dpi", 150.0).clamp(36.0, 600.0) as f32;
            let dir = output_dir(first, "gambar")?;
            let outputs = pdf_to_images(first, &dir, &opts.text("pages"), dpi, kind)?;
            let message = format!("{} halaman diubah menjadi gambar di folder \"{}\".", outputs.len(), file_name(&dir));
            (outputs, 0, message)
        }
        "image_to_pdf" => {
            let size = match opts.text("page_size").as_str() {
                "a4" => PageSize::A4,
                "letter" => PageSize::Letter,
                _ => PageSize::Fit,
            };
            let orient = match opts.text("orientation").as_str() {
                "portrait" => Orient::Portrait,
                "landscape" => Orient::Landscape,
                _ => Orient::Auto,
            };
            let margin = opts.number("margin", 0.0).clamp(0.0, 144.0) as f32;
            let output = output_file(first, "gambar", "pdf");
            let after = images_to_pdf(&inputs, &output, size, orient, margin)?;
            (vec![output], after, format!("{count} gambar diubah menjadi PDF."))
        }
        "watermark" => {
            let text = opts.text("text");
            if text.trim().is_empty() {
                return fail("Isi teks watermark-nya dulu.");
            }
            let mark = Watermark {
                text: &text,
                font_size: opts.number("font_size", 60.0).clamp(6.0, 300.0) as f32,
                opacity: (opts.number("opacity", 30.0) / 100.0).clamp(0.05, 1.0) as f32,
                rotation: opts.number("rotation", 45.0) as f32,
                color: match opts.text("color").as_str() {
                    "gray" => [0.5, 0.5, 0.5],
                    "black" => [0.0, 0.0, 0.0],
                    "blue" => [0.1, 0.3, 0.8],
                    _ => [0.85, 0.1, 0.1],
                },
            };
            let pages = opts.text("pages");
            let (outputs, after) = each("watermark", &|i, o| watermark(i, o, &pages, &mark))?;
            (outputs, after, "Watermark sudah ditambahkan.".into())
        }
        "page_numbers" => {
            let format = opts.text("format");
            let position = opts.text("position");
            let numbers = PageNumbers {
                format: if format.is_empty() { "{n}" } else { &format },
                position: if position.is_empty() { "bottom-center" } else { &position },
                start: opts.number("start", 1.0).max(0.0) as u32,
                font_size: opts.number("font_size", 11.0).clamp(6.0, 72.0) as f32,
            };
            let pages = opts.text("pages");
            let (outputs, after) = each("bernomor", &|i, o| page_numbers(i, o, &pages, &numbers))?;
            (outputs, after, "Nomor halaman sudah ditambahkan.".into())
        }
        "protect" => {
            let password = opts.text("password");
            let (outputs, after) = each("terkunci", &|i, o| protect(i, o, &password))?;
            (outputs, after, "PDF sudah dikunci dengan password (AES-256). Jangan sampai lupa password-nya!".into())
        }
        "unlock" => {
            let password = opts.text("password");
            let (outputs, after) = each("terbuka", &|i, o| unlock(i, o, &password))?;
            (outputs, after, "Password sudah dihapus dari PDF.".into())
        }
        "repair" => {
            let (outputs, after) = each("diperbaiki", &|i, o| repair(i, o))?;
            (outputs, after, "PDF sudah ditulis ulang dari bagian yang bisa dipulihkan.".into())
        }
        other => return fail(format!("Alat \"{other}\" tidak dikenal.")),
    };

    let after = if after == 0 {
        outputs.iter().filter_map(|p| fs::metadata(p).ok()).map(|m| m.len()).sum()
    } else {
        after
    };
    Ok(done(outputs, before, after, message))
}

fn done(outputs: Vec<PathBuf>, before: u64, after: u64, message: String) -> ToolResult {
    ToolResult {
        outputs: outputs.into_iter().map(|p| p.to_string_lossy().into_owned()).collect(),
        before,
        after,
        message,
    }
}
