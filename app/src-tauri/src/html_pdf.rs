//! HTML files and web pages to PDF, printed by the system's own web engine
//! (WebView2 on Windows, WebKit on macOS and Linux), so nothing extra is
//! downloaded or installed.
//!
//! The page loads in a hidden window that is sealed off from the app: it has
//! its own throwaway profile (no cookies or logins from anywhere), it can't
//! open windows or download, it only navigates to web and file addresses, and
//! it can't reach PDFRivet's commands or its `rivet://` page images (see
//! [`is_render_window`], checked in `lib.rs`).

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc,
    },
    time::Duration,
};

use rivet_core::{Error, ErrorCode};
use serde::Deserialize;
use tauri::{
    AppHandle, Runtime, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    webview::{NewWindowResponse, PageLoadEvent},
};

/// Windows that render web pages for printing have labels starting with this.
const LABEL_PREFIX: &str = "html-render-";

/// The window (webview) belongs to HTML to PDF, so it gets nothing from the app.
pub(crate) fn is_render_window(label: &str) -> bool {
    label.starts_with(LABEL_PREFIX)
}

/// What to turn into a PDF.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum HtmlSource {
    File { path: PathBuf },
    Url { url: String },
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum HtmlPaper {
    A4,
    Letter,
}

/// How the pages are printed.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HtmlOptions {
    paper: HtmlPaper,
    landscape: bool,
    /// Margin on every side, in millimetres.
    margin_mm: f32,
    /// Print background colours and pictures.
    backgrounds: bool,
}

// (Linux sets the paper by name and margins in millimetres.)
#[cfg_attr(target_os = "linux", allow(dead_code))]
impl HtmlOptions {
    /// Paper width and height in inches (portrait; `landscape` turns it).
    fn inches(&self) -> (f64, f64) {
        match self.paper {
            HtmlPaper::A4 => (8.27, 11.69),
            HtmlPaper::Letter => (8.5, 11.0),
        }
    }

    fn margin_inches(&self) -> f64 {
        f64::from(self.margin_mm.clamp(0.0, 50.0)) / 25.4
    }
}

/// A page can take this long to load before giving up.
const LOAD_TIMEOUT: Duration = Duration::from_secs(45);
/// After loading, a moment for late fonts, pictures and scripts.
const SETTLE: Duration = Duration::from_millis(1500);
/// Printing can take this long.
const PRINT_TIMEOUT: Duration = Duration::from_secs(120);

fn failed(detail: impl Into<String>) -> Error {
    Error::new(ErrorCode::HtmlFailed, detail)
}

/// The address to load: a local file, or a web address (http/https only).
fn address(source: &HtmlSource) -> Result<Url, Error> {
    match source {
        HtmlSource::File { path } => Url::from_file_path(path)
            .map_err(|_| Error::new(ErrorCode::FileNotFound, path.display().to_string())),
        HtmlSource::Url { url } => {
            let text = url.trim();
            // "example.com" means a web page.
            let text = if text.contains("://") {
                text.to_owned()
            } else {
                format!("https://{text}")
            };
            let parsed =
                Url::parse(&text).map_err(|e| Error::new(ErrorCode::InvalidUrl, e.to_string()))?;
            match parsed.scheme() {
                "http" | "https" => Ok(parsed),
                other => Err(Error::new(
                    ErrorCode::InvalidUrl,
                    format!("{other}: addresses aren't allowed"),
                )),
            }
        }
    }
}

/// Loads the page in a sealed-off hidden window and prints it to `path`.
#[tauri::command]
pub(crate) async fn html_to_pdf(
    app: AppHandle,
    source: HtmlSource,
    options: HtmlOptions,
    path: PathBuf,
) -> Result<(), Error> {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    let url = address(&source)?;
    let label = format!("{LABEL_PREFIX}{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let (loaded_tx, loaded_rx) = mpsc::channel();
    let window = WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(url))
        .title("PDFRivet")
        .visible(false)
        .focused(false)
        .skip_taskbar(true)
        .inner_size(1024.0, 1400.0)
        // A throwaway profile: no cookies, logins or storage, kept or shared.
        .incognito(true)
        .on_navigation(|url| {
            matches!(
                url.scheme(),
                "http" | "https" | "file" | "about" | "data" | "blob"
            )
        })
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .on_page_load(move |_, payload| {
            if payload.event() == PageLoadEvent::Finished {
                let _ = loaded_tx.send(());
            }
        })
        .build()
        .map_err(|e| failed(e.to_string()))?;

    let result = print_when_loaded(&window, loaded_rx, options, &path).await;
    let _ = window.destroy();
    result
}

async fn print_when_loaded<R: Runtime>(
    window: &WebviewWindow<R>,
    loaded: mpsc::Receiver<()>,
    options: HtmlOptions,
    path: &Path,
) -> Result<(), Error> {
    wait(move || {
        loaded
            .recv_timeout(LOAD_TIMEOUT)
            .map_err(|_| Error::new(ErrorCode::HtmlTimeout, "the page didn't finish loading"))
    })
    .await?;
    tokio_sleep(SETTLE).await;

    // Printed next to the target first, then moved into place.
    let temp = path.with_file_name(format!(
        ".{}.rivet-saving",
        path.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default()
    ));
    let (done_tx, done_rx) = mpsc::channel::<Result<(), String>>();
    platform::print(window, options, &temp, done_tx)?;
    wait(move || {
        done_rx
            .recv_timeout(PRINT_TIMEOUT)
            .map_err(|_| failed("printing took too long"))?
            .map_err(failed)
    })
    .await
    .inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })?;
    std::fs::rename(&temp, path).map_err(|e| Error::new(ErrorCode::SaveFailed, e.to_string()))
}

/// Waits for a blocking channel off the async runtime.
async fn wait<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, Error> + Send + 'static,
) -> Result<T, Error> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| Error::new(ErrorCode::Internal, e.to_string()))?
}

async fn tokio_sleep(duration: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(duration)).await;
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod platform {
    //! WebView2's own "print to PDF".
    use super::*;
    use webview2_com::{
        Microsoft::Web::WebView2::Win32::{
            COREWEBVIEW2_PRINT_ORIENTATION_LANDSCAPE, COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT,
            ICoreWebView2_7, ICoreWebView2Environment6,
        },
        PrintToPdfCompletedHandler,
    };
    use windows::core::{HSTRING, Interface};

    pub(super) fn print<R: Runtime>(
        window: &WebviewWindow<R>,
        options: HtmlOptions,
        path: &Path,
        done: mpsc::Sender<Result<(), String>>,
    ) -> Result<(), Error> {
        let path = HSTRING::from(path.as_os_str());
        window
            .with_webview(move |webview| {
                let report = done.clone();
                let started = (|| -> windows::core::Result<()> {
                    // SAFETY: COM calls on the webview's own (main) thread, with
                    // interfaces WebView2 handed us; everything stays alive for the call.
                    unsafe {
                        let core = webview.controller().CoreWebView2()?;
                        let core: ICoreWebView2_7 = core.cast()?;
                        let environment: ICoreWebView2Environment6 =
                            webview.environment().cast()?;
                        let settings = environment.CreatePrintSettings()?;
                        let (width, height) = options.inches();
                        let margin = options.margin_inches();
                        settings.SetOrientation(if options.landscape {
                            COREWEBVIEW2_PRINT_ORIENTATION_LANDSCAPE
                        } else {
                            COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT
                        })?;
                        settings.SetPageWidth(width)?;
                        settings.SetPageHeight(height)?;
                        settings.SetMarginTop(margin)?;
                        settings.SetMarginBottom(margin)?;
                        settings.SetMarginLeft(margin)?;
                        settings.SetMarginRight(margin)?;
                        settings.SetShouldPrintBackgrounds(options.backgrounds)?;
                        settings.SetShouldPrintHeaderAndFooter(false)?;
                        let handler =
                            PrintToPdfCompletedHandler::create(Box::new(move |result, ok| {
                                let _ = report.send(match (result, ok) {
                                    (Ok(()), true) => Ok(()),
                                    (Err(e), _) => Err(e.to_string()),
                                    (Ok(()), false) => {
                                        Err("WebView2 couldn't print the page".into())
                                    }
                                });
                                Ok(())
                            }));
                        core.PrintToPdf(&path, &settings, &handler)
                    }
                })();
                if let Err(e) = started {
                    let _ = done.send(Err(e.to_string()));
                }
            })
            .map_err(|e| failed(e.to_string()))
    }
}

#[cfg(target_os = "linux")]
mod platform {
    //! WebKitGTK printing to a PDF file through GTK's "Print to File".
    use super::*;
    use gtk::{PageOrientation, PageSetup, PaperSize, PrintSettings, Unit};
    use webkit2gtk::{PrintOperation, PrintOperationExt, SettingsExt, WebViewExt};

    pub(super) fn print<R: Runtime>(
        window: &WebviewWindow<R>,
        options: HtmlOptions,
        path: &Path,
        done: mpsc::Sender<Result<(), String>>,
    ) -> Result<(), Error> {
        let uri = Url::from_file_path(path)
            .map_err(|_| failed("bad output path"))?
            .to_string();
        window
            .with_webview(move |webview| {
                let view = webview.inner();
                if let Some(settings) = view.settings() {
                    settings.set_print_backgrounds(options.backgrounds);
                }
                let print_settings = PrintSettings::new();
                print_settings.set_printer("Print to File");
                print_settings.set(gtk::PRINT_SETTINGS_OUTPUT_FILE_FORMAT, Some("pdf"));
                print_settings.set(gtk::PRINT_SETTINGS_OUTPUT_URI, Some(&uri));
                let setup = PageSetup::new();
                let paper = match options.paper {
                    HtmlPaper::A4 => "iso_a4",
                    HtmlPaper::Letter => "na_letter",
                };
                setup.set_paper_size(&PaperSize::new(Some(paper)));
                setup.set_orientation(if options.landscape {
                    PageOrientation::Landscape
                } else {
                    PageOrientation::Portrait
                });
                let margin = f64::from(options.margin_mm.clamp(0.0, 50.0));
                setup.set_top_margin(margin, Unit::Mm);
                setup.set_bottom_margin(margin, Unit::Mm);
                setup.set_left_margin(margin, Unit::Mm);
                setup.set_right_margin(margin, Unit::Mm);
                let operation = PrintOperation::new(&view);
                operation.set_print_settings(&print_settings);
                operation.set_page_setup(&setup);
                let finished = done.clone();
                operation.connect_finished(move |_| {
                    let _ = finished.send(Ok(()));
                });
                operation.connect_failed(move |_, error| {
                    let _ = done.send(Err(error.to_string()));
                });
                operation.print();
            })
            .map_err(|e| failed(e.to_string()))
    }
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod platform {
    //! WKWebView's print operation, saved to a file without any panel.
    use super::*;
    use objc2::{rc::Retained, runtime::NSObjectProtocol, sel};
    use objc2_app_kit::{NSPaperOrientation, NSPrintInfo, NSPrintJobSavingURL, NSPrintSaveJob};
    use objc2_foundation::{NSSize, NSString, NSURL};
    use objc2_web_kit::WKWebView;

    pub(super) fn print<R: Runtime>(
        window: &WebviewWindow<R>,
        options: HtmlOptions,
        path: &Path,
        done: mpsc::Sender<Result<(), String>>,
    ) -> Result<(), Error> {
        let path = path.to_string_lossy().into_owned();
        window
            .with_webview(move |webview| {
                // SAFETY: Tauri hands us its live WKWebView; we're on the main
                // thread, where AppKit and WebKit must be used.
                let result = unsafe {
                    let view: &WKWebView = &*webview.inner().cast();
                    // Background colours and pictures (macOS 13.3 and later).
                    let preferences = view.configuration().preferences();
                    if preferences.respondsToSelector(sel!(setShouldPrintBackgrounds:)) {
                        preferences.setShouldPrintBackgrounds(options.backgrounds);
                    }
                    let info = NSPrintInfo::new();
                    let (width, height) = options.inches();
                    info.setPaperSize(NSSize::new(width * 72.0, height * 72.0));
                    info.setOrientation(if options.landscape {
                        NSPaperOrientation::Landscape
                    } else {
                        NSPaperOrientation::Portrait
                    });
                    let margin = options.margin_inches() * 72.0;
                    info.setTopMargin(margin);
                    info.setBottomMargin(margin);
                    info.setLeftMargin(margin);
                    info.setRightMargin(margin);
                    info.setJobDisposition(NSPrintSaveJob);
                    let url: Retained<NSURL> = NSURL::fileURLWithPath(&NSString::from_str(&path));
                    info.dictionary().insert(NSPrintJobSavingURL, &*url);
                    let operation = view.printOperationWithPrintInfo(&info);
                    operation.setShowsPrintPanel(false);
                    operation.setShowsProgressPanel(false);
                    operation.runOperation()
                };
                let _ = done.send(if result {
                    Ok(())
                } else {
                    Err("WebKit couldn't print the page".into())
                });
            })
            .map_err(|e| failed(e.to_string()))
    }
}
