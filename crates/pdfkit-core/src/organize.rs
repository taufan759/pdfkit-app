//! Page-level tools: merge, split, extract, remove, reorder, rotate and repair.

use std::path::{Path, PathBuf};

use lopdf::{dictionary, Bookmark, Dictionary, Document, ObjectId};

use crate::doc::{file_stem, inherited, load, materialize_pages, resolve_dict, save, set_pages, unique_path};
use crate::error::{fail, Result};
use crate::pages::{parse_page_list, parse_page_set, parse_ranges};

pub(crate) enum SplitMode<'a> {
    Ranges(&'a str),
    Every(u32),
    All,
}

pub(crate) fn merge(inputs: &[PathBuf], output: &Path) -> Result<u64> {
    let mut merged = Document::with_version("1.7");
    let mut pages = Vec::new();
    let mut bookmarks = Vec::new();
    let mut fields = Vec::new();
    let mut form: Option<Dictionary> = None;
    let mut next_id = 1;

    for path in inputs {
        let mut doc = load(path)?;
        materialize_pages(&mut doc);
        doc.renumber_objects_with(next_id);
        next_id = doc.max_id + 1;

        let doc_pages: Vec<ObjectId> = doc.page_iter().collect();
        if let Some(&first) = doc_pages.first() {
            bookmarks.push((file_stem(path), first));
        }
        pages.extend(doc_pages);

        // Keep form fields working by gathering every document's fields into one AcroForm.
        let acro_form = doc.catalog().ok().and_then(|c| c.get(b"AcroForm").ok().cloned());
        if let Some(acro_form) = acro_form {
            let acro_form = resolve_dict(&doc, Some(&acro_form));
            let list = acro_form
                .get(b"Fields")
                .ok()
                .and_then(|f| doc.dereference(f).ok())
                .and_then(|(_, o)| o.as_array().ok().cloned());
            fields.extend(list.unwrap_or_default());
            form.get_or_insert(acro_form);
        }
        merged.objects.extend(doc.objects);
    }

    merged.max_id = next_id - 1;
    let catalog_id = merged.add_object(dictionary! { "Type" => "Catalog" });
    merged.trailer.set("Root", catalog_id);
    if let Some(mut form) = form {
        form.set("Fields", fields);
        form.remove(b"XFA");
        let form_id = merged.add_object(form);
        merged.get_dictionary_mut(catalog_id)?.set("AcroForm", form_id);
    }
    set_pages(&mut merged, &pages)?;

    for (title, page) in bookmarks {
        merged.add_bookmark(Bookmark::new(title, [0.0; 3], 0, page), None);
    }
    if let Some(outline) = merged.build_outline() {
        let catalog = merged.get_dictionary_mut(catalog_id)?;
        catalog.set("Outlines", outline);
        catalog.set("PageMode", "UseOutlines");
    }
    save(&mut merged, output)
}

pub(crate) fn split(input: &Path, mode: SplitMode, out_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut source = load(input)?;
    materialize_pages(&mut source);
    let all = source.get_pages();
    let total = all.len() as u32;
    let groups: Vec<Vec<u32>> = match mode {
        SplitMode::Ranges(spec) => parse_ranges(spec, total)?,
        SplitMode::Every(n) => (1..=total).collect::<Vec<_>>().chunks(n.max(1) as usize).map(<[u32]>::to_vec).collect(),
        SplitMode::All => (1..=total).map(|n| vec![n]).collect(),
    };

    let stem = file_stem(input);
    let mut outputs = Vec::with_capacity(groups.len());
    for group in &groups {
        let mut doc = source.clone();
        let ids: Vec<ObjectId> = group.iter().map(|n| all[n]).collect();
        set_pages(&mut doc, &ids)?;
        let path = unique_path(out_dir, &format!("{stem}_hal-{}", label(group)), Some("pdf"));
        save(&mut doc, &path)?;
        outputs.push(path);
    }
    Ok(outputs)
}

/// "3" or "3-7" for a run of pages, otherwise the first and last page.
fn label(group: &[u32]) -> String {
    let (first, last) = (group[0], group[group.len() - 1]);
    let contiguous = group.windows(2).all(|w| w[1] == w[0] + 1);
    match (group.len(), contiguous) {
        (1, _) => first.to_string(),
        (_, true) => format!("{first}-{last}"),
        _ => format!("{first}-dst"),
    }
}

/// Keeps the pages of `spec` in the order written (extract and reorder).
pub(crate) fn keep_pages(input: &Path, output: &Path, spec: &str) -> Result<u64> {
    let mut doc = load(input)?;
    materialize_pages(&mut doc);
    let all = doc.get_pages();
    let ids: Vec<ObjectId> = parse_page_list(spec, all.len() as u32)?.iter().map(|n| all[n]).collect();
    set_pages(&mut doc, &ids)?;
    save(&mut doc, output)
}

pub(crate) fn remove_pages(input: &Path, output: &Path, spec: &str) -> Result<u64> {
    let mut doc = load(input)?;
    materialize_pages(&mut doc);
    let all = doc.get_pages();
    let removed = parse_page_set(spec, all.len() as u32)?;
    let ids: Vec<ObjectId> = all.iter().filter(|(n, _)| !removed.contains(n)).map(|(_, id)| *id).collect();
    if ids.is_empty() {
        return fail("Tidak bisa menghapus semua halaman; sisakan minimal satu.");
    }
    set_pages(&mut doc, &ids)?;
    save(&mut doc, output)
}

pub(crate) fn rotate(input: &Path, output: &Path, angle: i64, spec: &str) -> Result<u64> {
    let mut doc = load(input)?;
    let all = doc.get_pages();
    for n in parse_page_set(spec, all.len() as u32)? {
        let id = all[&n];
        let current = inherited(&doc, id, b"Rotate").and_then(|o| o.as_i64().ok()).unwrap_or(0);
        doc.get_dictionary_mut(id)?.set("Rotate", (current + angle).rem_euclid(360));
    }
    save(&mut doc, output)
}

/// Rewrites the file from what could be recovered: a fresh page tree, live objects only.
pub(crate) fn repair(input: &Path, output: &Path) -> Result<u64> {
    let mut doc = load(input)?;
    materialize_pages(&mut doc);
    let pages: Vec<ObjectId> = doc.page_iter().collect();
    if pages.is_empty() {
        return fail("Tidak ada halaman yang bisa dipulihkan dari PDF ini.");
    }
    set_pages(&mut doc, &pages)?;
    doc.renumber_objects();
    save(&mut doc, output)
}

#[cfg(test)]
mod tests {
    use super::label;

    #[test]
    fn labels_groups() {
        assert_eq!(label(&[4]), "4");
        assert_eq!(label(&[2, 3, 4]), "2-4");
        assert_eq!(label(&[5, 1]), "5-dst");
    }
}
