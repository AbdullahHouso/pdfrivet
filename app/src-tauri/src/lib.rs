//! PDFRivet desktop app: a thin Tauri layer over `rivet-core`.
//!
//! - Commands (`open_document`, `get_outline`, …) are called from the UI with `invoke`.
//! - Page images are served by the `rivet://` protocol (see [`page_protocol`]),
//!   which sends raw pixels instead of encoded PNGs for speed.
//! - Files opened from the OS ("Open with", double-click, a second launch) are
//!   queued and announced to the UI with the `open-files` event.

#[cfg(windows)]
mod taskbar_tabs;

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use rivet_core::{
    DocId, DocInfo, Engine, Error, ErrorCode, FieldChange, FormField, OutlineItem, PageLink,
    Rotation,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, http};

/// App-wide state. The engine is `None` if PDFium failed to load at startup.
struct AppState {
    engine: Option<Engine>,
    startup_error: Option<String>,
    /// PDFs passed in by the OS before the UI was ready to receive them.
    pending_files: Mutex<Vec<PathBuf>>,
}

impl AppState {
    fn engine(&self) -> Result<&Engine, Error> {
        self.engine.as_ref().ok_or_else(|| {
            Error::new(
                ErrorCode::LibraryNotFound,
                self.startup_error.clone().unwrap_or_default(),
            )
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenedDocument {
    doc_id: DocId,
    info: DocInfo,
}

/// Runs a blocking engine call off the async runtime.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, Error> + Send + 'static,
) -> Result<T, Error> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| Error::new(ErrorCode::Internal, e.to_string()))?
}

#[tauri::command]
async fn open_document(
    path: PathBuf,
    password: Option<String>,
    state: State<'_, AppState>,
) -> Result<OpenedDocument, Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.open(&path, password))
        .await
        .map(|(doc_id, info)| OpenedDocument { doc_id, info })
}

#[tauri::command]
async fn get_outline(doc_id: DocId, state: State<'_, AppState>) -> Result<Vec<OutlineItem>, Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.outline(doc_id)).await
}

#[tauri::command]
async fn get_links(
    doc_id: DocId,
    page: u32,
    state: State<'_, AppState>,
) -> Result<Vec<PageLink>, Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.links(doc_id, page)).await
}

#[tauri::command]
async fn get_form_fields(
    doc_id: DocId,
    page: u32,
    state: State<'_, AppState>,
) -> Result<Vec<FormField>, Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.form_fields(doc_id, page)).await
}

#[tauri::command]
async fn change_field(
    doc_id: DocId,
    page: u32,
    field: u32,
    change: FieldChange,
    state: State<'_, AppState>,
) -> Result<(), Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.change_field(doc_id, page, field, change)).await
}

#[tauri::command]
async fn save_document(
    doc_id: DocId,
    path: PathBuf,
    state: State<'_, AppState>,
) -> Result<(), Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.save(doc_id, &path)).await
}

#[tauri::command]
async fn document_properties(
    doc_id: DocId,
    state: State<'_, AppState>,
) -> Result<rivet_core::metadata::DocProperties, Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.properties(doc_id)).await
}

#[tauri::command]
async fn set_metadata(
    doc_id: DocId,
    metadata: rivet_core::metadata::Metadata,
    state: State<'_, AppState>,
) -> Result<(), Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.set_metadata(doc_id, metadata)).await
}

#[tauri::command]
async fn list_printers() -> Result<Vec<rivet_core::print::PrinterInfo>, Error> {
    blocking(rivet_core::print::list_printers).await
}

/// Where a page lands on the paper, for the print preview (same maths as printing).
#[tauri::command]
fn print_placement(
    page_width: f32,
    page_height: f32,
    paper_width_mm: f32,
    paper_height_mm: f32,
    scaling: rivet_core::print::Scaling,
    orientation: rivet_core::print::Orientation,
) -> rivet_core::print::Placement {
    use rivet_core::print::{DEFAULT_MARGIN_PT, MM_TO_PT, place_page};
    place_page(
        page_width,
        page_height,
        paper_width_mm * MM_TO_PT,
        paper_height_mm * MM_TO_PT,
        DEFAULT_MARGIN_PT,
        scaling,
        orientation,
    )
}

/// Shows the printer driver's own settings window (Windows). Returns `null` if
/// cancelled.
#[cfg(windows)]
#[tauri::command]
async fn printer_properties(
    window: tauri::WebviewWindow,
    printer: String,
    current: Option<Vec<u8>>,
) -> Result<Option<serde_json::Value>, Error> {
    let hwnd = window
        .hwnd()
        .map_err(|e| Error::new(ErrorCode::Internal, e.to_string()))?
        .0 as isize;
    blocking(move || {
        rivet_core::print::printer_properties(hwnd, &printer, current.as_deref())
            .map(|p| p.map(|p| serde_json::to_value(p).unwrap_or_default()))
    })
    .await
}

/// Other systems have no driver settings window; the dialog hides the button.
#[cfg(not(windows))]
#[tauri::command]
async fn printer_properties(
    _window: tauri::WebviewWindow,
    _printer: String,
    _current: Option<Vec<u8>>,
) -> Result<Option<serde_json::Value>, Error> {
    Ok(None)
}

#[tauri::command]
async fn print_document(
    doc_id: DocId,
    settings: rivet_core::print::PrintSettings,
    printer_settings: Option<Vec<u8>>,
    state: State<'_, AppState>,
) -> Result<(), Error> {
    let engine = state.engine()?.clone();
    blocking(move || engine.print(doc_id, settings, printer_settings)).await
}

#[tauri::command]
fn set_visible_pages(
    doc_id: DocId,
    first: u32,
    last: u32,
    state: State<'_, AppState>,
) -> Result<(), Error> {
    state.engine()?.set_visible_pages(doc_id, first, last);
    Ok(())
}

#[tauri::command]
fn close_document(doc_id: DocId, state: State<'_, AppState>) -> Result<(), Error> {
    state.engine()?.close(doc_id);
    Ok(())
}

/// Returns (and clears) the PDFs the OS asked us to open before the UI was ready.
#[tauri::command]
fn take_pending_files(state: State<'_, AppState>) -> Vec<PathBuf> {
    state
        .pending_files
        .lock()
        .map(|mut files| std::mem::take(&mut *files))
        .unwrap_or_default()
}

/// Which of the given paths still exist (for the recent-files list).
#[tauri::command]
fn files_exist(paths: Vec<PathBuf>) -> Vec<bool> {
    paths.iter().map(|p| p.is_file()).collect()
}

/// Keeps only existing files from command-line style arguments.
fn files_from_args(args: impl IntoIterator<Item = String>, cwd: &Path) -> Vec<PathBuf> {
    args.into_iter()
        .filter(|a| !a.starts_with('-'))
        .map(|a| {
            let p = PathBuf::from(a);
            if p.is_absolute() { p } else { cwd.join(p) }
        })
        .filter(|p| p.is_file())
        .collect()
}

/// Windows: shows each tab as its own preview in the taskbar (see `taskbar_tabs`).
/// `enabled` is false when the setting is off or documents open in separate windows.
#[cfg(windows)]
#[tauri::command]
fn set_taskbar_tabs(
    window: tauri::WebviewWindow,
    tabs: Vec<taskbar_tabs::TaskbarTab>,
    active: Option<u32>,
    enabled: bool,
) {
    let target = window.clone();
    let _ = window.run_on_main_thread(move || taskbar_tabs::sync(&target, tabs, active, enabled));
}

/// Taskbar previews per tab are a Windows feature; elsewhere this does nothing.
#[cfg(not(windows))]
#[tauri::command]
fn set_taskbar_tabs(_tabs: Vec<serde_json::Value>, _active: Option<u32>, _enabled: bool) {}

/// Hands files to the UI: queued for `take_pending_files`, plus an event for a running UI.
///
/// With several windows open ("Separate windows" setting), only one is told, so
/// the files don't open twice: the focused window, else the main one, else any.
fn open_files(app: &AppHandle, files: Vec<PathBuf>) {
    if files.is_empty() {
        return;
    }
    if let Ok(mut pending) = app.state::<AppState>().pending_files.lock() {
        pending.extend(files.iter().cloned());
    }
    let windows = app.webview_windows();
    let target = windows
        .values()
        .find(|w| w.is_focused().unwrap_or(false))
        .or_else(|| windows.get("main"))
        .or_else(|| windows.values().next());
    if let Some(window) = target {
        let _ = window.emit_to(window.label(), "open-files", ());
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Finds the PDFium library: bundled next to the app in release builds,
/// or in `vendor/pdfium/<platform>` (from `cargo xtask fetch-pdfium`) during development.
fn pdfium_dir(app: &tauri::App) -> PathBuf {
    let bundled = app.path().resource_dir().map(|d| d.join("pdfium"));
    match bundled {
        Ok(dir) if dir.is_dir() => dir,
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../vendor/pdfium")
            .join(rivet_core::pdfium_platform()),
    }
}

/// Handles `rivet://localhost/page/<doc>/<page>?scale=<f32>&rot=<degrees>[&thumb=1][&cached=1]`.
///
/// The body is an 8-byte header (width and height as little-endian u32)
/// followed by raw RGBA pixels, ready for `ImageData` on a canvas.
/// Pages that scrolled out of view get an empty `204 No Content`, and so do
/// `cached=1` requests for pages that aren't rendered at that size yet.
fn page_protocol(engine: &Engine, uri: &http::Uri) -> http::Response<Vec<u8>> {
    let parts: Vec<&str> = uri.path().trim_matches('/').split('/').collect();
    let query = |name: &str| {
        uri.query()?
            .split('&')
            .find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='))
    };
    let scale = query("scale")
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(1.0);
    let rotation = Rotation::from_degrees(query("rot").and_then(|s| s.parse().ok()).unwrap_or(0));

    // Printing: /print/<doc>/<page>?dpi=<n> returns a JPEG of the upright page.
    if let ["print", doc, page] = parts.as_slice() {
        let dpi = query("dpi")
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(200.0)
            .clamp(72.0, 600.0);
        return print_page(engine, doc, page, dpi);
    }

    let result = match parts.as_slice() {
        ["page", doc, page] => match (doc.parse(), page.parse()) {
            (Ok(doc), Ok(page)) if query("cached").is_some() => {
                engine.cached(doc, page, scale, rotation)
            }
            (Ok(doc), Ok(page)) if query("thumb").is_some() => {
                engine.render_thumbnail(doc, page, scale, rotation)
            }
            (Ok(doc), Ok(page)) => engine.render(doc, page, scale, rotation),
            _ => Err(Error::new(ErrorCode::Internal, "bad page URL")),
        },
        _ => Err(Error::new(ErrorCode::Internal, "unknown path")),
    };

    // Set RIVET_DEBUG=1 to log every page request (handy when pages don't appear).
    if std::env::var_os("RIVET_DEBUG").is_some() {
        match &result {
            Ok(p) => eprintln!("[rivet] {uri} -> {}x{}", p.width, p.height),
            Err(e) => eprintln!("[rivet] {uri} -> {:?}", e.code),
        }
    }

    let builder = http::Response::builder()
        // The page is fetched from the app's own origin, which differs from rivet://.
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "no-store");
    match result {
        Ok(page) => {
            let mut body = Vec::with_capacity(8 + page.rgba.len());
            body.extend_from_slice(&page.width.to_le_bytes());
            body.extend_from_slice(&page.height.to_le_bytes());
            body.extend_from_slice(&page.rgba);
            builder
                .header("Content-Type", "application/octet-stream")
                .body(body)
        }
        Err(e) if e.code == ErrorCode::Cancelled => builder
            .status(http::StatusCode::NO_CONTENT)
            .body(Vec::new()),
        Err(e) => builder
            .status(http::StatusCode::BAD_REQUEST)
            .header("Content-Type", "application/json")
            .body(serde_json::to_vec(&e).unwrap_or_default()),
    }
    .unwrap_or_else(|_| http::Response::new(Vec::new()))
}

/// Renders a page for printing and encodes it as JPEG (much smaller than raw
/// pixels, so even long documents fit in memory while the print dialog is open).
fn print_page(engine: &Engine, doc: &str, page: &str, dpi: f32) -> http::Response<Vec<u8>> {
    let rendered = match (doc.parse(), page.parse()) {
        (Ok(doc), Ok(page)) => engine.render_for_print(doc, page, dpi / 72.0),
        _ => Err(Error::new(ErrorCode::Internal, "bad print URL")),
    };
    let jpeg = rendered.and_then(|p| {
        // JPEG has no transparency; pages are rendered on white anyway.
        let (pixels, _) = p.rgba.as_chunks::<4>();
        let rgb: Vec<u8> = pixels.iter().flat_map(|px| [px[0], px[1], px[2]]).collect();
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92)
            .encode(&rgb, p.width, p.height, image::ExtendedColorType::Rgb8)
            .map_err(|e| Error::new(ErrorCode::Internal, e.to_string()))?;
        Ok(out)
    });
    let builder = http::Response::builder()
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "no-store");
    match jpeg {
        Ok(bytes) => builder.header("Content-Type", "image/jpeg").body(bytes),
        Err(e) => builder
            .status(http::StatusCode::BAD_REQUEST)
            .header("Content-Type", "application/json")
            .body(serde_json::to_vec(&e).unwrap_or_default()),
    }
    .unwrap_or_else(|_| http::Response::new(Vec::new()))
}

/// Tells the UI the OS languages before it starts, so PDFRivet opens in the
/// user's language. (Some webviews don't report the OS language reliably.)
fn system_locales_script() -> String {
    let locales: Vec<String> = sys_locale::get_locales().collect();
    format!(
        "window.__RIVET_SYSTEM_LOCALES__ = {};",
        serde_json::to_string(&locales).unwrap_or_else(|_| "[]".into())
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be the first plugin: a second launch hands its files to this instance and exits.
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            let files = files_from_args(args.into_iter().skip(1), Path::new(&cwd));
            open_files(app, files);
        }))
        .plugin(tauri_plugin_dialog::init())
        // Opens web links from PDFs in the default browser (http, https and mailto only).
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        // Remembers the main window's size and position. Windows opened for single
        // documents ("Separate windows") are placed next to the one they came from.
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_filter(|label| label == "main")
                .build(),
        )
        // Restarts the app after an update.
        .plugin(tauri_plugin_process::init())
        .append_invoke_initialization_script(system_locales_script())
        .setup(|app| {
            // Checks for, downloads and installs signed updates (see `updater.svelte.ts`).
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;
            let (engine, startup_error) = match Engine::start(&pdfium_dir(app)) {
                Ok(engine) => (Some(engine), None),
                Err(e) => {
                    eprintln!("Failed to start PDFium: {e}");
                    (None, Some(e.to_string()))
                }
            };
            let cwd = std::env::current_dir().unwrap_or_default();
            let initial = files_from_args(std::env::args().skip(1), &cwd);
            app.manage(AppState {
                engine,
                startup_error,
                pending_files: Mutex::new(initial),
            });
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("rivet", |ctx, request, responder| {
            let Some(engine) = ctx.app_handle().state::<AppState>().engine.clone() else {
                responder.respond(
                    http::Response::builder()
                        .status(http::StatusCode::SERVICE_UNAVAILABLE)
                        .body(Vec::new())
                        .unwrap(),
                );
                return;
            };
            // Render on another thread so the webview never waits on PDFium.
            std::thread::spawn(move || responder.respond(page_protocol(&engine, request.uri())));
        })
        .invoke_handler(tauri::generate_handler![
            open_document,
            get_outline,
            get_links,
            get_form_fields,
            change_field,
            save_document,
            list_printers,
            document_properties,
            set_metadata,
            print_placement,
            printer_properties,
            print_document,
            set_visible_pages,
            close_document,
            take_pending_files,
            set_taskbar_tabs,
            files_exist
        ])
        .build(tauri::generate_context!())
        .expect("error while building PDFRivet")
        .run(|_app, _event| {
            // macOS delivers "Open with" files as an event instead of arguments.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = _event {
                let files = urls
                    .into_iter()
                    .filter_map(|u| u.to_file_path().ok())
                    .collect();
                open_files(_app, files);
            }
        });
}
