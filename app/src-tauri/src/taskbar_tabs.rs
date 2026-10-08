//! Windows only: each open tab gets its own preview in the taskbar, like
//! separate windows, while PDFRivet keeps one window with tabs.
//!
//! Windows does this with "proxy" windows: one invisible window per tab,
//! registered with the taskbar as a tab of the main window
//! (`ITaskbarList3::RegisterTab`). When the taskbar wants a preview, it asks
//! the proxy (`WM_DWMSENDICONICTHUMBNAIL`) and we answer with the tab's current
//! page. Clicking a preview activates its proxy, and closing it sends
//! `WM_CLOSE`; both are passed on to the UI as a `taskbar-tab` event.
//!
//! Everything here runs on the main (UI) thread, which owns these windows.
#![allow(unsafe_code)]

use std::cell::RefCell;

use rivet_core::{Engine, Rotation};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        Graphics::{
            Dwm::{
                DWMWA_DISALLOW_PEEK, DWMWA_FORCE_ICONIC_REPRESENTATION, DWMWA_HAS_ICONIC_BITMAP,
                DwmInvalidateIconicBitmaps, DwmSetIconicThumbnail, DwmSetWindowAttribute,
            },
            Gdi::{
                BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS,
                DeleteObject,
            },
        },
        System::{
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
            LibraryLoader::GetModuleHandleW,
        },
        UI::{
            Shell::{ITaskbarList3, TaskbarList},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, ICON_BIG, ICON_SMALL, IsIconic,
                RegisterClassW, SW_RESTORE, SendMessageW, SetForegroundWindow, SetWindowTextW,
                ShowWindow, WA_INACTIVE, WM_ACTIVATE, WM_CLOSE, WM_DWMSENDICONICTHUMBNAIL,
                WM_GETICON, WM_SETICON, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_EX_NOACTIVATE,
                WS_EX_TOOLWINDOW, WS_POPUP, WS_SYSMENU,
            },
        },
    },
    core::{BOOL, HSTRING, w},
};

use crate::AppState;

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

struct Proxy {
    hwnd: HWND,
    tab: TaskbarTab,
}

#[derive(Default)]
struct Taskbar {
    app: Option<AppHandle>,
    main: HWND,
    list: Option<ITaskbarList3>,
    proxies: Vec<Proxy>,
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
                    if proxy.tab != tab {
                        // SAFETY: valid window handle owned by this thread.
                        unsafe {
                            let _ = SetWindowTextW(proxy.hwnd, &HSTRING::from(tab.title.as_str()));
                            let _ = DwmInvalidateIconicBitmaps(proxy.hwnd);
                        }
                        proxy.tab = tab;
                    }
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
        // Previews come from us (the window itself is never shown), and hovering
        // a preview doesn't "peek" at an empty window.
        let on = BOOL::from(true);
        let size = size_of::<BOOL>() as u32;
        let pointer = (&on as *const BOOL).cast();
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_FORCE_ICONIC_REPRESENTATION, pointer, size);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_HAS_ICONIC_BITMAP, pointer, size);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_DISALLOW_PEEK, pointer, size);
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
            let max_width = ((lparam.0 >> 16) & 0xffff) as u32;
            let max_height = (lparam.0 & 0xffff) as u32;
            send_thumbnail(hwnd, max_width, max_height);
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

/// Tells the UI about a click on a preview, and brings the main window forward.
fn notify(hwnd: HWND, action: &'static str) {
    let found = TASKBAR.with(|cell| {
        let taskbar = cell.try_borrow().ok()?;
        let tab = taskbar.proxies.iter().find(|p| p.hwnd == hwnd)?.tab.id;
        Some((taskbar.app.clone()?, taskbar.main, tab))
    });
    let Some((app, main, id)) = found else { return };
    let _ = app.emit_to("main", "taskbar-tab", TaskbarTabEvent { action, id });
    if action == "activate" {
        // SAFETY: valid main window handle.
        unsafe {
            if IsIconic(main).as_bool() {
                let _ = ShowWindow(main, SW_RESTORE);
            }
            let _ = SetForegroundWindow(main);
        }
    }
}

/// Renders the tab's current page to fit the preview, and gives it to Windows.
fn send_thumbnail(hwnd: HWND, max_width: u32, max_height: u32) {
    let found = TASKBAR.with(|cell| {
        let taskbar = cell.try_borrow().ok()?;
        let tab = taskbar.proxies.iter().find(|p| p.hwnd == hwnd)?.tab.clone();
        Some((taskbar.app.clone()?, tab))
    });
    let Some((app, tab)) = found else { return };
    let Some(engine) = app.state::<AppState>().engine.clone() else {
        return;
    };
    if let Some(page) = render_fitting(&engine, &tab, max_width, max_height) {
        set_thumbnail(hwnd, &page);
    }
}

fn render_fitting(
    engine: &Engine,
    tab: &TaskbarTab,
    max_width: u32,
    max_height: u32,
) -> Option<rivet_core::RenderedPage> {
    if tab.width_pt <= 0.0 || tab.height_pt <= 0.0 || max_width == 0 || max_height == 0 {
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
    (page.width <= max_width && page.height <= max_height).then_some(page)
}

/// Copies RGBA pixels into a 32-bit bitmap (BGRA, opaque) and hands it to DWM.
fn set_thumbnail(hwnd: HWND, page: &rivet_core::RenderedPage) {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: page.width as i32,
            // Negative height: rows go from the top down, like ours.
            biHeight: -(page.height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    // SAFETY: the bitmap is created with exactly width × height × 4 bytes, which
    // are filled below and released after DWM has made its copy.
    unsafe {
        let Ok(bitmap) = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
            return;
        };
        let out = std::slice::from_raw_parts_mut(bits.cast::<u8>(), page.rgba.len());
        let (dst, _) = out.as_chunks_mut::<4>();
        let (src, _) = page.rgba.as_chunks::<4>();
        for (d, s) in dst.iter_mut().zip(src) {
            *d = [s[2], s[1], s[0], 255];
        }
        let _ = DwmSetIconicThumbnail(hwnd, bitmap, 0);
        let _ = DeleteObject(bitmap.into());
    }
}
