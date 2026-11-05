//! End-to-end checks: every tool runs on generated PDFs and its output is read back.

use std::path::{Path, PathBuf};

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdfkit_core::{inspect_files, run_tool, ToolResult};
use serde_json::{json, Value};
use tempfile::TempDir;

/// A Letter-size PDF whose pages say "Page N", optionally with a photo-like image on each.
fn sample_pdf(dir: &Path, name: &str, pages: u32, image: Option<(u32, u32)>) -> String {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
    });
    let image_id = image.map(|(w, h)| {
        let mut stream = Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64,
                "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
            },
            photo_pixels(w, h),
        );
        stream.compress().unwrap();
        doc.add_object(stream)
    });

    let mut kids = Vec::new();
    for n in 1..=pages {
        let mut ops = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 24.into()]),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new("Tj", vec![Object::string_literal(format!("Page {n}"))]),
            Operation::new("ET", vec![]),
        ];
        if image_id.is_some() {
            ops.extend([
                Operation::new("q", vec![]),
                Operation::new("cm", vec![400.into(), 0.into(), 0.into(), 300.into(), 72.into(), 300.into()]),
                Operation::new("Do", vec!["Im0".into()]),
                Operation::new("Q", vec![]),
            ]);
        }
        let content = doc.add_object(Stream::new(dictionary! {}, Content { operations: ops }.encode().unwrap()));
        let mut resources = dictionary! { "Font" => dictionary! { "F1" => font } };
        if let Some(id) = image_id {
            resources.set("XObject", dictionary! { "Im0" => id });
        }
        kids.push(Object::Reference(doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => content, "Resources" => resources,
        })));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Count" => pages as i64, "Kids" => kids,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let path = dir.join(name);
    doc.save(&path).unwrap();
    path.to_string_lossy().into_owned()
}

/// Smooth gradients plus grain, so it behaves like a photo under compression.
fn photo_pixels(w: u32, h: u32) -> Vec<u8> {
    let mut seed = 0x2545_f491_u32;
    let mut data = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let grain = (seed >> 24) as i32 % 24 - 12;
            for base in [x * 255 / w, y * 255 / h, (x + y) * 127 / (w + h) + 64] {
                data.push((base as i32 + grain).clamp(0, 255) as u8);
            }
        }
    }
    data
}

fn run(tool: &str, inputs: &[&String], options: Value) -> ToolResult {
    let inputs: Vec<String> = inputs.iter().map(|s| s.to_string()).collect();
    run_tool(tool, &inputs, &options).unwrap_or_else(|e| panic!("{tool} failed: {e}"))
}

fn page_count(path: &str) -> usize {
    Document::load(path).unwrap().get_pages().len()
}

fn page_text(path: &str, page: u32) -> String {
    Document::load(path).unwrap().extract_text(&[page]).unwrap()
}

#[test]
fn merges_in_order_with_bookmarks() {
    let dir = TempDir::new().unwrap();
    let a = sample_pdf(dir.path(), "a.pdf", 2, None);
    let b = sample_pdf(dir.path(), "b.pdf", 3, None);
    let res = run("merge", &[&b, &a], json!({}));
    let out = &res.outputs[0];
    assert!(out.ends_with("b_gabung.pdf"));
    assert_eq!(page_count(out), 5);
    assert!(page_text(out, 1).contains("Page 1"));
    assert!(page_text(out, 4).contains("Page 1"));
    assert!(page_text(out, 5).contains("Page 2"));
    let doc = Document::load(out).unwrap();
    assert!(doc.catalog().unwrap().has(b"Outlines"));
}

#[test]
fn splits_by_ranges_chunks_and_pages() {
    let dir = TempDir::new().unwrap();
    let src = sample_pdf(dir.path(), "doc.pdf", 5, None);

    let res = run("split", &[&src], json!({ "mode": "ranges", "ranges": "1-2, 5" }));
    assert_eq!(res.outputs.len(), 2);
    assert_eq!(page_count(&res.outputs[0]), 2);
    assert!(page_text(&res.outputs[1], 1).contains("Page 5"));

    let res = run("split", &[&src], json!({ "mode": "every", "every": 2 }));
    assert_eq!(res.outputs.iter().map(|o| page_count(o)).collect::<Vec<_>>(), vec![2, 2, 1]);

    let res = run("split", &[&src], json!({ "mode": "all" }));
    assert_eq!(res.outputs.len(), 5);
}

#[test]
fn compresses_images_and_downsamples() {
    let dir = TempDir::new().unwrap();
    let src = sample_pdf(dir.path(), "photo.pdf", 2, Some((2400, 1600)));
    let res = run("compress", &[&src], json!({ "level": "medium" }));
    assert!(res.after < res.before / 2, "{} -> {}", res.before, res.after);

    let doc = Document::load(&res.outputs[0]).unwrap();
    assert_eq!(doc.get_pages().len(), 2);
    let widths: Vec<i64> = doc
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter(|s| s.dict.get(b"Subtype").and_then(Object::as_name).is_ok_and(|n| n == b"Image"))
        .map(|s| s.dict.get(b"Width").unwrap().as_i64().unwrap())
        .collect();
    assert_eq!(widths, vec![1600]);

    // The compressed file must still render.
    let res = run("pdf_to_image", &[&res.outputs[0]], json!({ "format": "png", "dpi": 36 }));
    assert_eq!(res.outputs.len(), 2);
}

#[test]
fn compress_never_grows_a_file() {
    let dir = TempDir::new().unwrap();
    let src = sample_pdf(dir.path(), "text.pdf", 1, None);
    let res = run("compress", &[&src], json!({ "level": "high" }));
    assert!(res.after <= res.before);
    assert_eq!(page_count(&res.outputs[0]), 1);
}

#[test]
fn rotates_selected_pages() {
    let dir = TempDir::new().unwrap();
    let src = sample_pdf(dir.path(), "doc.pdf", 2, None);
    let res = run("rotate", &[&src], json!({ "angle": "270", "pages": "1" }));
    let doc = Document::load(&res.outputs[0]).unwrap();
    let pages = doc.get_pages();
    let rotation = |n: u32| doc.get_dictionary(pages[&n]).unwrap().get(b"Rotate").and_then(Object::as_i64).unwrap_or(0);
    assert_eq!((rotation(1), rotation(2)), (270, 0));
}

#[test]
fn removes_extracts_and_reorders_pages() {
    let dir = TempDir::new().unwrap();
    let src = sample_pdf(dir.path(), "doc.pdf", 4, None);

    let removed = run("remove", &[&src], json!({ "pages": "2-3" }));
    assert_eq!(page_count(&removed.outputs[0]), 2);
    assert!(page_text(&removed.outputs[0], 2).contains("Page 4"));
    assert!(run_tool("remove", std::slice::from_ref(&src), &json!({ "pages": "1-4" })).is_err());

    let extracted = run("extract", &[&src], json!({ "pages": "3" }));
    assert_eq!(page_count(&extracted.outputs[0]), 1);

    let reordered = run("reorder", &[&src], json!({ "order": "4-1" }));
    assert_eq!(page_count(&reordered.outputs[0]), 4);
    assert!(page_text(&reordered.outputs[0], 1).contains("Page 4"));
    assert!(page_text(&reordered.outputs[0], 4).contains("Page 1"));
}

#[test]
fn renders_pages_to_images() {
    let dir = TempDir::new().unwrap();
    let src = sample_pdf(dir.path(), "doc.pdf", 3, Some((300, 200)));
    let res = run("pdf_to_image", &[&src], json!({ "format": "jpg", "dpi": "72", "pages": "2-3" }));
    assert_eq!(res.outputs.len(), 2);
    assert!(res.outputs[0].ends_with("doc_hal-002.jpg"));
    let img = image::open(&res.outputs[0]).unwrap();
    assert_eq!((img.width(), img.height()), (612, 792));
}

#[test]
fn turns_images_into_pages() {
    let dir = TempDir::new().unwrap();
    let png: PathBuf = dir.path().join("a.png");
    let jpg: PathBuf = dir.path().join("b.jpg");
    image::RgbaImage::from_pixel(200, 100, image::Rgba([10, 200, 30, 128])).save(&png).unwrap();
    image::RgbImage::from_raw(300, 400, photo_pixels(300, 400)).unwrap().save(&jpg).unwrap();
    let (png, jpg) = (png.to_string_lossy().into_owned(), jpg.to_string_lossy().into_owned());

    let res = run("image_to_pdf", &[&png, &jpg], json!({ "page_size": "a4", "orientation": "auto", "margin": "18" }));
    let doc = Document::load(&res.outputs[0]).unwrap();
    let pages = doc.get_pages();
    assert_eq!(pages.len(), 2);
    let width = |n: u32| doc.get_dictionary(pages[&n]).unwrap().get(b"MediaBox").unwrap().as_array().unwrap()[2].as_float().unwrap();
    // The wide PNG gets a landscape page, the tall JPEG a portrait one.
    assert!(width(1) > 800.0 && width(2) < 600.0);
    let jpeg_kept = doc.objects.values().filter_map(|o| o.as_stream().ok()).any(|s| {
        s.dict.get(b"Filter").and_then(Object::as_name).is_ok_and(|f| f == b"DCTDecode")
    });
    assert!(jpeg_kept);

    let rendered = run("pdf_to_image", &[&res.outputs[0]], json!({ "format": "png", "dpi": 36 }));
    assert_eq!(rendered.outputs.len(), 2);
}

#[test]
fn stamps_watermarks_and_page_numbers() {
    let dir = TempDir::new().unwrap();
    let src = sample_pdf(dir.path(), "doc.pdf", 3, None);

    let marked = run("watermark", &[&src], json!({ "text": "RAHASIA", "font_size": 50, "opacity": 30, "rotation": "45", "color": "red" }));
    assert!(page_text(&marked.outputs[0], 2).contains("RAHASIA"));
    assert!(page_text(&marked.outputs[0], 2).contains("Page 2"));

    let numbered = run("page_numbers", &[&marked.outputs[0]], json!({ "format": "{n} / {total}", "position": "bottom-right", "start": 1, "pages": "" }));
    assert!(page_text(&numbered.outputs[0], 3).contains("3 / 3"));

    let rendered = run("pdf_to_image", &[&numbered.outputs[0]], json!({ "format": "png", "dpi": 36 }));
    assert_eq!(rendered.outputs.len(), 3);
}

#[test]
fn locks_and_unlocks_with_a_password() {
    let dir = TempDir::new().unwrap();
    let src = sample_pdf(dir.path(), "doc.pdf", 2, None);

    let locked = run("protect", &[&src], json!({ "password": "rahasia123" }));
    let out = &locked.outputs[0];
    assert!(inspect_files(std::slice::from_ref(out))[0].encrypted);
    assert!(run_tool("merge", &[out.clone(), src.clone()], &json!({})).is_err());

    let wrong = run_tool("unlock", std::slice::from_ref(out), &json!({ "password": "salah" })).unwrap_err();
    assert_eq!(wrong.to_string(), "Password salah.");

    let unlocked = run("unlock", &[out], json!({ "password": "rahasia123" }));
    assert!(!inspect_files(&[unlocked.outputs[0].clone()])[0].encrypted);
    assert!(page_text(&unlocked.outputs[0], 2).contains("Page 2"));
}

#[test]
fn repairs_and_inspects() {
    let dir = TempDir::new().unwrap();
    let src = sample_pdf(dir.path(), "doc.pdf", 2, None);
    let res = run("repair", &[&src], json!({}));
    assert_eq!(page_count(&res.outputs[0]), 2);

    let info = &inspect_files(std::slice::from_ref(&src))[0];
    assert_eq!(info.pages, Some(2));
    assert!(!info.encrypted);
}

#[test]
fn never_overwrites_outputs() {
    let dir = TempDir::new().unwrap();
    let src = sample_pdf(dir.path(), "doc.pdf", 1, None);
    let first = run("repair", &[&src], json!({}));
    let second = run("repair", &[&src], json!({}));
    assert!(first.outputs[0].ends_with("doc_diperbaiki.pdf"));
    assert!(second.outputs[0].ends_with("doc_diperbaiki (2).pdf"));
}
