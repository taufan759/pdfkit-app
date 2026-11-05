//! PDF operations behind PDFKit.
//!
//! Every tool reads its inputs from disk and writes new files next to them; the
//! originals are never modified. Everything runs locally, nothing goes online.

mod compress;
mod doc;
mod error;
mod images;
mod organize;
mod pages;
mod render;
mod security;
mod stamp;
mod tools;

pub use error::{Error, Result};
pub use pages::{parse_page_list, parse_page_set, parse_ranges};
pub use tools::{inspect_files, run_tool, FileInfo, ToolResult};
