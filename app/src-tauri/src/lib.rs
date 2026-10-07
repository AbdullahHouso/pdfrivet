//! Rivet desktop app: a thin Tauri layer over `rivet-core`.
//!
//! - Commands (`open_document`, `get_outline`, …) are called from the UI with `invoke`.
//! - Page images are served by the `rivet://` protocol (see [`page_protocol`]),
//!   which sends raw pixels instead of encoded PNGs for speed.
//! - Files opened from the OS ("Open with", double-click, a second launch) are
//!   queued and announced to the UI with the `open-files` event.

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use rivet_core::{DocId, DocInfo, Engine, Error, ErrorCode, OutlineItem, Rotation};
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

/// Hands files to the UI: queued for `take_pending_files`, plus an event for a running UI.
fn open_files(app: &AppHandle, files: Vec<PathBuf>) {
    if files.is_empty() {
        return;
    }
    if let Ok(mut pending) = app.state::<AppState>().pending_files.lock() {
        pending.extend(files.iter().cloned());
    }
    let _ = app.emit("open-files", ());
    if let Some(window) = app.get_webview_window("main") {
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

/// Handles `rivet://localhost/page/<doc>/<page>?scale=<f32>&rot=<degrees>[&thumb=1]`.
///
/// The body is an 8-byte header (width and height as little-endian u32)
/// followed by raw RGBA pixels, ready for `ImageData` on a canvas.
/// Pages that scrolled out of view get an empty `204 No Content`.
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

    let result = match parts.as_slice() {
        ["page", doc, page] => match (doc.parse(), page.parse()) {
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

/// Tells the UI the OS languages before it starts, so Rivet opens in the
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
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .append_invoke_initialization_script(system_locales_script())
        .setup(|app| {
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
            set_visible_pages,
            close_document,
            take_pending_files,
            files_exist
        ])
        .build(tauri::generate_context!())
        .expect("error while building Rivet")
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
