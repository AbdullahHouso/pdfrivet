//! Rivet desktop app: a thin Tauri layer over `rivet-core`.
//!
//! - Commands (`open_document`, `close_document`) are called from the UI with `invoke`.
//! - Page images are served by the `rivet://` protocol (see [`page_protocol`]),
//!   which sends raw pixels instead of encoded PNGs for speed.

use std::{path::PathBuf, sync::Mutex};

use rivet_core::{DocId, DocInfo, Engine, Error, ErrorCode};
use serde::Serialize;
use tauri::{Manager, State, http};

/// App-wide state. The engine is `None` if PDFium failed to load at startup.
struct AppState {
    engine: Option<Engine>,
    startup_error: Option<String>,
    /// A PDF passed on the command line ("Open with Rivet"), opened once at startup.
    initial_file: Mutex<Option<PathBuf>>,
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

#[tauri::command]
async fn open_document(path: PathBuf, state: State<'_, AppState>) -> Result<OpenedDocument, Error> {
    let engine = state.engine()?.clone();
    // Opening can take a moment for big files; keep it off the async runtime.
    tauri::async_runtime::spawn_blocking(move || engine.open(&path, None))
        .await
        .map_err(|e| Error::new(ErrorCode::Internal, e.to_string()))?
        .map(|(doc_id, info)| OpenedDocument { doc_id, info })
}

#[tauri::command]
fn take_initial_file(state: State<'_, AppState>) -> Option<PathBuf> {
    state.initial_file.lock().ok()?.take()
}

#[tauri::command]
fn close_document(doc_id: DocId, state: State<'_, AppState>) -> Result<(), Error> {
    state.engine()?.close(doc_id);
    Ok(())
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

/// Handles `rivet://localhost/page/<doc>/<page>?scale=<f32>`.
///
/// The body is an 8-byte header (width and height as little-endian u32)
/// followed by raw RGBA pixels, ready for `ImageData` on a canvas.
fn page_protocol(engine: &Engine, uri: &http::Uri) -> http::Response<Vec<u8>> {
    let parts: Vec<&str> = uri.path().trim_matches('/').split('/').collect();
    let scale = uri
        .query()
        .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("scale=")))
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(1.0);

    let result = match parts.as_slice() {
        ["page", doc, page] => match (doc.parse(), page.parse()) {
            (Ok(doc), Ok(page)) => engine.render(doc, page, scale),
            _ => Err(Error::new(ErrorCode::Internal, "bad page URL")),
        },
        _ => Err(Error::new(ErrorCode::Internal, "unknown path")),
    };

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
        .append_invoke_initialization_script(system_locales_script())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let (engine, startup_error) = match Engine::start(&pdfium_dir(app)) {
                Ok(engine) => (Some(engine), None),
                Err(e) => {
                    eprintln!("Failed to start PDFium: {e}");
                    (None, Some(e.to_string()))
                }
            };
            let initial_file = std::env::args_os()
                .skip(1)
                .map(PathBuf::from)
                .find(|p| p.is_file());
            app.manage(AppState {
                engine,
                startup_error,
                initial_file: Mutex::new(initial_file),
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
            close_document,
            take_initial_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running Rivet");
}
