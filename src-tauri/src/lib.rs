mod config;
mod crm;
mod engine;
mod notify;
mod runtime;
mod schedule;
mod text;
mod tray;
mod updater;
mod window_pos;

use chrono::Local;
use runtime::{AppState, MAIN};
use tauri::{Manager, PhysicalPosition, WindowEvent};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};
use tauri_plugin_window_state::StateFlags;
use window_pos::{needs_reposition, top_right, Rect};

/// 位置を保存していないとき・保存した位置のモニターがないときは、メインモニターの右上に置く
fn place_main_window(window: &tauri::WebviewWindow, first_run: bool) {
    let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) else { return };
    let rect = |m: &tauri::Monitor| Rect { x: m.position().x, y: m.position().y, width: m.size().width, height: m.size().height };
    let monitors: Vec<Rect> = window.available_monitors().unwrap_or_default().iter().map(rect).collect();
    if !needs_reposition((pos.x, pos.y), &monitors, first_run) {
        return;
    }
    if let Ok(Some(m)) = window.primary_monitor() {
        let (x, y) = top_right(&rect(&m), size.width, m.scale_factor());
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| runtime::show_main(app)))
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Folder { path: config::log_dir(), file_name: Some("widget".into()) }),
                    Target::new(TargetKind::Stdout),
                ])
                .rotation_strategy(RotationStrategy::KeepSome(7))
                .max_file_size(1_000_000)
                .timezone_strategy(TimezoneStrategy::UseLocal)
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::POSITION | StateFlags::SIZE)
                .with_denylist(&[runtime::SETTINGS])
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            runtime::get_state,
            runtime::refresh_now,
            runtime::open_record,
            runtime::dismiss_badge,
            runtime::open_settings,
            runtime::get_settings,
            runtime::save_settings,
            runtime::test_connection,
            updater::install_update,
            updater::check_update,
        ])
        .setup(|app| {
            let dir = config::data_dir();
            let settings = config::load_settings(&dir);
            let today = Local::now().date_naive();
            let engine = engine::Engine::new(today, config::load_cache(&dir, today, &settings.cache_source()));
            let configured = settings.is_connection_configured();
            let (always_on_top, autostart) = (settings.always_on_top, settings.autostart);
            app.manage(AppState::new(dir, settings, engine));
            app.manage(updater::UpdateState::default());
            log::info!("起動しました（v{}）", env!("CARGO_PKG_VERSION"));

            tray::create(app, always_on_top)?;

            // window-state プラグインの保存ファイルがなければ初回の起動
            let first_run = app
                .path()
                .app_config_dir()
                .map(|d| !d.join(".window-state.json").exists())
                .unwrap_or(true);
            if let Some(w) = app.get_webview_window(MAIN) {
                let _ = w.set_always_on_top(always_on_top);
                place_main_window(&w, first_run);
                let _ = w.show();
            }
            let handle = app.handle().clone();
            runtime::apply_autostart(&handle, autostart);
            if !configured {
                runtime::open_settings_window(&handle);
            }
            tauri::async_runtime::spawn(runtime::poll_loop(handle.clone()));
            tauri::async_runtime::spawn(updater::update_loop(handle));
            Ok(())
        })
        .on_window_event(|window, event| {
            // ウィジェットは閉じずに隠す（終了はトレイから）
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == MAIN {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("アプリを起動できませんでした");
}
