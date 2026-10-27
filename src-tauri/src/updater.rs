//! 自動アップデート。GitHub のリリースの latest.json を見て、新しい版があれば知らせる。
//! 更新するかどうかはユーザーが決める（ウィジェットのフッターかトレイのメニューから）。

use crate::runtime::{self, AppState};
use crate::{notify, tray};
use chrono::{DateTime, Duration, Local};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// 自動で確認する間隔（時間）
pub const CHECK_INTERVAL_HOURS: i64 = 6;
/// 起動してから最初に確認するまでの時間（起動直後の取得と重ならないようにする）
const FIRST_CHECK_DELAY: std::time::Duration = std::time::Duration::from_secs(30);
/// 確認の時刻になったかを見る間隔
const LOOP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(600);

/// 自動の確認をする時刻か（一度も確認していない、前回から間隔以上たった、時計が戻った）
pub fn should_check(last: Option<DateTime<Local>>, now: DateTime<Local>) -> bool {
    match last {
        None => true,
        Some(t) => now < t || now - t >= Duration::hours(CHECK_INTERVAL_HOURS),
    }
}

#[derive(Default)]
pub struct UpdateState {
    pending: Mutex<Option<Update>>,
    last_check: Mutex<Option<DateTime<Local>>>,
    /// 同じ版の通知を何度も出さない
    notified_version: Mutex<Option<String>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn set_available(app: &AppHandle, version: Option<String>) {
    tray::set_update(app, version.as_deref());
    *lock(&app.state::<AppState>().update_version) = version;
    runtime::publish(app);
}

/// 新しい版があるか確認する。`manual` のときは「最新です」「確認できませんでした」も通知する
pub async fn check(app: &AppHandle, manual: bool) {
    let st = app.state::<UpdateState>();
    *lock(&st.last_check) = Some(Local::now());
    let result = match app.updater() {
        Ok(u) => u.check().await,
        Err(e) => Err(e),
    };
    match result {
        Ok(Some(update)) => {
            let version = update.version.clone();
            log::info!("新しいバージョンがあります: {} → {version}", update.current_version);
            *lock(&st.pending) = Some(update);
            let first = lock(&st.notified_version).as_deref() != Some(version.as_str());
            if first || manual {
                notify::show(
                    app,
                    &format!("新しいバージョン v{version} があります"),
                    "ウィジェットの下の「更新」か、トレイのメニューから更新できます。",
                    None,
                );
                *lock(&st.notified_version) = Some(version.clone());
            }
            set_available(app, Some(version));
        }
        Ok(None) => {
            log::info!("最新のバージョンです");
            *lock(&st.pending) = None;
            set_available(app, None);
            if manual {
                let current = app.package_info().version.to_string();
                notify::show(app, "最新のバージョンです", &format!("v{current} を使っています。"), None);
            }
        }
        Err(e) => {
            log::warn!("アップデートを確認できませんでした: {e}");
            if manual {
                notify::show(app, "アップデートを確認できませんでした", &e.to_string(), None);
            }
        }
    }
}

/// 確認しておいた新しい版をダウンロードしてインストールする。
/// Windows ではインストーラーが起動してアプリは終了し、インストールが終わると起動し直す。
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let update = lock(&app.state::<UpdateState>().pending).clone().ok_or("新しいバージョンはありません")?;
    log::info!("v{} に更新します", update.version);
    runtime::logout(app).await;
    update
        .download_and_install(|_, _| {}, || log::info!("ダウンロードが終わりました"))
        .await
        .map_err(|e| {
            log::error!("更新できませんでした: {e}");
            format!("更新できませんでした（{e}）")
        })?;
    app.restart();
}

pub async fn update_loop(app: AppHandle) {
    tokio::time::sleep(FIRST_CHECK_DELAY).await;
    loop {
        let enabled = lock(&app.state::<AppState>().settings).auto_update_check;
        let last = *lock(&app.state::<UpdateState>().last_check);
        if enabled && should_check(last, Local::now()) {
            check(&app, false).await;
        }
        tokio::time::sleep(LOOP_INTERVAL).await;
    }
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    install(&app).await
}

#[tauri::command]
pub async fn check_update(app: AppHandle) {
    check(&app, true).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 27, h, m, 0).single().unwrap()
    }

    // UP-01〜UP-05: 自動で確認する時刻か
    #[test]
    fn up01_never_checked() {
        assert!(should_check(None, at(9, 0)));
    }
    #[test]
    fn up02_before_interval() {
        assert!(!should_check(Some(at(9, 0)), at(14, 59)));
    }
    #[test]
    fn up03_exactly_interval() {
        assert!(should_check(Some(at(9, 0)), at(15, 0)));
    }
    #[test]
    fn up04_after_interval() {
        assert!(should_check(Some(at(9, 0)), at(20, 0)));
    }
    #[test]
    fn up05_clock_moved_back() {
        assert!(should_check(Some(at(9, 0)), at(8, 0)));
    }
    #[test]
    fn up06_same_time() {
        assert!(!should_check(Some(at(9, 0)), at(9, 0)));
    }
}
