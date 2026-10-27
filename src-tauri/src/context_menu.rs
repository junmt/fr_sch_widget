//! ウィジェットの右クリックメニュー（WebView の標準のメニューの代わり）

use crate::runtime::{self, AppState};
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, Window};

/// メニューの項目の ID（トレイのメニューと区別するため ctx_ を付ける）
const OPEN_PREFIX: &str = "ctx_open:";

/// 右クリックしたときのメニューを出す。`item_id` は右クリックした予定（なければ None）
#[tauri::command]
pub async fn show_context_menu(window: WebviewWindow, item_id: Option<String>, compact: bool) -> Result<(), String> {
    let app = window.app_handle();
    let on_top = app.state::<AppState>().settings.lock().map(|s| s.always_on_top).unwrap_or(true);
    let menu = build(app, item_id.as_deref(), compact, on_top).map_err(|e| e.to_string())?;
    window.popup_menu(&menu).map_err(|e| e.to_string())
}

fn build(app: &AppHandle, item_id: Option<&str>, compact: bool, on_top: bool) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    if let Some(id) = item_id {
        menu.append(&MenuItem::with_id(app, format!("{OPEN_PREFIX}{id}"), "CRM で開く", true, None::<&str>)?)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    menu.append(&MenuItem::with_id(app, "ctx_refresh", "今すぐ更新", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "ctx_compact", if compact { "広げる" } else { "折りたたむ" }, true, None::<&str>)?)?;
    menu.append(&CheckMenuItem::with_id(app, "ctx_on_top", "常に最前面に表示", true, on_top, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "ctx_settings", "設定...", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "ctx_check_update", "アップデートを確認", true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "ctx_hide", "ウィジェットを隠す", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "ctx_quit", "終了", true, None::<&str>)?)?;
    Ok(menu)
}

/// メニューで選ばれた項目を処理する（メインウィンドウの on_menu_event から呼ぶ）
pub fn handle(window: &Window, event: MenuEvent) {
    let app = window.app_handle();
    let id = event.id.as_ref();
    if let Some(record) = id.strip_prefix(OPEN_PREFIX) {
        if let Err(e) = runtime::open_record(app.clone(), record.to_string()) {
            log::warn!("CRM を開けませんでした: {e}");
        }
        return;
    }
    match id {
        "ctx_refresh" => runtime::request_refresh(app),
        "ctx_compact" => {
            let _ = window.emit("toggle-compact", ());
        }
        "ctx_on_top" => runtime::toggle_always_on_top(app),
        "ctx_settings" => runtime::open_settings_window(app),
        "ctx_check_update" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { crate::updater::check(&app, true).await });
        }
        "ctx_hide" => {
            let _ = window.hide();
        }
        "ctx_quit" => runtime::quit(app),
        _ => {}
    }
}
