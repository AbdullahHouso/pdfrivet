//! Whether PDFRivet is the app that opens PDF files, and a way to make it so.
//!
//! - Windows: apps can't make themselves the default (since Windows 8 only the user
//!   can), so we open PDFRivet's page in Settings → Default apps. The installer
//!   registers PDFRivet there (`windows/installer-hooks.nsh`).
//! - macOS: Launch Services sets it directly.
//! - Linux: `xdg-mime` sets it, when PDFRivet's desktop file is installed (a .deb
//!   or .rpm; an AppImage has none until something integrates it).

use serde::Serialize;
use tauri::AppHandle;

/// What `make_default` did.
#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// PDFRivet now opens PDFs (macOS, Linux).
    #[cfg_attr(windows, allow(dead_code))]
    Done,
    /// The system's settings are open; the user finishes there (Windows).
    #[cfg_attr(not(windows), allow(dead_code))]
    OpenedSettings,
}

#[cfg(windows)]
pub use windows_impl::{is_default, make_default};

#[cfg(target_os = "macos")]
pub use macos_impl::{is_default, make_default};

#[cfg(target_os = "linux")]
pub use linux_impl::{is_default, make_default};

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn is_default(_app: &AppHandle) -> Option<bool> {
    None
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn make_default(_app: &AppHandle) -> Result<Outcome, String> {
    Err("not supported on this platform".into())
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_impl {
    use super::{AppHandle, Outcome};
    use tauri_plugin_opener::OpenerExt;
    use windows::{
        Win32::UI::Shell::{ASSOCF_NOTRUNCATE, ASSOCSTR_EXECUTABLE, AssocQueryStringW},
        core::{PCWSTR, PWSTR, w},
    };

    /// Compares the program Windows starts for a .pdf with the one running now.
    /// `None` if Windows can't tell (no app opens PDFs at all counts as "not us").
    pub fn is_default(_app: &AppHandle) -> Option<bool> {
        let Some(handler) = pdf_handler() else {
            return Some(false);
        };
        let ours = std::env::current_exe().ok()?.canonicalize().ok()?;
        // Canonical paths have the same case and prefix on both sides.
        Some(std::path::Path::new(&handler).canonicalize().ok() == Some(ours))
    }

    /// The program that opens .pdf files, following the user's choice in Settings.
    fn pdf_handler() -> Option<String> {
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        // SAFETY: `buf` holds `len` UTF-16 units, and the strings are static and
        // null-terminated. Windows writes at most `len` units, including the null.
        unsafe {
            AssocQueryStringW(
                ASSOCF_NOTRUNCATE,
                ASSOCSTR_EXECUTABLE,
                w!(".pdf"),
                PCWSTR::null(),
                Some(PWSTR(buf.as_mut_ptr())),
                &mut len,
            )
        }
        .ok()
        .ok()?;
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }

    /// Opens Settings at PDFRivet's default-apps page (Windows 11), where one click on
    /// `.pdf` makes it the default. Windows 10 ignores the app name and opens the
    /// Default apps page itself.
    pub fn make_default(app: &AppHandle) -> Result<Outcome, String> {
        // The name the installer gives PDFRivet under `RegisteredApplications`.
        let url = format!(
            "ms-settings:defaultapps?registeredAppUser={}",
            app.package_info().name
        );
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|e| e.to_string())?;
        Ok(Outcome::OpenedSettings)
    }
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos_impl {
    use super::{AppHandle, Outcome};
    use objc2_core_foundation::{CFRetained, CFString};
    use std::ptr::NonNull;

    /// The Uniform Type Identifier of PDF files.
    const PDF_TYPE: &str = "com.adobe.pdf";
    /// `kLSRolesAll`: viewer, editor and shell roles.
    const ROLES_ALL: u32 = 0xFFFF_FFFF;

    // Deprecated since macOS 12 but still working, and unlike the newer NSWorkspace
    // call it needs no macOS version check.
    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn LSCopyDefaultRoleHandlerForContentType(
            content_type: &CFString,
            role: u32,
        ) -> Option<NonNull<CFString>>;
        fn LSSetDefaultRoleHandlerForContentType(
            content_type: &CFString,
            role: u32,
            handler: &CFString,
        ) -> i32;
    }

    /// Compares the bundle ID of the app that opens PDFs with ours.
    pub fn is_default(app: &AppHandle) -> Option<bool> {
        let pdf = CFString::from_str(PDF_TYPE);
        // SAFETY: `pdf` is a valid CFString; the result follows the Copy rule
        // (we own it), so `CFRetained` releases it.
        let handler = unsafe { LSCopyDefaultRoleHandlerForContentType(&pdf, ROLES_ALL) }
            .map(|ptr| unsafe { CFRetained::from_raw(ptr) });
        let Some(handler) = handler else {
            return Some(false);
        };
        // Launch Services may report the ID in lower case.
        Some(
            handler
                .to_string()
                .eq_ignore_ascii_case(&app.config().identifier),
        )
    }

    pub fn make_default(app: &AppHandle) -> Result<Outcome, String> {
        let pdf = CFString::from_str(PDF_TYPE);
        let ours = CFString::from_str(&app.config().identifier);
        // SAFETY: both arguments are valid CFStrings that outlive the call.
        let status = unsafe { LSSetDefaultRoleHandlerForContentType(&pdf, ROLES_ALL, &ours) };
        if status == 0 {
            Ok(Outcome::Done)
        } else {
            Err(format!("LSSetDefaultRoleHandlerForContentType: {status}"))
        }
    }
}

#[cfg(target_os = "linux")]
mod linux_impl {
    use super::{AppHandle, Outcome};
    use std::{path::PathBuf, process::Command};

    const PDF_TYPE: &str = "application/pdf";

    /// Tauri's packages name the desktop file after the product: `PDFRivet.desktop`.
    fn desktop_file(app: &AppHandle) -> String {
        format!("{}.desktop", app.package_info().name)
    }

    /// Whether the desktop file is installed in one of the XDG application folders.
    fn installed(file: &str) -> bool {
        let home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
        let system = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        home.into_iter()
            .chain(system.split(':').map(PathBuf::from))
            .any(|dir| dir.join("applications").join(file).is_file())
    }

    /// `None` when PDFRivet isn't installed (development, AppImage) or `xdg-mime`
    /// is missing: there's nothing the user could switch to.
    pub fn is_default(app: &AppHandle) -> Option<bool> {
        let file = desktop_file(app);
        if !installed(&file) {
            return None;
        }
        let out = Command::new("xdg-mime")
            .args(["query", "default", PDF_TYPE])
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        Some(String::from_utf8_lossy(&out.stdout).trim() == file)
    }

    pub fn make_default(app: &AppHandle) -> Result<Outcome, String> {
        let status = Command::new("xdg-mime")
            .args(["default", &desktop_file(app), PDF_TYPE])
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(Outcome::Done)
        } else {
            Err(format!("xdg-mime: {status}"))
        }
    }
}
