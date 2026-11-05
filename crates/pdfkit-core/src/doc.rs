//! Loading, page-tree surgery and saving shared by the tools.

use std::collections::HashSet;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, SaveOptions};

use crate::error::{fail, Result};

const INHERITABLE: [&str; 4] = ["MediaBox", "CropBox", "Resources", "Rotate"];

pub(crate) fn load(path: &Path) -> Result<Document> {
    let locked = || {
        fail(format!(
            "\"{}\" terkunci password. Buka dulu dengan alat \"Buka kunci PDF\".",
            file_name(path)
        ))
    };
    match Document::load(path) {
        Ok(doc) if doc.is_encrypted() => locked(),
        Ok(doc) => Ok(doc),
        Err(lopdf::Error::InvalidPassword | lopdf::Error::Decryption(_)) => locked(),
        Err(e) => fail(format!("\"{}\" tidak bisa dibaca sebagai PDF ({e}).", file_name(path))),
    }
}

pub(crate) fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub(crate) fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "dokumen".into())
}

/// Looks up a page attribute, walking up the page tree for inherited values.
pub(crate) fn inherited(doc: &Document, page_id: ObjectId, key: &[u8]) -> Option<Object> {
    let mut current = page_id;
    let mut seen = HashSet::new();
    while seen.insert(current) {
        let node = doc.get_dictionary(current).ok()?;
        if let Ok(value) = node.get(key) {
            return Some(value.clone());
        }
        current = node.get(b"Parent").and_then(Object::as_reference).ok()?;
    }
    None
}

/// Resolves a dictionary that may be stored inline or behind a reference.
pub(crate) fn resolve_dict(doc: &Document, object: Option<&Object>) -> Dictionary {
    match object {
        Some(Object::Reference(id)) => doc.get_dictionary(*id).cloned().unwrap_or_default(),
        Some(Object::Dictionary(dict)) => dict.clone(),
        _ => Dictionary::new(),
    }
}

/// Copies inherited attributes onto every page so pages can leave their original tree.
pub(crate) fn materialize_pages(doc: &mut Document) {
    for page_id in doc.page_iter().collect::<Vec<_>>() {
        let values: Vec<_> = INHERITABLE
            .iter()
            .filter_map(|key| inherited(doc, page_id, key.as_bytes()).map(|v| (*key, v)))
            .collect();
        if let Ok(page) = doc.get_dictionary_mut(page_id) {
            for (key, value) in values {
                if !page.has(key.as_bytes()) {
                    page.set(key, value);
                }
            }
        }
    }
}

/// Replaces the page tree with a flat one holding `pages` in order; a page listed twice is copied.
///
/// Pages must already be materialized. Outlines, the structure tree and page labels point at
/// particular pages, so they are dropped because the page set changed.
pub(crate) fn set_pages(doc: &mut Document, pages: &[ObjectId]) -> Result<()> {
    let tree_id = doc.new_object_id();
    let mut kids = Vec::with_capacity(pages.len());
    let mut used = HashSet::new();
    for &page in pages {
        let id = if used.insert(page) {
            page
        } else {
            let copy = doc.get_dictionary(page)?.clone();
            doc.add_object(copy)
        };
        doc.get_dictionary_mut(id)?.set("Parent", tree_id);
        kids.push(Object::Reference(id));
    }
    let count = kids.len() as i64;
    doc.objects.insert(
        tree_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Count" => count, "Kids" => kids }),
    );
    let catalog = doc.catalog_mut()?;
    catalog.set("Pages", tree_id);
    for key in ["Outlines", "StructTreeRoot", "MarkInfo", "PageLabels", "OpenAction"] {
        catalog.remove(key.as_bytes());
    }
    doc.prune_objects();
    Ok(())
}

/// Saves with classic cross-reference tables, which every reader understands.
pub(crate) fn save(doc: &mut Document, path: &Path) -> Result<u64> {
    if !doc.is_encrypted() {
        doc.compress();
    }
    write_atomically(path, |out| doc.save_to(out))
}

/// Saves with object and cross-reference streams for the smallest file.
pub(crate) fn save_compact(doc: &mut Document, path: &Path) -> Result<u64> {
    doc.compress();
    let options = SaveOptions::builder()
        .use_object_streams(true)
        .use_xref_streams(true)
        .compression_level(9)
        .build();
    write_atomically(path, |out| doc.save_with_options(out, options))
}

fn write_atomically(
    path: &Path,
    write: impl FnOnce(&mut BufWriter<fs::File>) -> std::io::Result<()>,
) -> Result<u64> {
    let tmp = path.with_extension("pdfkit-tmp");
    let result = (|| -> Result<()> {
        let mut out = BufWriter::new(fs::File::create(&tmp)?);
        write(&mut out)?;
        out.flush()?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, path)?;
    Ok(fs::metadata(path)?.len())
}

/// `<dir>/<stem>_<suffix>.<ext>` next to the input, never overwriting an existing file.
pub(crate) fn output_file(input: &Path, suffix: &str, ext: &str) -> PathBuf {
    let dir = input.parent().unwrap_or(Path::new("."));
    unique_path(dir, &format!("{}_{suffix}", file_stem(input)), Some(ext))
}

/// A new folder next to the input for tools that write many files.
pub(crate) fn output_dir(input: &Path, suffix: &str) -> Result<PathBuf> {
    let dir = input.parent().unwrap_or(Path::new("."));
    let path = unique_path(dir, &format!("{}_{suffix}", file_stem(input)), None);
    fs::create_dir_all(&path)?;
    Ok(path)
}

pub(crate) fn unique_path(dir: &Path, base: &str, ext: Option<&str>) -> PathBuf {
    let name = |n: u32| {
        let base = if n == 1 { base.to_string() } else { format!("{base} ({n})") };
        match ext {
            Some(ext) => format!("{base}.{ext}"),
            None => base,
        }
    };
    (1..)
        .map(|n| dir.join(name(n)))
        .find(|p| !p.exists())
        .expect("an unused file name")
}
