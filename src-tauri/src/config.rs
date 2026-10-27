//! 設定・キャッシュ・資格情報の保存

use crate::schedule::{CrmTimeZone, Filter, ScheduleItem};
use chrono::{DateTime, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const APP_DIR: &str = "fr-sch-widget";
const KEYRING_SERVICE: &str = "fr-sch-widget";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub crm_url: String,
    pub username: String,
    /// CRM サーバーが日時を保存しているタイムゾーン
    pub crm_time_zone: CrmTimeZone,
    pub interval_minutes: u32,
    pub reminder_minutes: Vec<u32>,
    pub soon_minutes: u32,
    pub notify_new_changed: bool,
    pub notify_deleted: bool,
    /// 空ならすべての種別を表示する
    pub activity_types: Vec<String>,
    pub show_not_held: bool,
    pub show_held: bool,
    pub always_on_top: bool,
    pub opacity: u32,
    /// "system" | "light" | "dark"
    pub theme: String,
    pub autostart: bool,
    /// 新しいバージョンを自動で確認する
    pub auto_update_check: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            crm_url: String::new(),
            username: String::new(),
            crm_time_zone: CrmTimeZone::Local,
            interval_minutes: 3,
            reminder_minutes: vec![10],
            soon_minutes: 15,
            notify_new_changed: true,
            notify_deleted: false,
            activity_types: Vec::new(),
            show_not_held: true,
            show_held: true,
            always_on_top: true,
            opacity: 90,
            theme: "system".into(),
            autostart: true,
            auto_update_check: true,
        }
    }
}

pub const INTERVAL_RANGE: (u32, u32) = (1, 30);
pub const OPACITY_RANGE: (u32, u32) = (50, 100);
/// リマインドに指定できる最大の分数（1日）
pub const MAX_REMINDER_MINUTES: u32 = 24 * 60;

impl Settings {
    /// 範囲外の値を丸め、文字列を整える（保存する前と読み込んだ後に通す）
    pub fn normalized(mut self) -> Self {
        self.crm_url = crate::crm::normalize_url(&self.crm_url);
        self.username = self.username.trim().to_string();
        self.interval_minutes = self.interval_minutes.clamp(INTERVAL_RANGE.0, INTERVAL_RANGE.1);
        self.opacity = self.opacity.clamp(OPACITY_RANGE.0, OPACITY_RANGE.1);
        self.soon_minutes = self.soon_minutes.min(MAX_REMINDER_MINUTES);
        self.reminder_minutes.retain(|&m| m <= MAX_REMINDER_MINUTES);
        self.reminder_minutes.sort_unstable_by(|a, b| b.cmp(a));
        self.reminder_minutes.dedup();
        self.activity_types = self
            .activity_types
            .iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
            self.theme = "system".into();
        }
        self
    }

    pub fn is_connection_configured(&self) -> bool {
        !self.crm_url.is_empty() && !self.username.is_empty()
    }

    pub fn filter(&self) -> Filter {
        Filter {
            activity_types: self.activity_types.clone(),
            show_held: self.show_held,
            show_not_held: self.show_not_held,
        }
    }
}

pub fn validate_url(url: &str) -> Result<(), String> {
    let lower = url.to_ascii_lowercase();
    if url.is_empty() || lower.starts_with("https://") || lower.starts_with("http://") {
        Ok(())
    } else {
        Err("CRM の URL は http:// か https:// で始めてください".into())
    }
}

/// 設定を保存するときのアクセスキーの扱い
#[derive(Debug, PartialEq, Eq)]
pub enum KeyAction {
    /// 入力されたキーを新しいユーザー名で保存する
    Save(String),
    /// ユーザー名だけ変わったので、前のユーザー名のキーを引き継ぐ
    Move,
    /// 何もしない
    Keep,
}

pub fn key_action(input_key: Option<&str>, old_username: &str, new_username: &str) -> KeyAction {
    match input_key.map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => KeyAction::Save(k.to_string()),
        None if !old_username.is_empty() && old_username != new_username => KeyAction::Move,
        None => KeyAction::Keep,
    }
}

/// `%APPDATA%\fr-sch-widget`
pub fn data_dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join(APP_DIR)
}

/// `%LOCALAPPDATA%\fr-sch-widget\logs`
pub fn log_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIR)
        .join("logs")
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            log::warn!("{} を読み込めませんでした: {e}", path.display());
            None
        }
    }
}

/// 一時ファイルに書いてから置き換える（書き込み中に落ちても壊れたファイルを残さない）
fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)
}

/// ログインに使ったトークンなど（`crm::AuthState`）。トークンだけではログインできないので秘密情報ではない
pub fn load_auth(dir: &Path) -> crate::crm::AuthState {
    read_json(&dir.join("auth.json")).unwrap_or_default()
}

pub fn save_auth(dir: &Path, auth: &crate::crm::AuthState) -> std::io::Result<()> {
    write_json(&dir.join("auth.json"), auth)
}

pub fn load_settings(dir: &Path) -> Settings {
    read_json::<Settings>(&dir.join("config.json")).unwrap_or_default().normalized()
}

pub fn save_settings(dir: &Path, settings: &Settings) -> std::io::Result<()> {
    write_json(&dir.join("config.json"), settings)
}

/// 前回取得した予定（その日の分だけ使う）
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cache {
    pub date: NaiveDate,
    pub last_updated: DateTime<Local>,
    pub items: Vec<ScheduleItem>,
}

pub fn load_cache(dir: &Path, today: NaiveDate) -> Option<Cache> {
    read_json::<Cache>(&dir.join("cache.json")).filter(|c| c.date == today)
}

pub fn save_cache(dir: &Path, cache: &Cache) -> std::io::Result<()> {
    write_json(&dir.join("cache.json"), cache)
}

/// アクセスキーを Windows 資格情報マネージャーに保存する
pub mod credentials {
    use super::KEYRING_SERVICE;

    fn entry(username: &str) -> keyring::Result<keyring::Entry> {
        keyring::Entry::new(KEYRING_SERVICE, username)
    }

    pub fn load(username: &str) -> Option<String> {
        if username.is_empty() {
            return None;
        }
        match entry(username).and_then(|e| e.get_password()) {
            Ok(k) if !k.is_empty() => Some(k),
            Ok(_) | Err(keyring::Error::NoEntry) => None,
            Err(e) => {
                log::warn!("資格情報を読み込めませんでした: {e}");
                None
            }
        }
    }

    pub fn save(username: &str, access_key: &str) -> Result<(), String> {
        entry(username).and_then(|e| e.set_password(access_key)).map_err(|e| e.to_string())
    }

    pub fn delete(username: &str) {
        if let Ok(e) = entry(username) {
            let _ = e.delete_credential();
        }
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
