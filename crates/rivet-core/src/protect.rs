//! Password protection: adding it, changing it and taking it off.
//!
//! PDFium reads protected files but can't write protection, so it's applied
//! when saving (see [`crate::Document::final_bytes`]): the bytes PDFium wrote
//! are decrypted (when the file was protected) and encrypted again with lopdf,
//! using AES-256, the strongest method PDF has.

use std::{collections::BTreeMap, sync::Arc};

use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::{EncryptionState, EncryptionVersion, Object, Permissions};
use serde::Deserialize;
use ts_rs::TS;

use crate::{Error, ErrorCode, Result};

/// What the next save does about passwords.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum Protection {
    /// Protect the file. An empty `open_password` lets anyone open it (then
    /// only the restrictions apply); the `owner_password` lifts the restrictions.
    #[serde(rename_all = "camelCase")]
    Set {
        open_password: String,
        owner_password: String,
        allowed: Allowed,
    },
    /// Save without any protection.
    Remove,
}

/// What people who open a protected file (without the owner password) may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct Allowed {
    pub print: bool,
    pub copy: bool,
    /// Change the pages and their content.
    pub edit: bool,
    pub fill_forms: bool,
    /// Add comments and other annotations (and fill forms).
    pub annotate: bool,
}

impl Allowed {
    fn permissions(self) -> Permissions {
        // Screen readers may always read the text.
        let mut p = Permissions::COPYABLE_FOR_ACCESSIBILITY;
        if self.print {
            p |= Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY;
        }
        if self.copy {
            p |= Permissions::COPYABLE;
        }
        if self.edit {
            p |= Permissions::MODIFIABLE | Permissions::ASSEMBLABLE;
        }
        if self.fill_forms {
            p |= Permissions::FILLABLE;
        }
        if self.annotate {
            p |= Permissions::ANNOTABLE | Permissions::FILLABLE;
        }
        p
    }
}

fn failed(e: &dyn std::fmt::Display) -> Error {
    Error::new(ErrorCode::SaveFailed, e.to_string())
}

/// A protected file's bytes without the protection (`password` is either one).
pub(crate) fn decrypt(bytes: &[u8], password: &str) -> Result<Vec<u8>> {
    let options = lopdf::LoadOptions::with_password(password);
    let mut doc = lopdf::Document::load_mem_with_options(bytes, options).map_err(|e| failed(&e))?;
    if doc.is_encrypted() {
        doc.decrypt(password).map_err(|e| failed(&e))?;
    }
    doc.trailer.remove(b"Encrypt");
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| failed(&e))?;
    Ok(out)
}

/// Protects an unprotected file's bytes with AES-256.
pub(crate) fn encrypt(bytes: &[u8], open: &str, owner: &str, allowed: Allowed) -> Result<Vec<u8>> {
    let mut doc = lopdf::Document::load_mem(bytes).map_err(|e| failed(&e))?;
    // The file's identifier takes part in the encryption; PDFium writes one,
    // but a file without it gets one.
    if doc.trailer.get(b"ID").is_err() {
        let id = Object::string_literal(random::<16>()?.to_vec());
        doc.trailer.set("ID", Object::Array(vec![id.clone(), id]));
    }
    // Without an owner password of its own, the open password lifts the restrictions.
    let owner = if owner.is_empty() { open } else { owner };
    let key = random::<32>()?;
    let filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    let state = EncryptionState::try_from(EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
        file_encryption_key: &key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: owner,
        user_password: open,
        permissions: allowed.permissions(),
    })
    .map_err(|e| failed(&e))?;
    doc.encrypt(&state).map_err(|e| failed(&e))?;
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| failed(&e))?;
    Ok(out)
}

fn random<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|e| Error::new(ErrorCode::Internal, e.to_string()))?;
    Ok(bytes)
}
