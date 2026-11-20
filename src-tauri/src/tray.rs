//! The menu-bar icon and the popover that drops from it.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hitch_core::hitch::{Link, Status};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalPosition, Rect, WebviewWindow, WindowEvent};

const TRAY_ID: &str = "hitch";
const WINDOW: &str = "main";
const ICON_ON: &[u8] = include_bytes!("../icons/tray.png");
const ICON_OFF: &[u8] = include_bytes!("../icons/tray-off.png");

/// Gap between the menu bar and the popover, and the minimum distance from a screen edge.
const GAP: f64 = 6.0;
const EDGE: f64 = 8.0;

/// When the popover last hid itself because it lost focus (unix millis). Clicking the
/// icon while the popover is open blurs it first; without this the same click would
/// immediately open it again.
static HIDDEN_AT: AtomicU64 = AtomicU64::new(0);
const REOPEN_GUARD_MS: u64 = 250;

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let quit = MenuItem::with_id(app, "quit", "Quit Hitch", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(ICON_ON)?)
        .icon_as_template(true)
        .tooltip("Hitch")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if event.id() == "quit" {
                app.exit(0);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle(tray.app_handle(), rect);
            }
        })
        .build(app)?;

    if let Some(window) = app.get_webview_window(WINDOW) {
        let popover = window.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::Focused(false) = event {
                // Stay open while developing so the inspector can be used.
                if !cfg!(debug_assertions) || std::env::var_os("HITCH_STAY_OPEN").is_none() {
                    HIDDEN_AT.store(now_ms(), Ordering::SeqCst);
                    let _ = popover.hide();
                }
            }
        });
    }
    Ok(())
}

/// The icon is always the full-strength Oxen mark. It gains a slash when Oxen is set up
/// but Claude is not using it.
pub fn reflect(app: &AppHandle, status: &Status) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let (icon, tooltip) = match status.link {
        Link::Connected if status.needs_restart => (ICON_ON, "Hitch — restart Claude to apply"),
        Link::Connected => (ICON_ON, "Hitch — Claude is on Oxen"),
        Link::Paused => (ICON_OFF, "Hitch — Claude is using Anthropic"),
        Link::Drifted => (ICON_OFF, "Hitch — Claude's setup changed"),
        Link::NotConfigured => (ICON_ON, "Hitch — not set up yet"),
    };
    if let Ok(image) = Image::from_bytes(icon) {
        let _ = tray.set_icon(Some(image));
        let _ = tray.set_icon_as_template(true);
    }
    let _ = tray.set_tooltip(Some(tooltip));
}

/// Opens the popover once the menu-bar icon has a position. Used on first launch.
///
/// Right after launch the icon is not laid out yet and reports a placeholder position,
/// so wait for one that is actually in a menu bar. If that never comes, open in the
/// top-right corner of the main screen, which is where menu-bar icons live.
pub fn show_when_ready(app: AppHandle) {
    std::thread::spawn(move || {
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(150));
            let rect = app
                .tray_by_id(TRAY_ID)
                .and_then(|tray| tray.rect().ok().flatten());
            let window = app.get_webview_window(WINDOW);
            if let (Some(rect), Some(window)) = (rect, window) {
                if in_menu_bar(&window, &rect) {
                    let handle = app.clone();
                    let _ = app.run_on_main_thread(move || show(&handle, rect));
                    return;
                }
            }
        }
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || show_in_corner(&handle));
    });
}

/// How far down from the top of a screen a menu-bar icon can sit, in points.
const MENU_BAR_DEPTH: f64 = 60.0;

fn in_menu_bar(window: &WebviewWindow, rect: &Rect) -> bool {
    let scale = window.scale_factor().unwrap_or(1.0);
    let position = rect.position.to_physical::<f64>(scale);
    let size = rect.size.to_physical::<f64>(scale);
    if size.width <= 0.0 || size.height <= 0.0 {
        return false;
    }
    let Ok(monitors) = window.available_monitors() else {
        return false;
    };
    monitors.iter().any(|monitor| {
        let left = f64::from(monitor.position().x);
        let top = f64::from(monitor.position().y);
        let right = left + f64::from(monitor.size().width);
        (left..right).contains(&position.x)
            && (top..top + MENU_BAR_DEPTH * monitor.scale_factor()).contains(&position.y)
    })
}

fn show_in_corner(app: &AppHandle) {
    let Some(window) = app.get_webview_window(WINDOW) else {
        return;
    };
    if let (Ok(Some(monitor)), Ok(size)) = (window.primary_monitor(), window.outer_size()) {
        let scale = monitor.scale_factor();
        let x = f64::from(monitor.position().x) + f64::from(monitor.size().width)
            - f64::from(size.width)
            - EDGE * scale;
        let y = f64::from(monitor.position().y) + (MENU_BAR_DEPTH / 2.0 + GAP) * scale;
        let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    }
    let _ = window.show();
    let _ = window.set_focus();
}

fn toggle(app: &AppHandle, rect: Rect) {
    let Some(window) = app.get_webview_window(WINDOW) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else if now_ms().saturating_sub(HIDDEN_AT.load(Ordering::SeqCst)) > REOPEN_GUARD_MS {
        show(app, rect);
    }
}

fn show(app: &AppHandle, rect: Rect) {
    let Some(window) = app.get_webview_window(WINDOW) else {
        return;
    };
    place_under(&window, rect);
    let _ = window.show();
    let _ = window.set_focus();
}

/// Centers the popover under the icon, kept inside the screen the icon is on.
fn place_under(window: &WebviewWindow, rect: Rect) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let icon_position = rect.position.to_physical::<f64>(scale);
    let icon_size = rect.size.to_physical::<f64>(scale);
    let Ok(size) = window.outer_size() else {
        return;
    };
    let width = f64::from(size.width);

    let mut x = icon_position.x + icon_size.width / 2.0 - width / 2.0;
    let y = icon_position.y + icon_size.height + GAP * scale;

    if let Ok(Some(monitor)) = window.monitor_from_point(icon_position.x, icon_position.y) {
        let left = f64::from(monitor.position().x) + EDGE * scale;
        let right = f64::from(monitor.position().x) + f64::from(monitor.size().width)
            - width
            - EDGE * scale;
        x = x.clamp(left, right.max(left));
    }
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
