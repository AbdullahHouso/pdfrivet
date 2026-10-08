//! Windows only: each open tab gets its own preview in the taskbar, like
//! separate windows, while PDFRivet keeps one window with tabs.
//!
//! Windows does this with "proxy" windows: one invisible window per tab,
//! registered with the taskbar as a tab of the main window
//! (`ITaskbarList3::RegisterTab`). When the taskbar wants a preview, it asks
//! the proxy (`WM_DWMSENDICONICTHUMBNAIL` for the small preview,
//! `WM_DWMSENDICONICLIVEPREVIEWBITMAP` for the full-size "peek" on hover).
//!
//! Previews show the whole window, like separate windows do. Windows can only
//! capture what is on screen, so the window is captured whenever a tab has
//! been shown for a moment (`capture_active`), and background tabs use their
//! last capture. A tab that was never captured shows its current page instead.
//!
//! Clicking a preview activates its proxy, and closing it sends `WM_CLOSE`;
//! both are passed on to the UI as a `taskbar-tab` event.
//!
//! Everything here runs on the main (UI) thread, which owns these windows.
#![allow(unsafe_code)]

use std::{cell::RefCell, collections::HashMap};

use rivet_core::{Engine, Rotation};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::{
            Dwm::{
                DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_FORCE_ICONIC_REPRESENTATION,
                DWMWA_HAS_ICONIC_BITMAP, DwmGetWindowAttribute, DwmInvalidateIconicBitmaps,
                DwmSetIconicLivePreviewBitmap, DwmSetIconicThumbnail, DwmSetWindowAttribute,
            },
            Gdi::{
                BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
                DIB_RGB_COLORS, DeleteDC, DeleteObject, HALFTONE, HBITMAP, SRCCOPY, SelectObject,
                SetBrushOrgEx, SetStretchBltMode, StretchBlt,
            },
        },
        Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow},
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            LibraryLoader::GetModuleHandleW,
        },
        UI::{
            Shell::{ITaskbarList3, TaskbarList},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowRect, ICON_BIG,
                ICON_SMALL, IsIconic, PW_RENDERFULLCONTENT, RegisterClassW, SW_RESTORE,
                SendMessageW, SetForegroundWindow, SetWindowTextW, ShowWindow, WA_INACTIVE,
                WM_ACTIVATE, WM_CLOSE, WM_DWMSENDICONICLIVEPREVIEWBITMAP,
                WM_DWMSENDICONICTHUMBNAIL, WM_GETICON, WM_SETICON, WNDCLASSW, WS_BORDER,
                WS_CAPTION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP, WS_SYSMENU,
            },
        },
    },
    core::{BOOL, HSTRING, w},
};

use crate::AppState;

/// Captures of background tabs are kept at most this big (about 2 MB each).
const SNAPSHOT_MAX: i32 = 960;

/// One tab, as the UI describes it.
#[derive(Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskbarTab {
    pub id: u32,
    pub title: String,
    pub doc_id: u32,
    pub page: u32,
    /// The page's size on screen, in PDF points (after rotation).
    pub width_pt: f32,
    pub height_pt: f32,
    pub rotation: i32,
}

/// Sent to the UI when a tab's taskbar preview is clicked or closed.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TaskbarTabEvent {
    action: &'static str,
    id: u32,
}

/// An image of the window: top-down BGRA pixels, fully opaque.
#[derive(Clone)]
struct Image {
    width: i32,
    height: i32,
    bgra: Vec<u8>,
}

struct Proxy {
    hwnd: HWND,
    tab: TaskbarTab,
}

#[derive(Default)]
struct Taskbar {
    app: Option<AppHandle>,
    main: HWND,
    /// The window holding the tabs (usually "main"; any window after switching back to tabs).
    label: String,
    list: Option<ITaskbarList3>,
    proxies: Vec<Proxy>,
    active: Option<u32>,
    /// The last capture of each tab, shown while the tab is in the background.
    snapshots: HashMap<u32, Image>,
}

thread_local! {
    static TASKBAR: RefCell<Taskbar> = RefCell::new(Taskbar::default());
}

const CLASS_NAME: windows::core::PCWSTR = w!("PDFRivetTaskbarTab");

/// Shows `tabs` (in this order) in the taskbar, with `active` highlighted.
/// Fewer than two tabs, or `enabled == false`, goes back to one normal entry.
/// Must run on the main thread.
pub fn sync(window: &WebviewWindow, tabs: Vec<TaskbarTab>, active: Option<u32>, enabled: bool) {
    let Ok(main) = window.hwnd() else { return };
    let tabs = if enabled && tabs.len() >= 2 {
        tabs
    } else {
        Vec::new()
    };
    TASKBAR.with(|cell| {
        let Ok(mut taskbar) = cell.try_borrow_mut() else {
            return;
        };
        taskbar.app = Some(window.app_handle().clone());
        taskbar.main = main;
        taskbar.label = window.label().to_owned();
        taskbar.active = active;
        taskbar
            .snapshots
            .retain(|id, _| tabs.iter().any(|t| t.id == *id));
        if taskbar.list.is_none() && !tabs.is_empty() {
            // SAFETY: COM is initialized on the main thread by the window library.
            taskbar.list = unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER) }
                .ok()
                .filter(|list: &ITaskbarList3| unsafe { list.HrInit() }.is_ok());
        }
        let Some(list) = taskbar.list.clone() else {
            return;
        };

        // Remove proxies of closed tabs.
        let mut kept = Vec::new();
        for proxy in std::mem::take(&mut taskbar.proxies) {
            if tabs.iter().any(|t| t.id == proxy.tab.id) {
                kept.push(proxy);
            } else {
                // SAFETY: the proxy window was created by us on this thread.
                unsafe {
                    let _ = list.UnregisterTab(proxy.hwnd);
                    let _ = DestroyWindow(proxy.hwnd);
                }
            }
        }

        // Create or update a proxy per tab, then put them in the tabs' order.
        let mut ordered = Vec::new();
        for tab in tabs {
            let proxy = match kept.iter().position(|p| p.tab.id == tab.id) {
                Some(i) => {
                    let mut proxy = kept.remove(i);
                    if proxy.tab.title != tab.title {
                        // SAFETY: valid window handle owned by this thread.
                        unsafe {
                            let _ = SetWindowTextW(proxy.hwnd, &HSTRING::from(tab.title.as_str()));
                        }
                    }
                    proxy.tab = tab;
                    proxy
                }
                None => match create_proxy(main, &tab) {
                    Some(hwnd) => {
                        // SAFETY: both windows exist; the taskbar keeps no ownership.
                        unsafe {
                            let _ = list.RegisterTab(hwnd, main);
                        }
                        Proxy { hwnd, tab }
                    }
                    None => continue,
                },
            };
            ordered.push(proxy);
        }
        for proxy in &ordered {
            // SAFETY: registered tab windows; a null "insert before" appends at the end.
            unsafe {
                let _ = list.SetTabOrder(proxy.hwnd, HWND::default());
            }
        }
        if let Some(proxy) = ordered.iter().find(|p| Some(p.tab.id) == active) {
            // SAFETY: as above.
            unsafe {
                let _ = list.SetTabActive(proxy.hwnd, main, 0);
            }
        }
        taskbar.proxies = ordered;
    });
}

/// Captures the window for the active tab's preview. The UI calls this once a
/// tab has been shown for a moment (pages rendered). Must run on the main thread.
pub fn capture_active() {
    let target = TASKBAR.with(|cell| {
        let taskbar = cell.try_borrow().ok()?;
        let active = taskbar.active?;
        let proxy = taskbar.proxies.iter().find(|p| p.tab.id == active)?.hwnd;
        Some((taskbar.main, active, proxy))
    });
    let Some((main, active, proxy)) = target else {
        return;
    };
    let Some(image) = capture_window(main) else {
        return;
    };
    let small = fit(&image, SNAPSHOT_MAX, SNAPSHOT_MAX, false);
    TASKBAR.with(|cell| {
        if let Ok(mut taskbar) = cell.try_borrow_mut() {
            taskbar.snapshots.insert(active, small);
        }
    });
    // SAFETY: valid proxy window; asks the taskbar to fetch the new preview.
    unsafe {
        let _ = DwmInvalidateIconicBitmaps(proxy);
    }
}

/// Creates the invisible window that stands for one tab in the taskbar.
fn create_proxy(main: HWND, tab: &TaskbarTab) -> Option<HWND> {
    // SAFETY: standard window-class registration and creation on the UI thread.
    // Registering the class again after the first time fails harmlessly.
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(proxy_proc),
            hInstance: instance.into(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            CLASS_NAME,
            &HSTRING::from(tab.title.as_str()),
            WS_POPUP | WS_BORDER | WS_SYSMENU | WS_CAPTION,
            -32000,
            -32000,
            10,
            10,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .ok()?;
        // The window itself is never shown: its previews come from us.
        let on = BOOL::from(true);
        let size = size_of::<BOOL>() as u32;
        let pointer = (&on as *const BOOL).cast();
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_FORCE_ICONIC_REPRESENTATION, pointer, size);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_HAS_ICONIC_BITMAP, pointer, size);
        // The app icon, next to the tab's title in the preview.
        for kind in [ICON_SMALL, ICON_BIG] {
            let icon = SendMessageW(main, WM_GETICON, Some(WPARAM(kind as usize)), None);
            SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(kind as usize)),
                Some(LPARAM(icon.0)),
            );
        }
        Some(hwnd)
    }
}

/// Handles the taskbar's messages to a tab's proxy window.
unsafe extern "system" fn proxy_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DWMSENDICONICTHUMBNAIL => {
            // The largest preview the taskbar can show: width in the high word.
            let max_width = ((lparam.0 >> 16) & 0xffff) as i32;
            let max_height = (lparam.0 & 0xffff) as i32;
            send_thumbnail(hwnd, max_width, max_height);
            LRESULT(0)
        }
        WM_DWMSENDICONICLIVEPREVIEWBITMAP => {
            send_live_preview(hwnd);
            LRESULT(0)
        }
        WM_ACTIVATE if (wparam.0 & 0xffff) as u32 != WA_INACTIVE => {
            notify(hwnd, "activate");
            LRESULT(0)
        }
        WM_CLOSE => {
            // The UI closes the tab (asking about unsaved changes first) and
            // then removes this proxy.
            notify(hwnd, "close");
            LRESULT(0)
        }
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// What a proxy stands for: the tab, whether it's the one on screen, and its last capture.
struct Lookup {
    app: AppHandle,
    main: HWND,
    label: String,
    tab: TaskbarTab,
    is_active: bool,
    snapshot: Option<Image>,
}

fn lookup(hwnd: HWND) -> Option<Lookup> {
    TASKBAR.with(|cell| {
        let taskbar = cell.try_borrow().ok()?;
        let tab = taskbar.proxies.iter().find(|p| p.hwnd == hwnd)?.tab.clone();
        Some(Lookup {
            app: taskbar.app.clone()?,
            main: taskbar.main,
            label: taskbar.label.clone(),
            is_active: taskbar.active == Some(tab.id),
            snapshot: taskbar.snapshots.get(&tab.id).cloned(),
            tab,
        })
    })
}

/// The window as it looks with this tab showing: captured now if the tab is on
/// screen, else its last capture.
fn current_image(found: &Lookup) -> Option<Image> {
    if found.is_active
        && let Some(image) = capture_window(found.main)
    {
        let small = fit(&image, SNAPSHOT_MAX, SNAPSHOT_MAX, false);
        TASKBAR.with(|cell| {
            if let Ok(mut taskbar) = cell.try_borrow_mut() {
                taskbar.snapshots.insert(found.tab.id, small);
            }
        });
        return Some(image);
    }
    found.snapshot.clone()
}

/// The small preview above the taskbar button.
fn send_thumbnail(hwnd: HWND, max_width: i32, max_height: i32) {
    let Some(found) = lookup(hwnd) else { return };
    let image = current_image(&found)
        .map(|image| fit(&image, max_width, max_height, false))
        .or_else(|| page_image(&found, max_width, max_height));
    if let Some(bitmap) = image.as_ref().and_then(to_bitmap) {
        // SAFETY: valid proxy window and bitmap; DWM copies the bitmap.
        unsafe {
            let _ = DwmSetIconicThumbnail(hwnd, bitmap, 0);
            let _ = DeleteObject(bitmap.into());
        }
    }
}

/// The full-size "peek" shown in place of the window while hovering a preview.
fn send_live_preview(hwnd: HWND) {
    let Some(found) = lookup(hwnd) else { return };
    let Some(image) = current_image(&found) else {
        return;
    };
    let Some(bounds) = visible_bounds(found.main) else {
        return;
    };
    // Background tabs' captures are kept small; stretch them to the window's size.
    let width = bounds.right - bounds.left;
    let height = bounds.bottom - bounds.top;
    let image = if (image.width, image.height) == (width, height) {
        image
    } else {
        stretch(&image, width, height)
    };
    if let Some(bitmap) = to_bitmap(&image) {
        // SAFETY: valid proxy window and bitmap; DWM copies the bitmap.
        unsafe {
            let _ = DwmSetIconicLivePreviewBitmap(hwnd, bitmap, None, 0);
            let _ = DeleteObject(bitmap.into());
        }
    }
}

/// Tells the UI about a click on a preview, and brings the main window forward.
fn notify(hwnd: HWND, action: &'static str) {
    let Some(found) = lookup(hwnd) else { return };
    let _ = found.app.emit_to(
        found.label.as_str(),
        "taskbar-tab",
        TaskbarTabEvent {
            action,
            id: found.tab.id,
        },
    );
    if action == "activate" {
        // SAFETY: valid main window handle.
        unsafe {
            if IsIconic(found.main).as_bool() {
                let _ = ShowWindow(found.main, SW_RESTORE);
            }
            let _ = SetForegroundWindow(found.main);
        }
    }
}

/// The window's visible rectangle on screen (without Windows' invisible
/// resize borders).
fn visible_bounds(main: HWND) -> Option<RECT> {
    let mut rect = RECT::default();
    // SAFETY: valid window and a RECT-sized out parameter.
    unsafe {
        DwmGetWindowAttribute(
            main,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut rect as *mut RECT).cast(),
            size_of::<RECT>() as u32,
        )
        .ok()?;
    }
    (rect.right > rect.left && rect.bottom > rect.top).then_some(rect)
}

/// Captures the whole window (title bar included), as on screen. Fails while
/// the window is minimized.
fn capture_window(main: HWND) -> Option<Image> {
    // SAFETY: plain GDI calls on objects created and released here.
    unsafe {
        if IsIconic(main).as_bool() {
            return None;
        }
        let mut window = RECT::default();
        GetWindowRect(main, &mut window).ok()?;
        let visible = visible_bounds(main).unwrap_or(window);
        let (width, height) = (window.right - window.left, window.bottom - window.top);
        if width <= 0 || height <= 0 {
            return None;
        }
        let (bitmap, pixels) = new_bitmap(width, height)?;
        let dc = CreateCompatibleDC(None);
        let previous = SelectObject(dc, bitmap.into());
        // PW_RENDERFULLCONTENT includes content drawn by the GPU (the web view).
        let ok = PrintWindow(main, dc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool();
        SelectObject(dc, previous);
        let _ = DeleteDC(dc);
        let full = Image {
            width,
            height,
            bgra: pixels.to_vec(),
        };
        let _ = DeleteObject(bitmap.into());
        if !ok {
            return None;
        }
        // Cut away the invisible resize borders around the window.
        Some(crop(
            &full,
            visible.left - window.left,
            visible.top - window.top,
            visible.right - visible.left,
            visible.bottom - visible.top,
        ))
    }
}

fn crop(image: &Image, x: i32, y: i32, width: i32, height: i32) -> Image {
    let x = x.clamp(0, image.width);
    let y = y.clamp(0, image.height);
    let width = width.clamp(1, image.width - x);
    let height = height.clamp(1, image.height - y);
    let mut bgra = Vec::with_capacity((width * height * 4) as usize);
    for row in y..y + height {
        let start = ((row * image.width + x) * 4) as usize;
        bgra.extend_from_slice(&image.bgra[start..start + (width * 4) as usize]);
    }
    // GDI leaves the alpha channel empty; the taskbar needs opaque pixels.
    let (pixels, _) = bgra.as_chunks_mut::<4>();
    for pixel in pixels {
        pixel[3] = 255;
    }
    Image {
        width,
        height,
        bgra,
    }
}

/// Scales an image to fit inside `max_width` × `max_height`, keeping its shape.
fn fit(image: &Image, max_width: i32, max_height: i32, enlarge: bool) -> Image {
    let ratio =
        (max_width as f32 / image.width as f32).min(max_height as f32 / image.height as f32);
    let ratio = if enlarge { ratio } else { ratio.min(1.0) };
    let width = ((image.width as f32 * ratio).floor() as i32).clamp(1, max_width.max(1));
    let height = ((image.height as f32 * ratio).floor() as i32).clamp(1, max_height.max(1));
    if (width, height) == (image.width, image.height) {
        image.clone()
    } else {
        stretch(image, width, height)
    }
}

/// Resizes an image with GDI's smooth (halftone) scaling.
fn stretch(image: &Image, width: i32, height: i32) -> Image {
    let fallback = || image.clone();
    // SAFETY: plain GDI calls on objects created and released here.
    unsafe {
        let Some((source, source_pixels)) = new_bitmap(image.width, image.height) else {
            return fallback();
        };
        source_pixels.copy_from_slice(&image.bgra);
        let Some((target, target_pixels)) = new_bitmap(width, height) else {
            let _ = DeleteObject(source.into());
            return fallback();
        };
        let source_dc = CreateCompatibleDC(None);
        let target_dc = CreateCompatibleDC(None);
        let old_source = SelectObject(source_dc, source.into());
        let old_target = SelectObject(target_dc, target.into());
        SetStretchBltMode(target_dc, HALFTONE);
        let _ = SetBrushOrgEx(target_dc, 0, 0, None);
        let _ = StretchBlt(
            target_dc,
            0,
            0,
            width,
            height,
            Some(source_dc),
            0,
            0,
            image.width,
            image.height,
            SRCCOPY,
        );
        SelectObject(source_dc, old_source);
        SelectObject(target_dc, old_target);
        let _ = DeleteDC(source_dc);
        let _ = DeleteDC(target_dc);
        let mut bgra = target_pixels.to_vec();
        let (pixels, _) = bgra.as_chunks_mut::<4>();
        for pixel in pixels {
            pixel[3] = 255;
        }
        let _ = DeleteObject(source.into());
        let _ = DeleteObject(target.into());
        Image {
            width,
            height,
            bgra,
        }
    }
}

/// A 32-bit top-down bitmap and its pixel memory.
///
/// # Safety
/// The returned slice is valid until the bitmap is deleted.
unsafe fn new_bitmap(width: i32, height: i32) -> Option<(HBITMAP, &'static mut [u8])> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative height: rows go from the top down.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    // SAFETY: CreateDIBSection allocates width × height × 4 bytes at `bits`.
    unsafe {
        let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(bitmap.into());
            return None;
        }
        let pixels =
            std::slice::from_raw_parts_mut(bits.cast::<u8>(), (width * height * 4) as usize);
        Some((bitmap, pixels))
    }
}

/// Copies an image into a new bitmap for DWM (the caller deletes it).
fn to_bitmap(image: &Image) -> Option<HBITMAP> {
    // SAFETY: the bitmap's memory has exactly the image's size.
    unsafe {
        let (bitmap, pixels) = new_bitmap(image.width, image.height)?;
        pixels.copy_from_slice(&image.bgra);
        Some(bitmap)
    }
}

/// For a tab that was never on screen: its current page, as a fallback.
fn page_image(found: &Lookup, max_width: i32, max_height: i32) -> Option<Image> {
    let engine = found.app.state::<AppState>().engine.clone()?;
    render_fitting(&engine, &found.tab, max_width, max_height)
}

fn render_fitting(
    engine: &Engine,
    tab: &TaskbarTab,
    max_width: i32,
    max_height: i32,
) -> Option<Image> {
    if tab.width_pt <= 0.0 || tab.height_pt <= 0.0 || max_width <= 0 || max_height <= 0 {
        return None;
    }
    let scale = (max_width as f32 / tab.width_pt).min(max_height as f32 / tab.height_pt);
    let page = engine
        .render_thumbnail(
            tab.doc_id,
            tab.page,
            scale,
            Rotation::from_degrees(tab.rotation),
        )
        .ok()?;
    // Rounding can make the page a pixel too big; Windows rejects that.
    if page.width as i32 > max_width || page.height as i32 > max_height {
        return None;
    }
    let mut bgra = page.rgba.to_vec();
    let (pixels, _) = bgra.as_chunks_mut::<4>();
    for pixel in pixels {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
    Some(Image {
        width: page.width as i32,
        height: page.height as i32,
        bgra,
    })
}
