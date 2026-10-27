//! F-RevoCRM 標準 Webservice API（webservice.php）のクライアント

use crate::schedule::{build_query, QUERY_PAGE_SIZE};
use chrono::NaiveDate;
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

    /// getchallenge → login
    pub async fn login(&self, username: &str, access_key: &str) -> Result<Session, CrmError> {
        let challenge = self.get(&[("operation", "getchallenge"), ("username", username)]).await?;
        let token = challenge
            .get("token")
            .and_then(Value::as_str)
            .ok_or_else(|| CrmError::BadResponse("getchallenge に token がありません".into()))?;
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
    pub async fn fetch_with_relogin(
        &self,
        session: &mut Option<Session>,
        username: &str,
        access_key: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<Value>, CrmError> {
        let mut retried = false;
        loop {
            let reused = session.is_some();
            if !reused {
                *session = Some(self.login(username, access_key).await?);
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
