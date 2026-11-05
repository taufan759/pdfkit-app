//! Text drawn over pages: watermarks and page numbers, in the standard Helvetica font.

use std::path::Path;

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream, StringFormat};

use crate::doc::{inherited, load, resolve_dict, save};
use crate::error::Result;
use crate::pages::parse_page_set;

const FONT: &[u8] = b"PdfkitF";
const STATE: &[u8] = b"PdfkitGS";

pub(crate) struct Watermark<'a> {
    pub text: &'a str,
    pub font_size: f32,
    pub opacity: f32,
    pub rotation: f32,
    pub color: [f32; 3],
}

pub(crate) fn watermark(input: &Path, output: &Path, spec: &str, mark: &Watermark) -> Result<u64> {
    let mut doc = load(input)?;
    let font = add_helvetica(&mut doc);
    let state = doc.add_object(dictionary! {
        "Type" => "ExtGState",
        "ca" => mark.opacity,
        "CA" => mark.opacity,
    });
    let text = win_ansi(mark.text);
    let width = text_width(&text, mark.font_size);
    let (sin, cos) = mark.rotation.to_radians().sin_cos();
    let pages = doc.get_pages();

    for number in parse_page_set(spec, pages.len() as u32)? {
        let page = pages[&number];
        let frame = Frame::of(&doc, page);
        let [r, g, b] = mark.color;
        let ops = vec![
            Operation::new("q", vec![]),
            Operation::new("cm", frame.matrix.iter().map(|v| (*v).into()).collect()),
            Operation::new("gs", vec![Object::Name(STATE.to_vec())]),
            Operation::new("rg", vec![r.into(), g.into(), b.into()]),
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(FONT.to_vec()), mark.font_size.into()]),
            Operation::new(
                "Tm",
                vec![cos.into(), sin.into(), (-sin).into(), cos.into(), (frame.width / 2.0).into(), (frame.height / 2.0).into()],
            ),
            Operation::new("Td", vec![(-width / 2.0).into(), (-mark.font_size * 0.35).into()]),
            Operation::new("Tj", vec![Object::String(text.clone(), StringFormat::Literal)]),
            Operation::new("ET", vec![]),
            Operation::new("Q", vec![]),
        ];
        add_overlay(&mut doc, page, Content { operations: ops }.encode()?, &[(b"Font", FONT, font), (b"ExtGState", STATE, state)])?;
    }
    save(&mut doc, output)
}

pub(crate) struct PageNumbers<'a> {
    /// `{n}` is the page number, `{total}` the last number used.
    pub format: &'a str,
    /// "bottom-center", "top-right" and so on.
    pub position: &'a str,
    pub start: u32,
    pub font_size: f32,
}

pub(crate) fn page_numbers(input: &Path, output: &Path, spec: &str, numbers: &PageNumbers) -> Result<u64> {
    let mut doc = load(input)?;
    let font = add_helvetica(&mut doc);
    let pages = doc.get_pages();
    let selected = parse_page_set(spec, pages.len() as u32)?;
    let last = numbers.start + selected.len() as u32 - 1;
    let (vertical, horizontal) = numbers.position.split_once('-').unwrap_or(("bottom", "center"));
    let margin = 24.0_f32.max(numbers.font_size * 1.5);

    for (index, number) in selected.iter().enumerate() {
        let page = pages[number];
        let frame = Frame::of(&doc, page);
        let label = numbers
            .format
            .replace("{n}", &(numbers.start + index as u32).to_string())
            .replace("{total}", &last.to_string());
        let text = win_ansi(&label);
        let width = text_width(&text, numbers.font_size);
        let x = match horizontal {
            "left" => margin,
            "right" => frame.width - margin - width,
            _ => (frame.width - width) / 2.0,
        };
        let y = match vertical {
            "top" => frame.height - margin - numbers.font_size * 0.7,
            _ => margin,
        };
        let ops = vec![
            Operation::new("q", vec![]),
            Operation::new("cm", frame.matrix.iter().map(|v| (*v).into()).collect()),
            Operation::new("rg", vec![0.15.into(), 0.15.into(), 0.15.into()]),
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(FONT.to_vec()), numbers.font_size.into()]),
            Operation::new("Td", vec![x.into(), y.into()]),
            Operation::new("Tj", vec![Object::String(text, StringFormat::Literal)]),
            Operation::new("ET", vec![]),
            Operation::new("Q", vec![]),
        ];
        add_overlay(&mut doc, page, Content { operations: ops }.encode()?, &[(b"Font", FONT, font)])?;
    }
    save(&mut doc, output)
}

/// The page as the reader sees it: its visible size and the matrix from that view to user space.
struct Frame {
    width: f32,
    height: f32,
    matrix: [f32; 6],
}

impl Frame {
    fn of(doc: &Document, page: ObjectId) -> Frame {
        let rect = inherited(doc, page, b"CropBox")
            .or_else(|| inherited(doc, page, b"MediaBox"))
            .and_then(|b| rect(doc, &b))
            .unwrap_or([0.0, 0.0, 612.0, 792.0]);
        let [x0, y0, x1, y1] = rect;
        let (w, h) = (x1 - x0, y1 - y0);
        let rotate = inherited(doc, page, b"Rotate").and_then(|r| r.as_i64().ok()).unwrap_or(0).rem_euclid(360);
        match rotate {
            90 => Frame { width: h, height: w, matrix: [0.0, 1.0, -1.0, 0.0, x0 + w, y0] },
            180 => Frame { width: w, height: h, matrix: [-1.0, 0.0, 0.0, -1.0, x0 + w, y0 + h] },
            270 => Frame { width: h, height: w, matrix: [0.0, -1.0, 1.0, 0.0, x0, y0 + h] },
            _ => Frame { width: w, height: h, matrix: [1.0, 0.0, 0.0, 1.0, x0, y0] },
        }
    }
}

fn rect(doc: &Document, object: &Object) -> Option<[f32; 4]> {
    let list = doc.dereference(object).ok()?.1.as_array().ok()?;
    let mut values = list.iter().filter_map(|v| v.as_float().ok().or_else(|| v.as_i64().ok().map(|i| i as f32)));
    let (a, b, c, d) = (values.next()?, values.next()?, values.next()?, values.next()?);
    Some([a.min(c), b.min(d), a.max(c), b.max(d)])
}

fn add_helvetica(doc: &mut Document) -> ObjectId {
    doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    })
}

/// Draws `content` on top of the page. The original drawing is wrapped in q/Q so whatever
/// graphics state it leaves behind can't shift or recolor the overlay.
fn add_overlay(
    doc: &mut Document,
    page: ObjectId,
    content: Vec<u8>,
    resources: &[(&[u8], &[u8], ObjectId)],
) -> Result<()> {
    let existing: Vec<Object> = match doc.get_dictionary(page)?.get(b"Contents") {
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Array(list)) => list.clone(),
            _ => vec![Object::Reference(*id)],
        },
        Ok(Object::Array(list)) => list.clone(),
        _ => Vec::new(),
    };
    let open = doc.add_object(Stream::new(dictionary! {}, b"q\n".to_vec()));
    let close = doc.add_object(Stream::new(dictionary! {}, [b"Q\n".as_slice(), &content].concat()));
    let mut contents = vec![Object::Reference(open)];
    contents.extend(existing);
    contents.push(Object::Reference(close));

    let mut dict = resolve_dict(doc, inherited(doc, page, b"Resources").as_ref());
    for (category, name, id) in resources {
        let mut group = resolve_dict(doc, dict.get(category).ok());
        group.set(name.to_vec(), *id);
        dict.set(category.to_vec(), group);
    }

    let page = doc.get_dictionary_mut(page)?;
    page.set("Contents", contents);
    page.set("Resources", dict);
    Ok(())
}

/// Encodes text for the WinAnsi encoding; characters it lacks become '?'.
fn win_ansi(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| match c {
            ' '..='~' => c as u8,
            '\u{A0}'..='\u{FF}' => c as u32 as u8,
            '€' => 0x80,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '•' => 0x95,
            '–' => 0x96,
            '—' => 0x97,
            _ => b'?',
        })
        .collect()
}

/// Helvetica advance widths (per 1000 units of font size) for codes 32–126.
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // ' '..'/'
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // '0'..'9'
    278, 278, 584, 584, 584, 556, 1015, // ':'..'@'
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, // 'A'..'M'
    722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, // 'N'..'Z'
    278, 278, 278, 469, 556, 333, // '['..'`'
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, // 'a'..'m'
    556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, // 'n'..'z'
    334, 260, 334, 584, // '{'..'~'
];

fn text_width(text: &[u8], size: f32) -> f32 {
    let units: u32 = text
        .iter()
        .map(|&b| match b {
            32..=126 => u32::from(HELVETICA[usize::from(b - 32)]),
            _ => 556,
        })
        .sum();
    units as f32 * size / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_measures_text() {
        assert_eq!(win_ansi("Rp 5€ é ✓"), b"Rp 5\x80 \xe9 ?".to_vec());
        assert!((text_width(b"Hi", 10.0) - 9.44).abs() < 0.01);
    }
}
