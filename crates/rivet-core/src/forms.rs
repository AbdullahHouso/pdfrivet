//! Interactive forms (AcroForms): reading fields and filling them in.
//!
//! Reading uses pdfium-render's safe API. Changing values goes through PDFium's
//! own form-filling engine (the `FORM_*` functions, the same ones Chrome's PDF
//! viewer uses). That engine knows every field type's rules (checkbox "on"
//! names, radio groups, choice lists) and regenerates each field's appearance,
//! so filled forms look right here and in other PDF readers.
//!
//! pdfium-render doesn't wrap the `FORM_*` functions yet, so this is the one
//! module that calls PDFium directly (`unsafe`). Every call follows the same
//! rules, checked in [`FormSession`]:
//! - handles come from live pdfium-render objects that outlive the calls;
//! - all calls happen on the Engine's single PDFium thread;
//! - the page is announced to the form engine (`FORM_OnAfterLoadPage`) before use
//!   and retired (`FORM_OnBeforeClosePage`) before the page is dropped.

#![allow(unsafe_code)]

use crate::document::RawBindings;
use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Error, ErrorCode, Result, geometry::PageGeometry, textstrings::Texts};

/// What kind of field this is, with its current value.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum FieldKind {
    Text {
        value: String,
        multiline: bool,
        password: bool,
    },
    Checkbox {
        checked: bool,
    },
    Radio {
        checked: bool,
    },
    /// A drop-down (combo box) or list box.
    Choice {
        options: Vec<String>,
        selected: Option<u32>,
    },
    /// Push buttons, signatures and unknown fields: shown, not editable (yet).
    Other,
}

/// One form field widget on a page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct FormField {
    /// Position of the widget among the page's annotations; identifies it in changes.
    pub index: u32,
    pub name: Option<String>,
    pub read_only: bool,
    /// Where the field is, as fractions of the page (top-left origin, before view rotation).
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub field: FieldKind,
}

/// A change the user made to a field.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum FieldChange {
    /// Click a checkbox or radio button (toggles / selects it).
    Toggle,
    /// Replace the text of a text field.
    Text { value: String },
    /// Select an option of a drop-down or list.
    Select { option: u32 },
}

pub(crate) fn read(page: &PdfPage, texts: &Texts) -> Vec<FormField> {
    let Some(geometry) = PageGeometry::new(page) else {
        return Vec::new();
    };
    page.annotations()
        .iter()
        .enumerate()
        .filter_map(|(index, annotation)| {
            let field = annotation.as_form_field()?;
            let rect = geometry.to_fraction(&annotation.bounds().ok()?)?;
            Some(FormField {
                index: index as u32,
                name: field.name().map(|n| texts.repair_dotted(n)),
                read_only: field.is_read_only(),
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
                field: kind_of(field, texts),
            })
        })
        .collect()
}

fn kind_of(field: &PdfFormField, texts: &Texts) -> FieldKind {
    match field {
        PdfFormField::Text(text) => FieldKind::Text {
            value: texts.repair(text.value().unwrap_or_default()),
            multiline: text.is_multiline(),
            password: text.is_password(),
        },
        PdfFormField::Checkbox(checkbox) => FieldKind::Checkbox {
            checked: checkbox.is_checked().unwrap_or(false),
        },
        PdfFormField::RadioButton(radio) => FieldKind::Radio {
            checked: radio.is_checked().unwrap_or(false),
        },
        PdfFormField::ComboBox(combo) => choice(combo.options(), texts),
        PdfFormField::ListBox(list) => choice(list.options(), texts),
        _ => FieldKind::Other,
    }
}

fn choice(options: &PdfFormFieldOptions, texts: &Texts) -> FieldKind {
    let mut labels = Vec::new();
    let mut selected = None;
    for option in options.iter() {
        if option.is_set() {
            selected = Some(labels.len() as u32);
        }
        labels.push(texts.repair(option.label().cloned().unwrap_or_default()));
    }
    FieldKind::Choice {
        options: labels,
        selected,
    }
}

/// Applies a change to the field with annotation `index` on `page`.
pub(crate) fn apply(
    pdfium: &Pdfium,
    document: &PdfDocument,
    page: &PdfPage,
    index: u32,
    change: &FieldChange,
) -> Result<()> {
    let annotation = page.annotations().get(index as PdfPageAnnotationIndex)?;
    let field = annotation.as_form_field().ok_or_else(|| {
        Error::new(
            ErrorCode::Internal,
            format!("annotation {index} is not a form field"),
        )
    })?;
    if field.is_read_only() {
        return Err(Error::new(
            ErrorCode::ReadOnlyField,
            field.name().unwrap_or_default(),
        ));
    }
    let bounds = annotation.bounds()?;
    let form = document
        .form()
        .ok_or_else(|| Error::new(ErrorCode::Internal, "document has no form"))?;

    let session = FormSession::start(
        pdfium,
        pdfium.bindings().get_handle_from_form(form),
        pdfium.bindings().get_handle_from_page(page),
    );
    let ok = match change {
        FieldChange::Toggle => {
            // Click the middle of the widget, exactly as a mouse would.
            let x = ((bounds.left().value + bounds.right().value) / 2.0) as f64;
            let y = ((bounds.top().value + bounds.bottom().value) / 2.0) as f64;
            session.click(x, y)
        }
        FieldChange::Text { value } => session.focus(index) && session.replace_all_text(value),
        FieldChange::Select { option } => session.focus(index) && session.select_option(*option),
    };
    // Leaving the field commits the value and rebuilds its appearance.
    session.commit();
    if ok {
        Ok(())
    } else {
        Err(Error::new(
            ErrorCode::Internal,
            format!("PDFium refused the change to field {index}"),
        ))
    }
}

/// A page announced to PDFium's form engine. Retires the page when dropped.
struct FormSession<'a> {
    bindings: &'a dyn PdfiumLibraryBindings,
    form: FPDF_FORMHANDLE,
    page: FPDF_PAGE,
}

impl<'a> FormSession<'a> {
    fn start(pdfium: &'a Pdfium, form: FPDF_FORMHANDLE, page: FPDF_PAGE) -> Self {
        let bindings = pdfium.bindings();
        // SAFETY: `form` and `page` come from a live PdfDocument/PdfPage that the
        // caller keeps alive for the whole session; we're on the PDFium thread.
        unsafe { bindings.FORM_OnAfterLoadPage(page, form) };
        Self {
            bindings,
            form,
            page,
        }
    }

    fn click(&self, x: f64, y: f64) -> bool {
        // SAFETY: valid handles (see `start`); coordinates are plain numbers.
        unsafe {
            self.bindings
                .FORM_OnLButtonDown(self.form, self.page, 0, x, y);
            self.bindings
                .FORM_OnLButtonUp(self.form, self.page, 0, x, y)
                != 0
        }
    }

    fn focus(&self, index: u32) -> bool {
        // SAFETY: the annotation handle is opened and closed within this block,
        // and only used while the page is alive.
        unsafe {
            let annot = self.bindings.FPDFPage_GetAnnot(self.page, index as i32);
            if annot.is_null() {
                return false;
            }
            let focused = self.bindings.FORM_SetFocusedAnnot(self.form, annot) != 0;
            self.bindings.FPDFPage_CloseAnnot(annot);
            focused
        }
    }

    fn replace_all_text(&self, value: &str) -> bool {
        // PDFium expects a NUL-terminated UTF-16 string.
        let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` outlives the call and is NUL-terminated.
        unsafe {
            self.bindings.FORM_SelectAllText(self.form, self.page);
            self.bindings
                .FORM_ReplaceSelection(self.form, self.page, wide.as_ptr() as _);
        }
        true
    }

    fn select_option(&self, option: u32) -> bool {
        // SAFETY: valid handles; PDFium checks the index range itself.
        unsafe {
            self.bindings
                .FORM_SetIndexSelected(self.form, self.page, option as i32, 1)
                != 0
        }
    }

    fn commit(&self) {
        // SAFETY: valid form handle.
        unsafe { self.bindings.FORM_ForceToKillFocus(self.form) };
    }
}

impl Drop for FormSession<'_> {
    fn drop(&mut self) {
        // SAFETY: matches FORM_OnAfterLoadPage in `start`; the page is still alive
        // because the session never outlives the caller's PdfPage.
        unsafe { self.bindings.FORM_OnBeforeClosePage(self.page, self.form) };
    }
}
