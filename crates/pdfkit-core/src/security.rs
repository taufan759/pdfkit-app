//! Adding and removing passwords. New locks use AES-256, the strongest PDF encryption.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::{Document, EncryptionState, EncryptionVersion, Permissions};

use crate::doc::{file_name, load, save};
use crate::error::{fail, Error, Result};

pub(crate) fn protect(input: &Path, output: &Path, password: &str) -> Result<u64> {
    if password.is_empty() {
        return fail("Password tidak boleh kosong.");
    }
    let mut doc = load(input)?;
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).map_err(|e| Error::Message(format!("Gagal membuat kunci enkripsi: {e}")))?;
    let filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    let state = EncryptionState::try_from(EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
        file_encryption_key: &key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: password,
        user_password: password,
        permissions: Permissions::all(),
    })?;
    doc.compress();
    doc.encrypt(&state)?;
    save(&mut doc, output)
}

pub(crate) fn unlock(input: &Path, output: &Path, password: &str) -> Result<u64> {
    let mut doc = match Document::load_with_password(input, password) {
        Ok(doc) if doc.is_encrypted() => return fail("Password salah."),
        Ok(doc) => doc,
        Err(lopdf::Error::InvalidPassword | lopdf::Error::Decryption(_)) => return fail("Password salah."),
        Err(e) => return fail(format!("\"{}\" tidak bisa dibaca sebagai PDF ({e}).", file_name(input))),
    };
    if !doc.was_encrypted() {
        return fail(format!("\"{}\" tidak dikunci password.", file_name(input)));
    }
    save(&mut doc, output)
}
