//! F-RevoCRM 標準 Webservice API（webservice.php）のクライアント

use crate::schedule::{build_query, QUERY_PAGE_SIZE};
use chrono::{DateTime, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CrmError {
    #[error("接続できません（{0}）")]
    Network(String),
    #[error("セッションが切れました")]
    SessionExpired,
    #[error("ログインできません：ユーザー名とアクセスキーを確認してください")]
    InvalidCredentials,
    #[error("予定を見る権限がありません")]
    AccessDenied,
    #[error("CRMのエラー：{code} {message}")]
    Api { code: String, message: String },
    #[error("CRMから予期しない応答がありました")]
    BadResponse(String),
    /// F-RevoCRM の不具合で、前の認証から5分たつまではログインできない（`retry_at` に自動で再試行する）
    #[error("CRM の認証の制限（5分に1回）のため、{} に再試行します", .retry_at.format("%H:%M"))]
    LoginBlocked { retry_at: DateTime<Local> },
}

impl CrmError {
    fn from_code(code: &str, message: &str) -> Self {
        match code {
            "INVALID_SESSIONID" | "SESSION_EXPIRED" | "SESSION_LEFT_IDLE" | "AUTHENTICATION_REQUIRED" => {
                CrmError::SessionExpired
            }
            "INVALID_USER_CREDENTIALS" | "AUTHENTICATION_FAILURE" | "ACCESSKEY_UNDEFINED" => {
                CrmError::InvalidCredentials
            }
            "ACCESS_DENIED" | "PERMISSION_DENIED" => CrmError::AccessDenied,
            _ => CrmError::Api { code: code.to_string(), message: message.to_string() },
        }
    }

    /// 自動で再試行しない（設定の見直しが必要な）エラーか
    pub fn needs_user_action(&self) -> bool {
        matches!(self, CrmError::InvalidCredentials | CrmError::AccessDenied)
    }

    /// 次に再試行する時刻が決まっているエラーなら、その時刻
    pub fn retry_at(&self) -> Option<DateTime<Local>> {
        match self {
            CrmError::LoginBlocked { retry_at } => Some(*retry_at),
            _ => None,
        }
    }
}

/// getchallenge の結果が来なかったときのトークンの有効期間（F-RevoCRM は5分）
const DEFAULT_CHALLENGE_SECONDS: i64 = 300;
/// 有効期限ぎりぎりのトークンは使わない（時計のずれと通信の時間の分）
pub const CHALLENGE_MARGIN_SECONDS: i64 = 15;
/// 認証の制限が解けたあと、少し待ってから再試行する
pub const BLOCK_EXTRA_SECONDS: i64 = 10;

/// ログインに成功したチャレンジトークン（CRM の DB に保存されていることが確かなもの）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Challenge {
    pub endpoint: String,
    pub username: String,
    pub token: String,
    /// この PC の時計での有効期限
    pub expires_at: DateTime<Utc>,
}

impl Challenge {
    /// まだ使えるか（同じ CRM・同じユーザーで、有効期限まで余裕がある）
    pub fn usable(&self, endpoint: &str, username: &str, now: DateTime<Utc>) -> bool {
        self.endpoint == endpoint
            && self.username == username
            && now < self.expires_at - chrono::Duration::seconds(CHALLENGE_MARGIN_SECONDS)
    }
}

/// ログインの状態（起動し直しても引き継ぐ）。
///
/// F-RevoCRM の不具合への対策：CRM は1ユーザーに有効なトークンを1つしか持てないのに、
/// 有効なトークンがある間（5分）に getchallenge を呼ぶと、保存できなかった新しいトークンを返す。
/// そのトークンでの login は必ず「ユーザー名かアクセスキーが違う」になる。そこで、
/// - ログインに成功したトークンを覚えておき、有効な間は getchallenge を呼ばずに使い回す
/// - 新しいトークンでの失敗は、まず「認証の制限」とみなし、トークンが切れる5分後に再試行する。
///   それでも失敗したら、本当にアクセスキーが違うとみなす
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AuthState {
    pub challenge: Option<Challenge>,
    /// 認証の制限が解けるはずの時刻（この時刻を過ぎても失敗したら、本当の認証エラー）
    pub blocked_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub name: String,
    pub user_id: String,
}

/// CRM の URL を正規化する（前後の空白、末尾の `/`、`/index.php` や `/webservice.php` を取り除く）
pub fn normalize_url(url: &str) -> String {
    let mut u = url.trim().trim_end_matches('/').to_string();
    for suffix in ["/index.php", "/webservice.php"] {
        if u.to_ascii_lowercase().ends_with(suffix) {
            u.truncate(u.len() - suffix.len());
        }
    }
    u.trim_end_matches('/').to_string()
}

/// 予定の詳細画面の URL（`18x1234` → record=1234）
pub fn record_url(base_url: &str, id: &str) -> Result<String, String> {
    let base = normalize_url(base_url);
    if base.is_empty() {
        return Err("CRM の URL が設定されていません".into());
    }
    let record_id: u64 = id
        .rsplit_once('x')
        .and_then(|(_, n)| n.parse().ok())
        .ok_or_else(|| format!("不正な ID です: {id}"))?;
    Ok(format!("{base}/index.php?module=Calendar&view=Detail&record={record_id}"))
}

pub struct CrmClient {
    http: reqwest::Client,
    endpoint: String,
    /// フィールドを指定した SELECT が失敗したら、以降は SELECT * を使う（古いバージョン向け）
    select_all: AtomicBool,
}

impl CrmClient {
    pub fn new(base_url: &str) -> Result<Self, CrmError> {
        Self::with_timeout(base_url, TIMEOUT)
    }

    pub fn with_timeout(base_url: &str, timeout: Duration) -> Result<Self, CrmError> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent(concat!("fr-sch-widget/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| CrmError::Network(e.to_string()))?;
        Ok(Self {
            http,
            endpoint: format!("{}/webservice.php", normalize_url(base_url)),
            select_all: AtomicBool::new(false),
        })
    }

    async fn parse(resp: Result<reqwest::Response, reqwest::Error>) -> Result<Value, CrmError> {
        let resp = resp.map_err(|e| CrmError::Network(describe_reqwest(&e)))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| CrmError::Network(describe_reqwest(&e)))?;
        let json: Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(_) => {
                let head: String = body.chars().take(200).collect();
                return Err(CrmError::BadResponse(format!("HTTP {status}: {head}")));
            }
        };
        match json.get("success").and_then(Value::as_bool) {
            Some(true) => Ok(json.get("result").cloned().unwrap_or(Value::Null)),
            Some(false) => {
                let err = json.get("error");
                let field = |k: &str| err.and_then(|e| e.get(k)).and_then(Value::as_str).unwrap_or("").to_string();
                Err(CrmError::from_code(&field("code"), &field("message")))
            }
            None => Err(CrmError::BadResponse(format!("HTTP {status}: success がありません"))),
        }
    }

    async fn get(&self, params: &[(&str, &str)]) -> Result<Value, CrmError> {
        Self::parse(self.http.get(&self.endpoint).query(params).send().await).await
    }

    async fn post(&self, form: &[(&str, &str)]) -> Result<Value, CrmError> {
        Self::parse(self.http.post(&self.endpoint).form(form).send().await).await
    }

    /// ログインする。使えるトークンがあれば使い回し、なければ getchallenge から始める（`AuthState` を参照）
    pub async fn login(
        &self,
        username: &str,
        access_key: &str,
        auth: &mut AuthState,
        now: DateTime<Utc>,
    ) -> Result<Session, CrmError> {
        if let Some(c) = auth.challenge.clone().filter(|c| c.usable(&self.endpoint, username, now)) {
            match self.login_with_token(username, access_key, &c.token).await {
                Ok(s) => {
                    auth.blocked_until = None;
                    return Ok(s);
                }
                // DB にあるはずのトークンで失敗した＝アクセスキーが違う
                Err(CrmError::InvalidCredentials) => {
                    auth.challenge = None;
                    auth.blocked_until = None;
                    return Err(CrmError::InvalidCredentials);
                }
                // トークンが消えていた（CRM 側で削除された、時計が大きくずれているなど）。取り直す
                Err(CrmError::Api { code, .. }) if code == "INVALID_AUTH_TOKEN" => auth.challenge = None,
                Err(e) => return Err(e),
            }
        }

        let challenge = self.get(&[("operation", "getchallenge"), ("username", username)]).await?;
        let token = challenge
            .get("token")
            .and_then(Value::as_str)
            .ok_or_else(|| CrmError::BadResponse("getchallenge に token がありません".into()))?
            .to_string();
        let lifetime = match (challenge.get("expireTime").and_then(Value::as_i64), challenge.get("serverTime").and_then(Value::as_i64)) {
            (Some(exp), Some(server)) if exp > server => exp - server,
            _ => DEFAULT_CHALLENGE_SECONDS,
        };
        let expires_at = now + chrono::Duration::seconds(lifetime.min(3600));

        match self.login_with_token(username, access_key, &token).await {
            Ok(s) => {
                auth.challenge = Some(Challenge {
                    endpoint: self.endpoint.clone(),
                    username: username.to_string(),
                    token,
                    expires_at,
                });
                auth.blocked_until = None;
                Ok(s)
            }
            Err(CrmError::InvalidCredentials) => match auth.blocked_until {
                // 制限が解けるまで待ったのに失敗した＝アクセスキーが違う
                Some(until) if now >= until => {
                    auth.blocked_until = None;
                    Err(CrmError::InvalidCredentials)
                }
                Some(until) => Err(CrmError::LoginBlocked { retry_at: until.with_timezone(&Local) }),
                None => {
                    let until = expires_at + chrono::Duration::seconds(BLOCK_EXTRA_SECONDS);
                    auth.blocked_until = Some(until);
                    Err(CrmError::LoginBlocked { retry_at: until.with_timezone(&Local) })
                }
            },
            Err(e) => Err(e),
        }
    }

    async fn login_with_token(&self, username: &str, access_key: &str, token: &str) -> Result<Session, CrmError> {
        let hashed = format!("{:x}", md5::compute(format!("{token}{access_key}")));
        let result = self
            .post(&[("operation", "login"), ("username", username), ("accessKey", &hashed)])
            .await?;
        let field = |k: &str| result.get(k).and_then(Value::as_str).map(str::to_string);
        match (field("sessionName"), field("userId")) {
            (Some(name), Some(user_id)) if !name.is_empty() && !user_id.is_empty() => Ok(Session { name, user_id }),
            _ => Err(CrmError::BadResponse("login に sessionName / userId がありません".into())),
        }
    }

    pub async fn logout(&self, session: &Session) {
        let _ = self.post(&[("operation", "logout"), ("sessionName", &session.name)]).await;
    }

    pub async fn query(&self, session: &Session, vtql: &str) -> Result<Vec<Value>, CrmError> {
        let result = self
            .get(&[("operation", "query"), ("sessionName", &session.name), ("query", vtql)])
            .await?;
        match result {
            Value::Array(rows) => Ok(rows),
            other => Err(CrmError::BadResponse(format!("query の結果が配列ではありません: {other}"))),
        }
    }

    /// 期間にかかる自分の予定をすべて取得する（100件ずつページング）
    pub async fn fetch_events(&self, session: &Session, from: NaiveDate, to: NaiveDate) -> Result<Vec<Value>, CrmError> {
        let mut all = Vec::new();
        let mut offset = 0;
        loop {
            let page = self.query_page(session, from, to, offset).await?;
            let n = page.len();
            all.extend(page);
            if n < QUERY_PAGE_SIZE {
                return Ok(all);
            }
            offset += QUERY_PAGE_SIZE;
        }
    }

    /// セッションがなければログインしてから取得する。
    /// 使い回したセッションが切れていたら、1回だけログインし直して取り直す。
    /// 失敗したときは、使えなくなったセッションを `session` から消す。
    #[allow(clippy::too_many_arguments)]
    pub async fn fetch_with_relogin(
        &self,
        session: &mut Option<Session>,
        auth: &mut AuthState,
        username: &str,
        access_key: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<Value>, CrmError> {
        let mut retried = false;
        loop {
            let reused = session.is_some();
            if !reused {
                *session = Some(self.login(username, access_key, auth, Utc::now()).await?);
                log::info!("ログインしました");
            }
            let current = session.clone().expect("セッションは直前に作った");
            match self.fetch_events(&current, from, to).await {
                Err(CrmError::SessionExpired) => {
                    *session = None;
                    if reused && !retried {
                        log::info!("セッションが切れたため、ログインし直します");
                        retried = true;
                        continue;
                    }
                    return Err(CrmError::SessionExpired);
                }
                Err(e) if e.needs_user_action() => {
                    *session = None;
                    return Err(e);
                }
                other => return other,
            }
        }
    }

    async fn query_page(&self, session: &Session, from: NaiveDate, to: NaiveDate, offset: usize) -> Result<Vec<Value>, CrmError> {
        let select_all = self.select_all.load(Ordering::Relaxed);
        let vtql = build_query(&session.user_id, from, to, offset, select_all);
        match self.query(session, &vtql).await {
            Err(CrmError::Api { code, message }) if !select_all => {
                log::warn!("フィールドを指定した query が失敗したため SELECT * で取り直します: {code} {message}");
                self.select_all.store(true, Ordering::Relaxed);
                self.query(session, &build_query(&session.user_id, from, to, offset, true)).await
            }
            other => other,
        }
    }
}

fn describe_reqwest(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "タイムアウト".into()
    } else if e.is_connect() {
        "サーバーにつながりません".into()
    } else {
        // URL に sessionName が含まれることがあるため、URL は出さない
        let s = e.to_string();
        match e.url() {
            Some(url) => s.replace(url.as_str(), "(URL)"),
            None => s,
        }
    }
}

#[cfg(test)]
#[path = "crm_tests.rs"]
mod tests;
