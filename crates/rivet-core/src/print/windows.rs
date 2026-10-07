//! Printing on Windows.
//!
//! Printers and their abilities come from the Windows print spooler. A job
//! opens a printer device context (DC) set up with the user's settings and asks
//! PDFium to draw every page straight onto it (`FPDF_RenderPage`), exactly how
//! Chrome prints PDFs. Output stays vector (sharp at any printer resolution).
//!
//! This module calls Win32 and PDFium functions directly, so it is one of the
//! few places allowed to use `unsafe`. Buffers are sized by asking Windows
//! first, handles are closed by `Drop` guards, and every call notes why it's safe.

#![allow(unsafe_code)]

use pdfium_render::prelude::*;
use serde::Serialize;
use ts_rs::TS;
use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::{
            Gdi::{
                CreateDCW, DEVMODEW, DM_COLLATE, DM_COLOR, DM_COPIES, DM_DUPLEX, DM_IN_BUFFER,
                DM_IN_PROMPT, DM_ORIENTATION, DM_OUT_BUFFER, DM_PAPERSIZE, DMCOLLATE_FALSE,
                DMCOLLATE_TRUE, DMCOLOR_COLOR, DMCOLOR_MONOCHROME, DMDUP_HORIZONTAL, DMDUP_SIMPLEX,
                DMDUP_VERTICAL, DMORIENT_LANDSCAPE, DMORIENT_PORTRAIT, DeleteDC, GetDeviceCaps,
                HDC, LOGPIXELSX, LOGPIXELSY, PHYSICALHEIGHT, PHYSICALOFFSETX, PHYSICALOFFSETY,
                PHYSICALWIDTH, ResetDCW,
            },
            Printing::{
                ClosePrinter, DocumentPropertiesW, EnumPrintersW, GetDefaultPrinterW, OpenPrinterW,
                PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_HANDLE, PRINTER_INFO_2W,
            },
        },
        Storage::Xps::{
            AbortDoc, DC_COLORDEVICE, DC_DUPLEX, DC_PAPERNAMES, DC_PAPERS, DC_PAPERSIZE, DOCINFOW,
            DeviceCapabilitiesW, EndDoc, EndPage, StartDocW, StartPage,
        },
    },
    core::{HSTRING, PCWSTR, PWSTR, w},
};

use super::{Duplex, Orientation, Paper, PrintSettings, PrinterInfo, place_page};
use crate::{Error, ErrorCode, Result};

// PDFium render flags (fpdfview.h).
const FPDF_ANNOT: i32 = 0x01;
const FPDF_GRAYSCALE: i32 = 0x08;
const FPDF_PRINTING: i32 = 0x800;

fn failed(what: impl std::fmt::Display) -> Error {
    Error::new(ErrorCode::PrintFailed, what.to_string())
}

/// Reads a NUL-terminated wide string from a Windows-owned pointer.
fn wide_to_string(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: Windows returned a valid NUL-terminated string.
    unsafe { p.to_string().unwrap_or_default() }
}

/// An open printer handle, closed when dropped.
struct Printer(PRINTER_HANDLE);

impl Printer {
    fn open(name: &str) -> Result<Self> {
        let mut handle = PRINTER_HANDLE::default();
        // SAFETY: `handle` is a valid out-pointer; the name is NUL-terminated.
        unsafe { OpenPrinterW(&HSTRING::from(name), &mut handle, None) }.map_err(failed)?;
        Ok(Self(handle))
    }

    /// The printer's settings (DEVMODE) as bytes: its defaults, or `input` merged in,
    /// optionally after showing the driver's own settings window.
    fn devmode(
        &self,
        name: &str,
        input: Option<&[u8]>,
        prompt: Option<HWND>,
    ) -> Result<Option<Vec<u8>>> {
        let name = HSTRING::from(name);
        // SAFETY: asking for the size needs no buffers.
        let size = unsafe { DocumentPropertiesW(None, self.0, &name, None, None, 0) };
        if size <= 0 {
            return Err(failed("the printer driver gave no settings"));
        }
        // u64 storage keeps the DEVMODE properly aligned.
        let mut out = vec![0u64; (size as usize).div_ceil(8)];
        let input_copy = input.map(|b| {
            let mut v = vec![0u64; b.len().div_ceil(8).max(out.len())];
            // SAFETY: `v` has at least `b.len()` bytes.
            unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), v.as_mut_ptr().cast(), b.len()) };
            v
        });
        let mut mode = DM_OUT_BUFFER.0;
        if input_copy.is_some() {
            mode |= DM_IN_BUFFER.0;
        }
        if prompt.is_some() {
            mode |= DM_IN_PROMPT.0;
        }
        // SAFETY: `out` holds `size` bytes as the driver asked; `input_copy` (if any)
        // is a DEVMODE previously produced by this driver.
        let result = unsafe {
            DocumentPropertiesW(
                prompt,
                self.0,
                &name,
                Some(out.as_mut_ptr().cast()),
                input_copy.as_ref().map(|v| v.as_ptr().cast::<DEVMODEW>()),
                mode,
            )
        };
        const IDOK: i32 = 1;
        if result < 0 {
            return Err(failed("the printer driver refused the settings"));
        }
        if prompt.is_some() && result != IDOK {
            return Ok(None); // The user cancelled the driver's window.
        }
        // SAFETY: `out` was sized for a DEVMODE of `size` bytes.
        let bytes = unsafe { std::slice::from_raw_parts(out.as_ptr().cast::<u8>(), size as usize) };
        Ok(Some(bytes.to_vec()))
    }
}

impl Drop for Printer {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenPrinterW and is closed once.
        let _ = unsafe { ClosePrinter(self.0) };
    }
}

fn default_printer() -> Option<String> {
    let mut len = 0u32;
    // SAFETY: the first call only reports the needed length.
    let _ = unsafe { GetDefaultPrinterW(None, &mut len) };
    if len == 0 {
        return None;
    }
    let mut buf = vec![0u16; len as usize];
    // SAFETY: `buf` holds `len` characters.
    let ok = unsafe { GetDefaultPrinterW(Some(PWSTR(buf.as_mut_ptr())), &mut len) };
    ok.as_bool().then(|| {
        String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())])
    })
}

/// Asks the driver about one capability, returning the raw count.
fn capability(
    name: &HSTRING,
    port: &HSTRING,
    cap: windows::Win32::Storage::Xps::PRINTER_DEVICE_CAPABILITIES,
    out: Option<&mut [u16]>,
) -> i32 {
    // SAFETY: when given, `out` is sized from an earlier count query for the same capability.
    unsafe { DeviceCapabilitiesW(name, port, cap, out.map(|b| PWSTR(b.as_mut_ptr())), None) }
}

fn papers_of(name: &HSTRING, port: &HSTRING) -> Vec<Paper> {
    let count = capability(name, port, DC_PAPERS, None);
    if count <= 0 {
        return Vec::new();
    }
    let n = count as usize;
    let mut ids = vec![0u16; n];
    let mut names = vec![0u16; n * 64];
    // POINT = two i32 = four u16 per paper, in tenths of a millimetre.
    let mut sizes = vec![0u16; n * 4];
    capability(name, port, DC_PAPERS, Some(&mut ids));
    capability(name, port, DC_PAPERNAMES, Some(&mut names));
    capability(name, port, DC_PAPERSIZE, Some(&mut sizes));
    (0..n)
        .filter_map(|i| {
            let raw = &names[i * 64..(i + 1) * 64];
            let label =
                String::from_utf16_lossy(&raw[..raw.iter().position(|&c| c == 0).unwrap_or(64)]);
            let w = i32::from(sizes[i * 4]) | (i32::from(sizes[i * 4 + 1]) << 16);
            let h = i32::from(sizes[i * 4 + 2]) | (i32::from(sizes[i * 4 + 3]) << 16);
            (w > 0 && h > 0).then(|| Paper {
                id: ids[i].to_string(),
                name: label.trim().to_owned(),
                width_mm: w as f32 / 10.0,
                height_mm: h as f32 / 10.0,
            })
        })
        .collect()
}

pub(crate) fn list_printers() -> Result<Vec<PrinterInfo>> {
    let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
    let (mut needed, mut count) = (0u32, 0u32);
    // SAFETY: the first call only reports the needed buffer size.
    let _ = unsafe { EnumPrintersW(flags, PCWSTR::null(), 2, None, &mut needed, &mut count) };
    if needed == 0 {
        return Ok(Vec::new());
    }
    let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
    // SAFETY: `buf` holds `needed` bytes, aligned for PRINTER_INFO_2W.
    let bytes =
        unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr().cast::<u8>(), needed as usize) };
    // SAFETY: as above; Windows fills `count` structs at the start of the buffer.
    unsafe {
        EnumPrintersW(
            flags,
            PCWSTR::null(),
            2,
            Some(bytes),
            &mut needed,
            &mut count,
        )
    }
    .map_err(failed)?;
    // SAFETY: Windows wrote `count` PRINTER_INFO_2W structs at the buffer start.
    let infos = unsafe {
        std::slice::from_raw_parts(buf.as_ptr().cast::<PRINTER_INFO_2W>(), count as usize)
    };
    let default = default_printer();

    Ok(infos
        .iter()
        .map(|info| {
            let name = wide_to_string(info.pPrinterName);
            let port = wide_to_string(info.pPortName);
            let (hname, hport) = (HSTRING::from(name.as_str()), HSTRING::from(port.as_str()));
            let default_paper = (!info.pDevMode.is_null()).then(|| {
                // SAFETY: Windows gave a valid DEVMODE pointer inside `buf`.
                let dm = unsafe { &*info.pDevMode };
                // SAFETY: printer DEVMODEs use the printer half of the union.
                unsafe { dm.Anonymous1.Anonymous1.dmPaperSize }.to_string()
            });
            PrinterInfo {
                is_default: default.as_deref() == Some(name.as_str()),
                papers: papers_of(&hname, &hport),
                default_paper,
                duplex: capability(&hname, &hport, DC_DUPLEX, None) == 1,
                color: capability(&hname, &hport, DC_COLORDEVICE, None) == 1,
                has_properties: true,
                name,
            }
        })
        .collect())
}

/// What the driver's settings window chose, so the Rivet dialog can show it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct PrinterProperties {
    /// The driver's full settings, kept and passed back when printing.
    #[ts(type = "number[]")]
    pub devmode: Vec<u8>,
    pub copies: u32,
    pub duplex: Duplex,
    pub grayscale: bool,
    pub paper: Option<String>,
    pub landscape: bool,
}

/// Shows the printer driver's own settings window ("Printer properties…").
/// Returns `None` if the user cancelled.
pub fn printer_properties(
    window: isize,
    printer: &str,
    current: Option<&[u8]>,
) -> Result<Option<PrinterProperties>> {
    let handle = Printer::open(printer)?;
    let Some(devmode) = handle.devmode(printer, current, Some(HWND(window as *mut _)))? else {
        return Ok(None);
    };
    // SAFETY: the bytes are a DEVMODE just produced by the driver.
    let dm = unsafe { &*devmode.as_ptr().cast::<DEVMODEW>() };
    // SAFETY: printer DEVMODEs use the printer half of the union.
    let fields = unsafe { dm.Anonymous1.Anonymous1 };
    Ok(Some(PrinterProperties {
        copies: fields.dmCopies.max(1) as u32,
        duplex: match dm.dmDuplex {
            d if d == DMDUP_VERTICAL => Duplex::LongEdge,
            d if d == DMDUP_HORIZONTAL => Duplex::ShortEdge,
            _ => Duplex::OneSided,
        },
        grayscale: dm.dmColor == DMCOLOR_MONOCHROME,
        paper: Some(fields.dmPaperSize.to_string()),
        landscape: fields.dmOrientation as u32 == DMORIENT_LANDSCAPE,
        devmode,
    }))
}

/// A printer device context, deleted when dropped.
struct PrinterDc(HDC);

impl Drop for PrinterDc {
    fn drop(&mut self) {
        // SAFETY: the DC came from CreateDCW and is deleted once.
        let _ = unsafe { DeleteDC(self.0) };
    }
}

/// Applies the dialog's choices on top of the driver's settings.
fn apply_settings(devmode: &mut [u64], settings: &PrintSettings, landscape: bool) {
    // SAFETY: `devmode` holds a DEVMODE produced by the driver (aligned u64 storage).
    let dm = unsafe { &mut *devmode.as_mut_ptr().cast::<DEVMODEW>() };
    dm.dmFields |= DM_COPIES | DM_COLLATE | DM_DUPLEX | DM_COLOR | DM_ORIENTATION;
    // SAFETY: printer DEVMODEs use the printer half of the union.
    let fields = unsafe { &mut dm.Anonymous1.Anonymous1 };
    fields.dmCopies = settings.copies.clamp(1, i16::MAX as u32) as i16;
    fields.dmOrientation = if landscape {
        DMORIENT_LANDSCAPE
    } else {
        DMORIENT_PORTRAIT
    } as i16;
    if let Some(paper) = settings
        .paper
        .as_deref()
        .and_then(|p| p.parse::<i16>().ok())
    {
        dm.dmFields |= DM_PAPERSIZE;
        fields.dmPaperSize = paper;
    }
    dm.dmCollate = if settings.collate {
        DMCOLLATE_TRUE
    } else {
        DMCOLLATE_FALSE
    };
    dm.dmDuplex = match settings.duplex {
        Duplex::OneSided => DMDUP_SIMPLEX,
        Duplex::LongEdge => DMDUP_VERTICAL,
        Duplex::ShortEdge => DMDUP_HORIZONTAL,
    };
    dm.dmColor = if settings.grayscale {
        DMCOLOR_MONOCHROME
    } else {
        DMCOLOR_COLOR
    };
}

pub(crate) fn print_document(
    pdfium: &Pdfium,
    document: &PdfDocument,
    settings: &PrintSettings,
    printer_settings: Option<&[u8]>,
) -> Result<()> {
    let printer = Printer::open(&settings.printer)?;
    let base = printer
        .devmode(&settings.printer, printer_settings, None)?
        .ok_or_else(|| failed("no printer settings"))?;
    let mut devmode = vec![0u64; base.len().div_ceil(8)];
    // SAFETY: `devmode` has room for `base.len()` bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(base.as_ptr(), devmode.as_mut_ptr().cast(), base.len())
    };

    let sizes: Vec<(f32, f32)> = settings
        .pages
        .iter()
        .map(|&p| {
            document
                .pages()
                .page_size(p as PdfPageIndex)
                .map(|r| (r.width().value, r.height().value))
        })
        .collect::<std::result::Result<_, _>>()?;
    let landscape_for = |(w, h): (f32, f32)| match settings.orientation {
        Orientation::Auto => w > h,
        Orientation::Portrait => false,
        Orientation::Landscape => true,
    };
    let first_landscape = sizes.first().map(|&s| landscape_for(s)).unwrap_or(false);
    apply_settings(&mut devmode, settings, first_landscape);

    let name = HSTRING::from(settings.printer.as_str());
    // SAFETY: `devmode` is a complete DEVMODE for this printer.
    let dc = unsafe {
        CreateDCW(
            w!("WINSPOOL"),
            &name,
            PCWSTR::null(),
            Some(devmode.as_ptr().cast()),
        )
    };
    if dc.is_invalid() {
        return Err(failed("could not open the printer"));
    }
    let dc = PrinterDc(dc);
    let title = HSTRING::from(settings.title.as_str());
    let doc_info = DOCINFOW {
        cbSize: std::mem::size_of::<DOCINFOW>() as i32,
        lpszDocName: PCWSTR(title.as_ptr()),
        ..Default::default()
    };
    // SAFETY: valid DC and DOCINFOW; `title` outlives the call.
    if unsafe { StartDocW(dc.0, &doc_info) } <= 0 {
        return Err(failed("the printer refused the job"));
    }

    let bindings = pdfium.bindings();
    let flags = FPDF_ANNOT
        | FPDF_PRINTING
        | if settings.grayscale {
            FPDF_GRAYSCALE
        } else {
            0
        };
    let mut current_landscape = first_landscape;
    for (&index, &size) in settings.pages.iter().zip(&sizes) {
        // Turn the sheet between pages when needed (allowed between EndPage and StartPage).
        let landscape = landscape_for(size);
        if landscape != current_landscape {
            apply_settings(&mut devmode, settings, landscape);
            // SAFETY: valid DC between pages, complete DEVMODE.
            unsafe { ResetDCW(dc.0, devmode.as_ptr().cast()) };
            current_landscape = landscape;
        }
        let page = document.pages().get(index as PdfPageIndex)?;
        // SAFETY: valid DC; these queries have no other requirements.
        let (dpi_x, dpi_y, paper_w, paper_h, off_x, off_y) = unsafe {
            (
                GetDeviceCaps(Some(dc.0), LOGPIXELSX) as f32,
                GetDeviceCaps(Some(dc.0), LOGPIXELSY) as f32,
                GetDeviceCaps(Some(dc.0), PHYSICALWIDTH) as f32,
                GetDeviceCaps(Some(dc.0), PHYSICALHEIGHT) as f32,
                GetDeviceCaps(Some(dc.0), PHYSICALOFFSETX) as f32,
                GetDeviceCaps(Some(dc.0), PHYSICALOFFSETY) as f32,
            )
        };
        let to_pt_x = 72.0 / dpi_x.max(1.0);
        let to_pt_y = 72.0 / dpi_y.max(1.0);
        let margin = (off_x * to_pt_x).max(off_y * to_pt_y);
        // The DC is already in the right orientation, so place on it as "portrait".
        let placement = place_page(
            size.0,
            size.1,
            paper_w * to_pt_x,
            paper_h * to_pt_y,
            margin,
            settings.scaling,
            if paper_w > paper_h {
                Orientation::Landscape
            } else {
                Orientation::Portrait
            },
        );
        // Device coordinates start at the printable area's corner.
        let x = (placement.x / to_pt_x - off_x).round() as i32;
        let y = (placement.y / to_pt_y - off_y).round() as i32;
        let w = (placement.width / to_pt_x).round() as i32;
        let h = (placement.height / to_pt_y).round() as i32;

        // SAFETY: valid DC; the page handle belongs to a live PdfPage; we're on the PDFium thread.
        let ok = unsafe {
            if StartPage(dc.0) <= 0 {
                false
            } else {
                bindings.FPDF_RenderPage(dc.0, page.page_handle(), x, y, w, h, 0, flags);
                EndPage(dc.0) > 0
            }
        };
        if !ok {
            // SAFETY: valid DC with a started document.
            unsafe { AbortDoc(dc.0) };
            return Err(failed("printing a page failed"));
        }
    }
    // SAFETY: valid DC with a started document.
    if unsafe { EndDoc(dc.0) } <= 0 {
        return Err(failed("the printer did not finish the job"));
    }
    Ok(())
}
