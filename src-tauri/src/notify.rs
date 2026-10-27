//! Windows のトースト通知

use tauri::AppHandle;

/// 通知を出す。`focus_id` を渡すと、クリックしたときにウィジェットを前に出してその予定を強調する
#[cfg(windows)]
pub fn show(app: &AppHandle, title: &str, body: &str, focus_id: Option<String>) {
    use tauri::Emitter;
    use tauri_winrt_notification::{Duration, Sound, Toast};

    let handle = app.clone();
    let result = Toast::new(&app_user_model_id(app))
        .title(title)
        .text1(body)
        .sound(Some(Sound::Default))
        .duration(Duration::Short)
        .on_activated(move |_| {
            crate::runtime::show_main(&handle);
            if let Some(id) = &focus_id {
                let _ = handle.emit("focus-item", id);
            }
            Ok(())
        })
        .show();
    if let Err(e) = result {
        log::warn!("通知を出せませんでした: {e:?}");
    }
}

#[cfg(not(windows))]
pub fn show(_app: &AppHandle, title: &str, body: &str, _focus_id: Option<String>) {
    log::info!("（通知は Windows のみ）{title}: {body}");
}

/// インストールしたアプリは identifier を、開発中（target\debug など）は PowerShell の ID を使う。
/// 登録されていない ID では Windows が通知を表示しないため。
#[cfg(windows)]
fn app_user_model_id(app: &AppHandle) -> String {
    let exe = std::env::current_exe().ok();
    if is_dev_build(exe.as_deref()) {
        tauri_winrt_notification::Toast::POWERSHELL_APP_ID.to_string()
    } else {
        app.config().identifier.clone()
    }
}

/// cargo の出力先（target\debug / target\release）から実行されているか
pub fn is_dev_build(exe: Option<&std::path::Path>) -> bool {
    exe.and_then(|p| p.parent())
        .map(|d| d.ends_with("target/debug") || d.ends_with("target/release"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::is_dev_build;
    use std::path::Path;

    // N-01〜N-05: 開発中のビルドかどうか（通知の AppUserModelID の選び方）
    #[test]
    fn n01_debug_dir_is_dev() {
        assert!(is_dev_build(Some(Path::new("C:/dev/app/src-tauri/target/debug/app.exe"))));
    }
    #[test]
    fn n02_release_dir_is_dev() {
        assert!(is_dev_build(Some(Path::new("C:/dev/app/src-tauri/target/release/app.exe"))));
    }
    #[test]
    fn n03_installed_dir_is_not_dev() {
        assert!(!is_dev_build(Some(Path::new("C:/Users/u/AppData/Local/FR予定ウィジェット/app.exe"))));
    }
    #[test]
    fn n04_similar_name_is_not_dev() {
        // "mytarget\\debug" のような部分一致では開発中とみなさない
        assert!(!is_dev_build(Some(Path::new("C:/x/mytarget/debug/app.exe"))));
        assert!(!is_dev_build(Some(Path::new("C:/x/target/debugger/app.exe"))));
    }
    #[test]
    fn n05_unknown_exe_is_not_dev() {
        assert!(!is_dev_build(None));
        assert!(!is_dev_build(Some(Path::new("app.exe"))));
    }
}
