//! アプリの状態・ポーリングのループ・画面から呼ぶコマンド

use crate::config::{self, credentials, KeyAction, Settings};
use crate::crm::{format_user_date, new_event_url, record_url, AuthState, CrmClient, CrmError, Session};
use crate::engine::{is_resumed, should_fetch, Engine, Notice, SyncState, SyncStatus};
use crate::schedule::{build_today, query_range, CrmTimeZone, ScheduleItem};
use crate::{notify, text, tray};
use chrono::{DateTime, Local, NaiveDate};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::Notify;

/// タイマーの間隔（リマインドとバッジの期限を見る）
const TICK: Duration = Duration::from_secs(10);

pub const MAIN: &str = "main";
pub const SETTINGS: &str = "settings";

/// CRM への接続（URL とユーザーが変わったら作り直す）
struct Connection {
    key: String,
    client: Arc<CrmClient>,
    session: Option<Session>,
    /// CRM のユーザー設定の日付形式（登録画面に日付を渡すときに使う。読めなかったら空）
    date_format: Option<String>,
}

pub struct AppState {
    pub engine: Mutex<Engine>,
    pub settings: Mutex<Settings>,
    /// 新しいバージョンがあれば、その版の番号
    pub update_version: Mutex<Option<String>>,
    data_dir: PathBuf,
    /// 手動更新・設定の保存で、すぐに取得させる
    wake: Notify,
    /// 同時に複数の取得をしないよう、取得中はこのロックを持つ
    conn: tokio::sync::Mutex<Option<Connection>>,
    /// ログインの状態（トークンの使い回し・認証の制限）。接続テストとも共有する
    auth: tokio::sync::Mutex<AuthState>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DisplayView {
    soon_minutes: u32,
    opacity: u32,
    theme: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StateView {
    items: Vec<ScheduleItem>,
    status: SyncStatus,
    today: NaiveDate,
    display: DisplayView,
    update_version: Option<String>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn new(data_dir: PathBuf, settings: Settings, engine: Engine) -> Self {
        let data_dir_for_auth = data_dir.clone();
        Self {
            engine: Mutex::new(engine),
            settings: Mutex::new(settings),
            update_version: Mutex::new(None),
            data_dir,
            wake: Notify::new(),
            conn: tokio::sync::Mutex::new(None),
            auth: tokio::sync::Mutex::new(config::load_auth(&data_dir_for_auth)),
        }
    }

    fn view(&self) -> StateView {
        let s = lock(&self.settings);
        let e = lock(&self.engine);
        StateView {
            items: e.items().to_vec(),
            status: e.status.clone(),
            today: e.day,
            display: DisplayView { soon_minutes: s.soon_minutes, opacity: s.opacity, theme: s.theme.clone() },
            update_version: lock(&self.update_version).clone(),
        }
    }
}

fn update_tray(app: &AppHandle, view: &StateView) {
    let tooltip = text::tray_tooltip(&view.items, Local::now().naive_local(), &view.status);
    let warn = matches!(view.status.state, SyncState::Error | SyncState::AuthError);
    tray::update(app, &tooltip, warn);
}

/// 画面とトレイに今の状態を反映する
pub fn publish(app: &AppHandle) {
    let view = app.state::<AppState>().view();
    update_tray(app, &view);
    if let Err(e) = app.emit("state-changed", view) {
        log::warn!("画面に状態を送れませんでした: {e}");
    }
}

fn dispatch(app: &AppHandle, notices: Vec<Notice>, today: NaiveDate) {
    for n in notices {
        let (title, body) = text::notice_text(&n, today);
        log::info!("通知: {title} / {body}");
        notify::show(app, &title, &body, text::notice_item_id(&n).map(str::to_string));
    }
}

// ---------- ポーリング ----------

pub async fn poll_loop(app: AppHandle) {
    let st = app.state::<AppState>();
    let mut next_fetch: Option<DateTime<Local>> = None;
    let mut last_tick = Local::now();
    let mut force = false;
    loop {
        let now = Local::now();
        let resumed = is_resumed(last_tick, now);
        last_tick = now;
        let rolled = lock(&st.engine).roll_day(now.date_naive());
        if resumed || rolled {
            log::info!("{}ため、すぐに取得します", if rolled { "日付が変わった" } else { "スリープから復帰した" });
            next_fetch = None;
            if rolled {
                publish(&app);
            }
        }

        let paused = lock(&st.engine).is_paused();
        if should_fetch(force, paused, next_fetch, now) {
            fetch_once(&app).await;
            let interval = lock(&st.settings).interval_minutes;
            next_fetch = Some(lock(&st.engine).next_fetch_at(Local::now(), interval));
            force = false;
        }

        let settings = lock(&st.settings).clone();
        let (notices, changed, today) = {
            let mut e = lock(&st.engine);
            let (n, c) = e.tick(Local::now().naive_local(), &settings);
            (n, c, e.day)
        };
        dispatch(&app, notices, today);
        if changed {
            publish(&app);
        } else {
            // ツールチップの残り時間だけ更新する
            update_tray(&app, &st.view());
        }

        tokio::select! {
            _ = tokio::time::sleep(TICK) => {}
            _ = st.wake.notified() => { force = true; }
        }
    }
}

async fn fetch_once(app: &AppHandle) {
    let st = app.state::<AppState>();
    let settings = lock(&st.settings).clone();
    let key = credentials::load(&settings.username);
    let Some(key) = key.filter(|_| settings.is_connection_configured()) else {
        lock(&st.engine).set_unconfigured();
        publish(app);
        return;
    };

    lock(&st.engine).status.fetching = true;
    publish(app);

    let result = fetch_records(&st, &settings, &key, Local::now().date_naive()).await;
    let now = Local::now();
    let mut save = None;
    let mut notices = Vec::new();
    {
        let mut e = lock(&st.engine);
        match result {
            Ok(records) => {
                let today = now.date_naive();
                e.roll_day(today);
                let items = build_today(&records, settings.crm_time_zone, &Local, today, &settings.filter());
                log::info!("取得しました: {}件（今日の表示 {}件）", records.len(), items.len());
                notices = e.on_success(items, now, &settings);
                save = e.cache(&settings.cache_source());
            }
            Err(err) => {
                log::warn!("取得に失敗しました: {err:?}");
                e.on_error(&err);
            }
        }
    }
    if let Some(cache) = save {
        if let Err(e) = config::save_cache(&st.data_dir, &cache) {
            log::warn!("キャッシュを保存できませんでした: {e}");
        }
    }
    let today = lock(&st.engine).day;
    dispatch(app, notices, today);
    publish(app);
}

/// ログイン（必要なら）して、今日の前後の予定を取得する。セッション切れなら1回だけログインし直す
/// `day` の前後の予定を取得する（ポーリングは今日、日めくりはその日）
async fn fetch_records(st: &AppState, settings: &Settings, access_key: &str, day: NaiveDate) -> Result<Vec<Value>, CrmError> {
    let mut guard = st.conn.lock().await;
    let conn_key = format!("{}|{}", settings.crm_url, settings.username);
    if guard.as_ref().map_or(true, |c| c.key != conn_key) {
        *guard = Some(Connection {
            key: conn_key,
            client: Arc::new(CrmClient::new(&settings.crm_url)?),
            session: None,
            date_format: None,
        });
    }
    let conn = guard.as_mut().expect("接続は直前に作った");
    let (from, to) = query_range(day);
    let mut auth = st.auth.lock().await;
    let before = auth.clone();
    let result = conn
        .client
        .fetch_with_relogin(&mut conn.session, &mut auth, &settings.username, access_key, from, to)
        .await;
    save_auth_if_changed(&st.data_dir, &before, &auth);
    drop(auth);

    if result.is_ok() && conn.date_format.is_none() {
        if let Some(session) = conn.session.clone() {
            let format = match conn.client.retrieve(&session, &session.user_id).await {
                Ok(user) => user.get("date_format").and_then(Value::as_str).unwrap_or("").to_string(),
                Err(e) => {
                    log::warn!("ユーザーの日付形式を読めませんでした: {e}");
                    String::new()
                }
            };
            log::info!("ユーザーの日付形式: {format:?}");
            conn.date_format = Some(format);
        }
    }
    result
}

fn save_auth_if_changed(dir: &std::path::Path, before: &AuthState, after: &AuthState) {
    if before != after {
        if let Err(e) = config::save_auth(dir, after) {
            log::warn!("ログインの状態を保存できませんでした: {e}");
        }
    }
}

/// 終了する前にログアウトする（最大2秒待つ）
pub async fn logout(app: &AppHandle) {
    let st = app.state::<AppState>();
    let conn = st.conn.lock().await.take();
    if let Some(Connection { client, session: Some(s), .. }) = conn {
        let _ = tokio::time::timeout(Duration::from_secs(2), client.logout(&s)).await;
    }
}

/// CRM からログアウトしてから終了する
pub fn quit(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        logout(&app).await;
        app.exit(0);
    });
}

// ---------- ウィンドウ ----------

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn toggle_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            show_main(app);
        }
    }
}

pub fn open_settings_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(SETTINGS) {
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("settings.html".into()))
        .title("設定 - F-RevoCRM Schedule Widget")
        .inner_size(480.0, 700.0)
        .min_inner_size(400.0, 400.0)
        .always_on_top(true)
        .center()
        .build();
    if let Err(e) = built {
        log::error!("設定画面を開けませんでした: {e}");
    }
}

pub fn apply_always_on_top(app: &AppHandle, on: bool) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.set_always_on_top(on);
    }
    tray::set_always_on_top_checked(app, on);
}

pub fn apply_autostart(app: &AppHandle, on: bool) {
    // 開発中の実行ファイルを自動起動に登録しない
    if cfg!(debug_assertions) {
        return;
    }
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    let result = match (on, launcher.is_enabled().unwrap_or(false)) {
        (true, false) => launcher.enable(),
        (false, true) => launcher.disable(),
        _ => Ok(()),
    };
    if let Err(e) = result {
        log::warn!("自動起動の設定を変えられませんでした: {e}");
    }
}

/// トレイから「常に最前面に表示」を切り替えたとき
pub fn toggle_always_on_top(app: &AppHandle) {
    let st = app.state::<AppState>();
    let settings = {
        let mut s = lock(&st.settings);
        s.always_on_top = !s.always_on_top;
        s.clone()
    };
    if let Err(e) = config::save_settings(&st.data_dir, &settings) {
        log::warn!("設定を保存できませんでした: {e}");
    }
    apply_always_on_top(app, settings.always_on_top);
}

pub fn request_refresh(app: &AppHandle) {
    app.state::<AppState>().wake.notify_one();
}

// ---------- コマンド ----------

#[tauri::command]
pub fn get_state(state: tauri::State<'_, AppState>) -> StateView {
    state.view()
}

#[tauri::command]
pub fn refresh_now(app: AppHandle) {
    request_refresh(&app);
}

#[tauri::command]
pub fn open_record(app: AppHandle, id: String) -> Result<(), String> {
    let base = lock(&app.state::<AppState>().settings).crm_url.clone();
    let url = record_url(&base, &id)?;
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayView {
    date: NaiveDate,
    items: Vec<ScheduleItem>,
    fetched_at: DateTime<Local>,
}

/// 日めくり：指定した日の予定を取得する（通知・バッジ・キャッシュの対象にはしない）
#[tauri::command]
pub async fn get_day(state: tauri::State<'_, AppState>, date: NaiveDate) -> Result<DayView, String> {
    let settings = lock(&state.settings).clone();
    let key = credentials::load(&settings.username)
        .filter(|_| settings.is_connection_configured())
        .ok_or("接続設定をしてください")?;
    let records = fetch_records(&state, &settings, &key, date).await.map_err(|e| e.to_string())?;
    let items = build_today(&records, settings.crm_time_zone, &Local, date, &settings.filter());
    Ok(DayView { date, items, fetched_at: Local::now() })
}

/// 活動の登録画面をブラウザで開く。今日以外の日を渡すと、その日の日付を入れて開く
#[tauri::command]
pub async fn open_new_event(app: AppHandle, date: Option<NaiveDate>) -> Result<(), String> {
    let st = app.state::<AppState>();
    let base = lock(&st.settings).crm_url.clone();
    let today = Local::now().date_naive();
    let date_text = match date.filter(|d| *d != today) {
        Some(d) => {
            let format = st.conn.lock().await.as_ref().and_then(|c| c.date_format.clone());
            format.and_then(|f| format_user_date(d, &f))
        }
        None => None,
    };
    let url = new_event_url(&base, date_text.as_deref())?;
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn dismiss_badge(app: AppHandle, id: String) {
    if lock(&app.state::<AppState>().engine).dismiss_badge(&id) {
        publish(&app);
    }
}

/// Windows ではウィンドウを作る処理を同期のコマンドで呼ぶと WebView2 が固まるため、async にする
#[tauri::command]
pub async fn open_settings(app: AppHandle) {
    open_settings_window(&app);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    settings: Settings,
    has_access_key: bool,
}

#[tauri::command]
pub fn get_settings(state: tauri::State<'_, AppState>) -> SettingsView {
    let settings = lock(&state.settings).clone();
    let has_access_key = credentials::load(&settings.username).is_some();
    SettingsView { settings, has_access_key }
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings, access_key: Option<String>) -> Result<SettingsView, String> {
    let st = app.state::<AppState>();
    let new = settings.normalized();
    config::validate_url(&new.crm_url)?;
    let old = lock(&st.settings).clone();

    match config::key_action(access_key.as_deref(), &old.username, &new.username) {
        KeyAction::Save(k) => credentials::save(&new.username, &k)?,
        KeyAction::Move => {
            // ユーザー名だけ変えた場合は、保存済みのアクセスキーを引き継ぐ
            if let Some(k) = credentials::load(&old.username) {
                credentials::save(&new.username, &k)?;
                credentials::delete(&old.username);
            }
        }
        KeyAction::Keep => {}
    }
    config::save_settings(&st.data_dir, &new).map_err(|e| format!("設定を保存できませんでした: {e}"))?;

    *lock(&st.settings) = new.clone();
    *st.conn.lock().await = None;
    lock(&st.engine).reset_baseline();
    apply_always_on_top(&app, new.always_on_top);
    apply_autostart(&app, new.autostart);
    publish(&app);
    request_refresh(&app);
    log::info!("設定を保存しました");

    let has_access_key = credentials::load(&new.username).is_some();
    Ok(SettingsView { settings: new, has_access_key })
}

#[tauri::command]
pub async fn test_connection(
    state: tauri::State<'_, AppState>,
    crm_url: String,
    username: String,
    access_key: Option<String>,
    crm_time_zone: Option<CrmTimeZone>,
) -> Result<String, String> {
    let url = crate::crm::normalize_url(&crm_url);
    let username = username.trim().to_string();
    config::validate_url(&url)?;
    if url.is_empty() || username.is_empty() {
        return Err("CRM の URL とユーザー名を入力してください".into());
    }
    let key = access_key
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .or_else(|| credentials::load(&username))
        .ok_or("アクセスキーを入力してください")?;

    let client = CrmClient::new(&url).map_err(|e| e.to_string())?;
    let session = {
        let mut auth = state.auth.lock().await;
        let before = auth.clone();
        let r = client.login(&username, &key, &mut auth, chrono::Utc::now()).await;
        save_auth_if_changed(&state.data_dir, &before, &auth);
        r.map_err(|e| e.to_string())?
    };
    let today = Local::now().date_naive();
    let (from, to) = query_range(today);
    let result = client.fetch_events(&session, from, to).await;
    client.logout(&session).await;
    let records = result.map_err(|e| e.to_string())?;
    let source = crm_time_zone.unwrap_or_default();
    let count = build_today(&records, source, &Local, today, &Settings::default().filter()).len();
    Ok(format!("接続できました（今日の予定 {count}件）"))
}

