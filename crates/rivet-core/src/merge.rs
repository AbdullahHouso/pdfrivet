//! Merging documents into a new one.
//!
//! The pages are copied by PDFium's import (see [`Document::import_pages`]),
//! one source at a time. This module then finishes the merged document:
//! bookmarks (each file's own, optionally under a bookmark named after the
//! file), and form fields, which PDFium's import brings along as widgets on
//! the pages but without the document's list of fields (`/AcroForm`), so
//! readers wouldn't treat them as fields any more.

use std::collections::{BTreeSet, HashMap, HashSet};

use lopdf::{Dictionary, Object, ObjectId};
use serde::Deserialize;
use ts_rs::TS;

use crate::{
    Document, Error, ErrorCode, OutlineItem, Result, document::write_atomically,
    incremental::Pdf as RawPdf, pages::remap_outline,
};

/// One merged file: where its pages went in the merged document.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct MergePart {
    /// The open document the pages came from.
    pub doc: crate::DocId,
    /// Its pages that were copied (0-based), in order.
    pub pages: Vec<u32>,
    /// The merged document's page where they start.
    pub start: u32,
    /// Names its bookmark (usually the file name).
    pub title: String,
}

/// Finishes a merged document (`merged`, whose pages were already copied from
/// `sources`) and writes it to `path`.
pub(crate) fn finish(
    merged: &mut Document,
    parts: &[(MergePart, &Document)],
    bookmark_files: bool,
    path: &std::path::Path,
) -> Result<()> {
    // Bookmarks: each file's own, renumbered; with `bookmark_files`, under one
    // bookmark per file.
    let mut outline = Vec::new();
    for (part, source) in parts {
        let place: HashMap<u32, u32> = part
            .pages
            .iter()
            .enumerate()
            .map(|(i, &p)| (p, part.start + i as u32))
            .collect();
        let mut items = remap_outline(source.outline(), &|p| place.get(&p).copied());
        forget_origins(&mut items);
        if bookmark_files {
            outline.push(OutlineItem {
                title: part.title.clone(),
                page: Some(part.start),
                children: items,
                origin: None,
            });
        } else {
            outline.extend(items);
        }
    }
    if !outline.is_empty() {
        merged.edited_outline = Some(outline);
    }
    let (mut bytes, _) = merged.final_bytes()?;

    let forms: Vec<(&MergePart, RawForm)> = parts
        .iter()
        .filter_map(|(part, source)| Some((part, read_form(source)?)))
        .collect();
    if !forms.is_empty() {
        bytes = join_forms(&bytes, &forms)?;
    }
    write_atomically(path, &bytes)
}

/// Bookmarks from another file can't reuse that file's outline entries.
fn forget_origins(items: &mut [OutlineItem]) {
    for item in items {
        item.origin = None;
        forget_origins(&mut item.children);
    }
}

/// A source's `/AcroForm` settings that the merged form needs: the default
/// appearance and the resources (fonts) its fields are drawn with.
struct RawForm {
    default_appearance: Option<Object>,
    resources: Option<Object>,
}

/// Reads a source's form settings, if it has form fields (and isn't encrypted).
fn read_form(source: &Document) -> Option<RawForm> {
    let bytes = source.file_bytes()?;
    let pdf = RawPdf::read(&bytes).ok()?;
    if pdf.trailer.has(b"Encrypt") {
        return None;
    }
    let root = pdf.resolve(pdf.trailer.get(b"Root").ok()?)?;
    let form = pdf.resolve(root.as_dict().ok()?.get(b"AcroForm").ok()?)?;
    let form = form.as_dict().ok()?;
    let fields = pdf.resolve(form.get(b"Fields").ok()?)?;
    if fields.as_array().map_or(true, Vec::is_empty) {
        return None;
    }
    let resources = form
        .get(b"DR")
        .ok()
        .and_then(|dr| deep_resolve(&pdf, dr, &mut HashSet::new(), 0));
    let default_appearance = form.get(b"DA").ok().cloned();
    Some(RawForm {
        default_appearance,
        resources,
    })
}

/// An object with every reference in it replaced by what it points to (for
/// copying a source's form resources, which are small, into the merged file).
fn deep_resolve(
    pdf: &RawPdf,
    object: &Object,
    seen: &mut HashSet<u32>,
    depth: usize,
) -> Option<Object> {
    if depth > 16 {
        return None;
    }
    Some(match object {
        Object::Reference((number, _)) => {
            if !seen.insert(*number) {
                return None;
            }
            let target = pdf.object(*number)?;
            let resolved = deep_resolve(pdf, &target, seen, depth + 1);
            seen.remove(number);
            resolved?
        }
        Object::Array(items) => Object::Array(
            items
                .iter()
                .filter_map(|i| deep_resolve(pdf, i, seen, depth + 1))
                .collect(),
        ),
        Object::Dictionary(dict) => Object::Dictionary(resolve_dict(pdf, dict, seen, depth)),
        Object::Stream(stream) => {
            let mut copy = stream.clone();
            copy.dict = resolve_dict(pdf, &stream.dict, seen, depth);
            Object::Stream(copy)
        }
        other => other.clone(),
    })
}

fn resolve_dict(
    pdf: &RawPdf,
    dict: &Dictionary,
    seen: &mut HashSet<u32>,
    depth: usize,
) -> Dictionary {
    let mut out = Dictionary::new();
    for (key, value) in dict.iter() {
        // A font's parent links would pull in whole page trees.
        if key == b"Parent" || key == b"P" {
            continue;
        }
        if let Some(v) = deep_resolve(pdf, value, seen, depth + 1) {
            out.set(key.clone(), v);
        }
    }
    out
}

/// Gives the merged document one `/AcroForm` listing every field its pages'
/// widgets belong to. Fields from different files with the same name would
/// share their value, so later ones are renamed ("Name_2").
fn join_forms(bytes: &[u8], forms: &[(&MergePart, RawForm)]) -> Result<Vec<u8>> {
    let failed = |e: &dyn std::fmt::Display| Error::new(ErrorCode::SaveFailed, e.to_string());
    let mut doc = lopdf::Document::load_mem(bytes).map_err(|e| failed(&e))?;
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();

    let mut fields: Vec<ObjectId> = Vec::new();
    let mut names: HashSet<Vec<u8>> = HashSet::new();
    let mut resources = Dictionary::new();
    let mut default_appearance = None;
    for (n, (part, form)) in forms.iter().enumerate() {
        // The top-level fields of this file's widgets, in page order.
        let mut roots = Vec::new();
        let mut seen = BTreeSet::new();
        let range = part.start as usize..part.start as usize + part.pages.len();
        for &page in pages.get(range).unwrap_or(&[]) {
            for widget in widgets(&doc, page) {
                let root = field_root(&doc, widget);
                if seen.insert(root) {
                    roots.push(root);
                }
            }
        }
        for &root in &roots {
            let Ok(field) = doc.get_dictionary_mut(root) else {
                continue;
            };
            let Ok(name) = field.get(b"T").and_then(Object::as_str).map(<[u8]>::to_vec) else {
                continue;
            };
            let mut unique = name.clone();
            let mut k = n + 1;
            while names.contains(&unique) {
                unique = [name.as_slice(), format!("_{k}").as_bytes()].concat();
                k += 1;
            }
            if unique != name {
                field.set("T", lopdf::text_string(&String::from_utf8_lossy(&unique)));
            }
            names.insert(unique);
        }
        fields.extend(roots);
        default_appearance = default_appearance.or_else(|| form.default_appearance.clone());
        if let Some(Object::Dictionary(dr)) = &form.resources {
            merge_resources(&mut doc, &mut resources, dr);
        }
    }
    if fields.is_empty() {
        return Ok(bytes.to_vec());
    }
    let mut acroform = Dictionary::new();
    acroform.set(
        "Fields",
        Object::Array(fields.into_iter().map(Object::Reference).collect()),
    );
    if let Some(da) = default_appearance {
        acroform.set("DA", da);
    }
    if !resources.is_empty() {
        acroform.set("DR", Object::Dictionary(resources));
    }
    let form_id = doc.add_object(Object::Dictionary(acroform));
    doc.catalog_mut()
        .map_err(|e| failed(&e))?
        .set("AcroForm", form_id);
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| failed(&e))?;
    Ok(out)
}

/// The widget annotations of a page.
fn widgets(doc: &lopdf::Document, page: ObjectId) -> Vec<ObjectId> {
    let Ok(page) = doc.get_dictionary(page) else {
        return Vec::new();
    };
    let annots = match page.get(b"Annots") {
        Ok(Object::Array(a)) => a.clone(),
        Ok(Object::Reference(id)) => doc
            .get_object(*id)
            .and_then(Object::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    annots
        .iter()
        .filter_map(|a| a.as_reference().ok())
        .filter(|id| {
            doc.get_dictionary(*id)
                .and_then(|d| d.get(b"Subtype"))
                .and_then(Object::as_name)
                .is_ok_and(|s| s == b"Widget")
        })
        .collect()
}

/// The top-level field a widget belongs to (following `/Parent`).
fn field_root(doc: &lopdf::Document, widget: ObjectId) -> ObjectId {
    let mut current = widget;
    for _ in 0..32 {
        match doc
            .get_dictionary(current)
            .and_then(|d| d.get(b"Parent"))
            .and_then(Object::as_reference)
        {
            Ok(parent) => current = parent,
            Err(_) => break,
        }
    }
    current
}

/// Adds a source's form resources (`/Font` and the like) to the merged ones,
/// as new objects; a name already used keeps the first file's.
fn merge_resources(doc: &mut lopdf::Document, into: &mut Dictionary, from: &Dictionary) {
    for (category, entries) in from.iter() {
        let Object::Dictionary(entries) = entries else {
            continue;
        };
        let mut target = match into.get(category) {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };
        for (name, value) in entries.iter() {
            if target.has(name) {
                continue;
            }
            let id = doc.add_object(value.clone());
            target.set(name.clone(), Object::Reference(id));
        }
        into.set(category.clone(), Object::Dictionary(target));
    }
}
