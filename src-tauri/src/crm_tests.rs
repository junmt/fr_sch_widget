//! テスト仕様書 3章（crm.rs）。関数名の先頭はテストIDに対応する。
use super::*;
use mockito::{Matcher, Mock, Server, ServerGuard};
use serde_json::json;

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn range() -> (NaiveDate, NaiveDate) {
    (d("2026-10-26"), d("2026-10-28"))
}

fn session() -> Session {
    Session { name: "SESS123".into(), user_id: "19x5".into() }
}

fn ok(result: Value) -> String {
    json!({"success": true, "result": result}).to_string()
}

fn err(code: &str) -> String {
    json!({"success": false, "error": {"code": code, "message": "msg"}}).to_string()
}

fn q(param: &str, value: &str) -> Matcher {
    Matcher::UrlEncoded(param.into(), value.into())
}

fn rows(n: usize, offset: usize) -> Value {
    Value::Array((0..n).map(|i| json!({"id": format!("18x{}", offset + i)})).collect())
}

fn client(server: &ServerGuard) -> CrmClient {
    CrmClient::new(&server.url()).unwrap()
}

/// 指定した offset・SELECT の種類の query に応答するモック
async fn query_mock(server: &mut ServerGuard, offset: usize, select_all: bool, body: String, times: usize) -> Mock {
    let (from, to) = range();
    server
        .mock("GET", "/webservice.php")
        .match_query(Matcher::AllOf(vec![
            q("operation", "query"),
            q("query", &build_query("19x5", from, to, offset, select_all)),
        ]))
        .with_body(body)
        .expect(times)
        .create_async()
        .await
}

// ===== 3.1 normalize_url =====

#[test]
fn nu_normalize_url() {
    let cases = [
        ("NU-01", "https://crm.example.jp/frevocrm", "https://crm.example.jp/frevocrm"),
        ("NU-02", "  https://crm.example.jp/frevocrm/  ", "https://crm.example.jp/frevocrm"),
        ("NU-03", "https://crm.example.jp/frevocrm/index.php", "https://crm.example.jp/frevocrm"),
        ("NU-04", "https://crm.example.jp/frevocrm/INDEX.PHP", "https://crm.example.jp/frevocrm"),
        ("NU-05", "https://crm.example.jp/webservice.php", "https://crm.example.jp"),
        ("NU-06", "https://crm.example.jp//", "https://crm.example.jp"),
        ("NU-07", "", ""),
        ("NU-08", "https://crm.example.jp/frevocrm/index.php/", "https://crm.example.jp/frevocrm"),
    ];
    for (id, input, expected) in cases {
        assert_eq!(normalize_url(input), expected, "{id}");
    }
}

// ===== 3.2 record_url =====

#[test]
fn ru01_no_base_url() {
    assert_eq!(record_url("", "18x1234"), Err("CRM の URL が設定されていません".into()));
    assert!(record_url("  ", "18x1234").is_err());
}
#[test]
fn ru02_invalid_id() {
    for id in ["1234", "18xabc", ""] {
        assert!(record_url("https://crm", id).unwrap_err().contains("不正な ID"), "{id:?}");
    }
}
#[test]
fn ru03_detail_url() {
    for base in ["https://crm.example.jp/frevocrm", "https://crm.example.jp/frevocrm/"] {
        assert_eq!(
            record_url(base, "18x1234").unwrap(),
            "https://crm.example.jp/frevocrm/index.php?module=Calendar&view=Detail&record=1234"
        );
    }
}

// ===== 3.3 通信エラーの文言 =====

#[tokio::test]
async fn ms01_network_error_hides_url_with_session() {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let c = CrmClient::new(&format!("http://127.0.0.1:{port}")).unwrap();
    match c.query(&session(), "SELECT").await {
        Err(CrmError::Network(msg)) => {
            assert!(!msg.contains("SESS123"), "{msg}");
            assert!(!msg.contains("127.0.0.1"), "{msg}");
        }
        other => panic!("{other:?}"),
    }
}

// ===== 3.4 エラーコードの分類 =====

#[test]
fn ec01_session_codes() {
    for code in ["INVALID_SESSIONID", "SESSION_EXPIRED", "SESSION_LEFT_IDLE", "AUTHENTICATION_REQUIRED"] {
        let e = CrmError::from_code(code, "");
        assert_eq!(e, CrmError::SessionExpired, "{code}");
        assert!(!e.needs_user_action(), "{code}");
    }
}
#[test]
fn ec02_credential_codes() {
    for code in ["INVALID_USER_CREDENTIALS", "AUTHENTICATION_FAILURE", "ACCESSKEY_UNDEFINED"] {
        let e = CrmError::from_code(code, "");
        assert_eq!(e, CrmError::InvalidCredentials, "{code}");
        assert!(e.needs_user_action(), "{code}");
    }
}
#[test]
fn ec03_permission_codes() {
    for code in ["ACCESS_DENIED", "PERMISSION_DENIED"] {
        let e = CrmError::from_code(code, "");
        assert_eq!(e, CrmError::AccessDenied, "{code}");
        assert!(e.needs_user_action(), "{code}");
    }
}
#[test]
fn ec04_other_codes() {
    for code in ["QUERY_SYNTAX_ERROR", "INVALID_AUTH_TOKEN", ""] {
        let e = CrmError::from_code(code, "bad");
        assert_eq!(e, CrmError::Api { code: code.into(), message: "bad".into() }, "{code:?}");
        assert!(!e.needs_user_action(), "{code:?}");
    }
}
#[test]
fn ec05_network_and_bad_response_are_retryable() {
    assert!(!CrmError::Network("x".into()).needs_user_action());
    assert!(!CrmError::BadResponse("x".into()).needs_user_action());
}

// ===== 3.5 応答の解釈 =====

#[tokio::test]
async fn rs01a_connection_refused() {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let c = CrmClient::new(&format!("http://127.0.0.1:{port}")).unwrap();
    assert!(matches!(c.query(&session(), "SELECT").await, Err(CrmError::Network(_))));
}
#[tokio::test]
async fn rs01b_timeout() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/webservice.php")
        .match_query(Matcher::Any)
        .with_chunked_body(|w| {
            std::thread::sleep(Duration::from_millis(800));
            w.write_all(ok(json!([])).as_bytes())
        })
        .create_async()
        .await;
    let c = CrmClient::with_timeout(&server.url(), Duration::from_millis(200)).unwrap();
    assert_eq!(c.query(&session(), "SELECT").await, Err(CrmError::Network("タイムアウト".into())));
}
#[tokio::test]
async fn rs02_html_body() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/webservice.php")
        .match_query(Matcher::Any)
        .with_status(500)
        .with_body("<html>Fatal error</html>")
        .create_async()
        .await;
    match client(&server).query(&session(), "SELECT").await {
        Err(CrmError::BadResponse(msg)) => {
            assert!(msg.contains("500"), "{msg}");
            assert!(msg.contains("<html>Fatal error</html>"), "{msg}");
        }
        other => panic!("{other:?}"),
    }
}
#[tokio::test]
async fn rs03_success_missing() {
    let mut server = Server::new_async().await;
    server.mock("GET", "/webservice.php").match_query(Matcher::Any).with_body(r#"{"result": []}"#).create_async().await;
    assert!(matches!(client(&server).query(&session(), "SELECT").await, Err(CrmError::BadResponse(_))));
}
#[tokio::test]
async fn rs04_success_true_returns_result() {
    let mut server = Server::new_async().await;
    server.mock("GET", "/webservice.php").match_query(Matcher::Any).with_body(ok(rows(2, 0))).create_async().await;
    assert_eq!(client(&server).query(&session(), "SELECT").await.unwrap().len(), 2);
}
#[tokio::test]
async fn rs05_success_false_is_classified() {
    let mut server = Server::new_async().await;
    for (code, expected) in [
        ("INVALID_SESSIONID", CrmError::SessionExpired),
        ("ACCESS_DENIED", CrmError::AccessDenied),
        ("QUERY_SYNTAX_ERROR", CrmError::Api { code: "QUERY_SYNTAX_ERROR".into(), message: "msg".into() }),
    ] {
        let m = server.mock("GET", "/webservice.php").match_query(Matcher::Any).with_body(err(code)).create_async().await;
        assert_eq!(client(&server).query(&session(), "SELECT").await, Err(expected), "{code}");
        m.remove_async().await;
    }
}
#[tokio::test]
async fn rs05b_error_without_code() {
    let mut server = Server::new_async().await;
    server.mock("GET", "/webservice.php").match_query(Matcher::Any).with_body(r#"{"success": false}"#).create_async().await;
    assert_eq!(
        client(&server).query(&session(), "SELECT").await,
        Err(CrmError::Api { code: "".into(), message: "".into() })
    );
}
#[tokio::test]
async fn rs06_result_missing_is_null() {
    // result がないと null。query は配列を期待するので BadResponse（QY-03 と同じ）
    let mut server = Server::new_async().await;
    server.mock("GET", "/webservice.php").match_query(Matcher::Any).with_body(r#"{"success": true}"#).create_async().await;
    assert!(matches!(client(&server).query(&session(), "SELECT").await, Err(CrmError::BadResponse(_))));
}
#[tokio::test]
async fn rs07_http_500_with_json_error() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/webservice.php")
        .match_query(Matcher::Any)
        .with_status(500)
        .with_body(err("INVALID_SESSIONID"))
        .create_async()
        .await;
    assert_eq!(client(&server).query(&session(), "SELECT").await, Err(CrmError::SessionExpired));
}
#[tokio::test]
async fn rs08_long_body_is_truncated() {
    let mut server = Server::new_async().await;
    let body = format!("<html>{}</html>", "あ".repeat(500));
    server.mock("GET", "/webservice.php").match_query(Matcher::Any).with_body(body).create_async().await;
    match client(&server).query(&session(), "SELECT").await {
        Err(CrmError::BadResponse(msg)) => {
            let head = msg.split_once(": ").unwrap().1;
            assert_eq!(head.chars().count(), 200, "{msg}");
        }
        other => panic!("{other:?}"),
    }
}

// ===== 3.6 login =====

async fn challenge_mock(server: &mut ServerGuard, body: String, times: usize) -> Mock {
    server
        .mock("GET", "/webservice.php")
        .match_query(Matcher::AllOf(vec![q("operation", "getchallenge"), q("username", "jun")]))
        .with_body(body)
        .expect(times)
        .create_async()
        .await
}

async fn login_mock(server: &mut ServerGuard, body: String, times: usize) -> Mock {
    server
        .mock("POST", "/webservice.php")
        .match_body(Matcher::AllOf(vec![q("operation", "login"), q("username", "jun")]))
        .with_body(body)
        .expect(times)
        .create_async()
        .await
}

#[tokio::test]
async fn lg01_success_sends_md5() {
    let mut server = Server::new_async().await;
    let expected_key = format!("{:x}", md5::compute("TOKENSECRET"));
    assert_eq!(expected_key.len(), 32);
    let lg = server
        .mock("POST", "/crm/webservice.php")
        .match_header("content-type", "application/x-www-form-urlencoded")
        .match_body(Matcher::AllOf(vec![q("operation", "login"), q("username", "jun"), q("accessKey", &expected_key)]))
        .with_body(ok(json!({"sessionName": "S1", "userId": "19x5"})))
        .expect(1)
        .create_async()
        .await;
    // URL の末尾の / を除いて /crm/webservice.php を呼ぶこと
    let ch = server
        .mock("GET", "/crm/webservice.php")
        .match_query(Matcher::AllOf(vec![q("operation", "getchallenge"), q("username", "jun")]))
        .with_body(ok(json!({"token": "TOKEN", "serverTime": 1, "expireTime": 2})))
        .expect(1)
        .create_async()
        .await;
    let c = CrmClient::new(&format!("{}/crm/", server.url())).unwrap();
    assert_eq!(c.login("jun", "SECRET").await.unwrap(), Session { name: "S1".into(), user_id: "19x5".into() });
    ch.assert_async().await;
    lg.assert_async().await;
}
#[tokio::test]
async fn lg02_challenge_error_skips_login() {
    let mut server = Server::new_async().await;
    challenge_mock(&mut server, err("INTERNAL_SERVER_ERROR"), 1).await;
    let lg = login_mock(&mut server, ok(json!({})), 0).await;
    assert!(matches!(client(&server).login("jun", "k").await, Err(CrmError::Api { .. })));
    lg.assert_async().await;
}
#[tokio::test]
async fn lg03_no_token_skips_login() {
    let mut server = Server::new_async().await;
    challenge_mock(&mut server, ok(json!({})), 1).await;
    let lg = login_mock(&mut server, ok(json!({})), 0).await;
    assert!(matches!(client(&server).login("jun", "k").await, Err(CrmError::BadResponse(_))));
    lg.assert_async().await;
}
#[tokio::test]
async fn lg04_login_error() {
    let mut server = Server::new_async().await;
    challenge_mock(&mut server, ok(json!({"token": "T"})), 1).await;
    login_mock(&mut server, err("INVALID_USER_CREDENTIALS"), 1).await;
    assert_eq!(client(&server).login("jun", "bad").await, Err(CrmError::InvalidCredentials));
}
#[tokio::test]
async fn lg05_lg06_lg07_incomplete_login_result() {
    for (id, body) in [
        ("LG-05", json!({"userId": "19x5"})),
        ("LG-06", json!({"sessionName": "", "userId": "19x5"})),
        ("LG-07", json!({"sessionName": "S"})),
        ("LG-07b", json!({"sessionName": "S", "userId": ""})),
    ] {
        let mut server = Server::new_async().await;
        challenge_mock(&mut server, ok(json!({"token": "T"})), 1).await;
        let lg = login_mock(&mut server, ok(body), 1).await;
        assert!(matches!(client(&server).login("jun", "k").await, Err(CrmError::BadResponse(_))), "{id}");
        lg.assert_async().await;
    }
}

// ===== 3.7 query / fetch_events =====

#[tokio::test]
async fn qy01_query_sends_params_and_returns_rows() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/webservice.php")
        .match_query(Matcher::AllOf(vec![q("operation", "query"), q("sessionName", "SESS123"), q("query", "SELECT x;")]))
        .with_body(ok(rows(3, 0)))
        .expect(1)
        .create_async()
        .await;
    assert_eq!(client(&server).query(&session(), "SELECT x;").await.unwrap(), rows(3, 0).as_array().unwrap().clone());
    m.assert_async().await;
}
#[tokio::test]
async fn qy02_result_not_array() {
    let mut server = Server::new_async().await;
    server.mock("GET", "/webservice.php").match_query(Matcher::Any).with_body(ok(json!({"a": 1}))).create_async().await;
    assert!(matches!(client(&server).query(&session(), "SELECT").await, Err(CrmError::BadResponse(_))));
}
#[tokio::test]
async fn qy03_result_null() {
    let mut server = Server::new_async().await;
    server.mock("GET", "/webservice.php").match_query(Matcher::Any).with_body(ok(Value::Null)).create_async().await;
    assert!(matches!(client(&server).query(&session(), "SELECT").await, Err(CrmError::BadResponse(_))));
}
#[tokio::test]
async fn fe01_short_page_ends() {
    let mut server = Server::new_async().await;
    let m = query_mock(&mut server, 0, false, ok(rows(99, 0)), 1).await;
    let (from, to) = range();
    assert_eq!(client(&server).fetch_events(&session(), from, to).await.unwrap().len(), 99);
    m.assert_async().await;
}
#[tokio::test]
async fn fe02_fe04_full_pages_continue() {
    let mut server = Server::new_async().await;
    let mut mocks = Vec::new();
    for (offset, n) in [(0, 100), (100, 100), (200, 5)] {
        mocks.push(query_mock(&mut server, offset, false, ok(rows(n, offset)), 1).await);
    }
    let (from, to) = range();
    let all = client(&server).fetch_events(&session(), from, to).await.unwrap();
    assert_eq!(all.len(), 205);
    assert_eq!(all[0]["id"], "18x0");
    assert_eq!(all[204]["id"], "18x204");
    for m in mocks {
        m.assert_async().await;
    }
}
#[tokio::test]
async fn fe03_fe07_error_on_later_page() {
    let mut server = Server::new_async().await;
    query_mock(&mut server, 0, false, ok(rows(100, 0)), 1).await;
    let (from, to) = range();
    server
        .mock("GET", "/webservice.php")
        .match_query(q("query", &build_query("19x5", from, to, 100, false)))
        .with_status(502)
        .with_body("Bad Gateway")
        .create_async()
        .await;
    assert!(matches!(client(&server).fetch_events(&session(), from, to).await, Err(CrmError::BadResponse(_))));
}
#[tokio::test]
async fn fe05_exact_page_then_empty() {
    let mut server = Server::new_async().await;
    let a = query_mock(&mut server, 0, false, ok(rows(100, 0)), 1).await;
    let b = query_mock(&mut server, 100, false, ok(rows(0, 0)), 1).await;
    let (from, to) = range();
    assert_eq!(client(&server).fetch_events(&session(), from, to).await.unwrap().len(), 100);
    a.assert_async().await;
    b.assert_async().await;
}
#[tokio::test]
async fn fe06_no_rows() {
    let mut server = Server::new_async().await;
    let m = query_mock(&mut server, 0, false, ok(rows(0, 0)), 1).await;
    let (from, to) = range();
    assert!(client(&server).fetch_events(&session(), from, to).await.unwrap().is_empty());
    m.assert_async().await;
}
#[tokio::test]
async fn fb01_api_error_falls_back_and_remembers() {
    let mut server = Server::new_async().await;
    let explicit = query_mock(&mut server, 0, false, err("QUERY_SYNTAX_ERROR"), 1).await;
    let star = query_mock(&mut server, 0, true, ok(rows(3, 0)), 2).await;
    let c = client(&server);
    let (from, to) = range();
    assert_eq!(c.fetch_events(&session(), from, to).await.unwrap().len(), 3);
    // FB-05: 2回目は最初から SELECT *
    assert_eq!(c.fetch_events(&session(), from, to).await.unwrap().len(), 3);
    explicit.assert_async().await;
    star.assert_async().await;
}
#[tokio::test]
async fn fb02_fallback_also_fails() {
    let mut server = Server::new_async().await;
    query_mock(&mut server, 0, false, err("QUERY_SYNTAX_ERROR"), 1).await;
    let star = query_mock(&mut server, 0, true, err("DATABASE_QUERY_ERROR"), 2).await;
    let c = client(&server);
    let (from, to) = range();
    assert!(matches!(c.fetch_events(&session(), from, to).await, Err(CrmError::Api { code, .. }) if code == "DATABASE_QUERY_ERROR"));
    // 切り替えは覚えている
    assert!(c.fetch_events(&session(), from, to).await.is_err());
    star.assert_async().await;
}
#[tokio::test]
async fn fb03_non_api_error_does_not_fall_back() {
    let mut server = Server::new_async().await;
    let explicit = query_mock(&mut server, 0, false, err("INVALID_SESSIONID"), 1).await;
    let star = query_mock(&mut server, 0, true, ok(rows(3, 0)), 0).await;
    let (from, to) = range();
    assert_eq!(client(&server).fetch_events(&session(), from, to).await, Err(CrmError::SessionExpired));
    explicit.assert_async().await;
    star.assert_async().await;
}
#[tokio::test]
async fn fb04_success_keeps_explicit_fields() {
    let mut server = Server::new_async().await;
    let explicit = query_mock(&mut server, 0, false, ok(rows(1, 0)), 2).await;
    let star = query_mock(&mut server, 0, true, ok(rows(1, 0)), 0).await;
    let c = client(&server);
    let (from, to) = range();
    c.fetch_events(&session(), from, to).await.unwrap();
    c.fetch_events(&session(), from, to).await.unwrap();
    explicit.assert_async().await;
    star.assert_async().await;
}

// ===== 3.8 fetch_with_relogin =====

/// login が成功する（sessionName = NEW）モック
async fn login_ok(server: &mut ServerGuard, times: usize) -> Mock {
    challenge_mock(server, ok(json!({"token": "T"})), times).await;
    login_mock(server, ok(json!({"sessionName": "NEW", "userId": "19x5"})), times).await
}

/// 指定したセッションの query に応答するモック
async fn session_query(server: &mut ServerGuard, sess: &str, body: String, times: usize) -> Mock {
    server
        .mock("GET", "/webservice.php")
        .match_query(Matcher::AllOf(vec![q("operation", "query"), q("sessionName", sess)]))
        .with_body(body)
        .expect(times)
        .create_async()
        .await
}

async fn relogin(server: &ServerGuard, sess: &mut Option<Session>) -> Result<Vec<Value>, CrmError> {
    let (from, to) = range();
    client(server).fetch_with_relogin(sess, "jun", "k", from, to).await
}

fn new_session() -> Option<Session> {
    Some(Session { name: "NEW".into(), user_id: "19x5".into() })
}

#[tokio::test]
async fn rl01_no_session_login_then_fetch() {
    let mut server = Server::new_async().await;
    let lg = login_ok(&mut server, 1).await;
    let qm = session_query(&mut server, "NEW", ok(rows(2, 0)), 1).await;
    let mut sess = None;
    assert_eq!(relogin(&server, &mut sess).await.unwrap().len(), 2);
    assert_eq!(sess, new_session());
    lg.assert_async().await;
    qm.assert_async().await;
}
#[tokio::test]
async fn rl02_login_fails() {
    let mut server = Server::new_async().await;
    challenge_mock(&mut server, ok(json!({"token": "T"})), 1).await;
    login_mock(&mut server, err("INVALID_USER_CREDENTIALS"), 1).await;
    let qm = session_query(&mut server, "NEW", ok(rows(1, 0)), 0).await;
    let mut sess = None;
    assert_eq!(relogin(&server, &mut sess).await, Err(CrmError::InvalidCredentials));
    assert_eq!(sess, None);
    qm.assert_async().await;
}
#[tokio::test]
async fn rl03_fresh_session_expired_does_not_loop() {
    let mut server = Server::new_async().await;
    let lg = login_ok(&mut server, 1).await;
    session_query(&mut server, "NEW", err("INVALID_SESSIONID"), 1).await;
    let mut sess = None;
    assert_eq!(relogin(&server, &mut sess).await, Err(CrmError::SessionExpired));
    assert_eq!(sess, None);
    lg.assert_async().await;
}
#[tokio::test]
async fn rl04_existing_session_ok() {
    let mut server = Server::new_async().await;
    let lg = login_ok(&mut server, 0).await;
    session_query(&mut server, "SESS123", ok(rows(1, 0)), 1).await;
    let mut sess = Some(session());
    assert_eq!(relogin(&server, &mut sess).await.unwrap().len(), 1);
    assert_eq!(sess, Some(session()));
    lg.assert_async().await;
}
#[tokio::test]
async fn rl05_expired_then_relogin_ok() {
    let mut server = Server::new_async().await;
    let lg = login_ok(&mut server, 1).await;
    session_query(&mut server, "SESS123", err("SESSION_EXPIRED"), 1).await;
    let qm = session_query(&mut server, "NEW", ok(rows(4, 0)), 1).await;
    let mut sess = Some(session());
    assert_eq!(relogin(&server, &mut sess).await.unwrap().len(), 4);
    assert_eq!(sess, new_session());
    lg.assert_async().await;
    qm.assert_async().await;
}
#[tokio::test]
async fn rl06_expired_twice() {
    let mut server = Server::new_async().await;
    let lg = login_ok(&mut server, 1).await;
    session_query(&mut server, "SESS123", err("INVALID_SESSIONID"), 1).await;
    let qm = session_query(&mut server, "NEW", err("INVALID_SESSIONID"), 1).await;
    let mut sess = Some(session());
    assert_eq!(relogin(&server, &mut sess).await, Err(CrmError::SessionExpired));
    assert_eq!(sess, None);
    lg.assert_async().await;
    qm.assert_async().await;
}
#[tokio::test]
async fn rl07_relogin_fails() {
    let mut server = Server::new_async().await;
    challenge_mock(&mut server, ok(json!({"token": "T"})), 1).await;
    let lg = login_mock(&mut server, err("INVALID_USER_CREDENTIALS"), 1).await;
    session_query(&mut server, "SESS123", err("INVALID_SESSIONID"), 1).await;
    let mut sess = Some(session());
    assert_eq!(relogin(&server, &mut sess).await, Err(CrmError::InvalidCredentials));
    assert_eq!(sess, None);
    lg.assert_async().await;
}
#[tokio::test]
async fn rl08_user_action_error_clears_session() {
    for code in ["ACCESS_DENIED", "INVALID_USER_CREDENTIALS"] {
        let mut server = Server::new_async().await;
        let lg = login_ok(&mut server, 0).await;
        session_query(&mut server, "SESS123", err(code), 1).await;
        let mut sess = Some(session());
        assert!(relogin(&server, &mut sess).await.unwrap_err().needs_user_action(), "{code}");
        assert_eq!(sess, None, "{code}");
        lg.assert_async().await;
    }
}
#[tokio::test]
async fn rl09_retryable_error_keeps_session() {
    let mut server = Server::new_async().await;
    let lg = login_ok(&mut server, 0).await;
    server
        .mock("GET", "/webservice.php")
        .match_query(Matcher::AllOf(vec![q("operation", "query"), q("sessionName", "SESS123")]))
        .with_status(503)
        .with_body("Service Unavailable")
        .create_async()
        .await;
    let mut sess = Some(session());
    assert!(matches!(relogin(&server, &mut sess).await, Err(CrmError::BadResponse(_))));
    assert_eq!(sess, Some(session()));
    lg.assert_async().await;
}

// ===== 3.9 logout =====

#[tokio::test]
async fn lo01_lo02_logout_posts_and_ignores_errors() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("POST", "/webservice.php")
        .match_body(Matcher::AllOf(vec![q("operation", "logout"), q("sessionName", "SESS123")]))
        .with_status(500)
        .expect(1)
        .create_async()
        .await;
    client(&server).logout(&session()).await;
    m.assert_async().await;
}
