//! タスクトレイのアイコンとメニュー

use crate::runtime;
use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, Wry};

const ICON: &[u8] = include_bytes!("../icons/tray.png");
const ICON_WARN: &[u8] = include_bytes!("../icons/tray-warn.png");

pub struct Tray {
    icon: TrayIcon,
    always_on_top: CheckMenuItem<Wry>,
    /// 同じ内容で何度も更新しないよう、最後に設定した値を覚えておく
    last: Mutex<(String, bool)>,
}

pub fn create(app: &App, always_on_top: bool) -> tauri::Result<()> {
    let refresh = MenuItem::with_id(app, "refresh", "今すぐ更新", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "ウィジェットを表示 / 隠す", true, None::<&str>)?;
    let on_top = CheckMenuItem::with_id(app, "on_top", "常に最前面に表示", true, always_on_top, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "設定...", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&refresh, &toggle, &on_top, &sep1, &settings, &sep2, &quit])?;

    let icon = TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(ICON)?)
        .tooltip("今日の予定")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "refresh" => runtime::request_refresh(app),
            "toggle" => runtime::toggle_main(app),
            "on_top" => runtime::toggle_always_on_top(app),
            "settings" => runtime::open_settings_window(app),
            "quit" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    runtime::logout(&app).await;
                    app.exit(0);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                runtime::toggle_main(tray.app_handle());
            }
        })
        .build(app)?;

    app.manage(Tray { icon, always_on_top: on_top, last: Mutex::new((String::new(), false)) });
    Ok(())
}

pub fn update(app: &AppHandle, tooltip: &str, warn: bool) {
    let Some(tray) = app.try_state::<Tray>() else { return };
    let mut last = tray.last.lock().unwrap_or_else(|e| e.into_inner());
    if last.0 != tooltip {
        let _ = tray.icon.set_tooltip(Some(tooltip));
        last.0 = tooltip.to_string();
    }
    if last.1 != warn {
        if let Ok(img) = Image::from_bytes(if warn { ICON_WARN } else { ICON }) {
            let _ = tray.icon.set_icon(Some(img));
        }
        last.1 = warn;
    }
}

pub fn set_always_on_top_checked(app: &AppHandle, on: bool) {
    if let Some(tray) = app.try_state::<Tray>() {
        let _ = tray.always_on_top.set_checked(on);
    }
}
