use std::fmt::Display;

/// Errors carry messages meant for the person using the app, in Indonesian.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("Gagal membaca atau menulis file: {0}")]
    Io(#[from] std::io::Error),
    #[error("PDF tidak bisa diproses: {0}")]
    Pdf(#[from] lopdf::Error),
    #[error("Gambar tidak bisa diproses: {0}")]
    Image(#[from] image::ImageError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub(crate) fn fail<T>(message: impl Display) -> Result<T> {
    Err(Error::Message(message.to_string()))
}
