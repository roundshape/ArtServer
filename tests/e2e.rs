//! エンドツーエンドのテスト。実際に待ち受けたサーバーに、rmcp の MCP クライアントと
//! 素の HTTP クライアントで接続する。

// テストの補助関数は #[test] の外にあるので、テスト用の許可をファイル全体に与える
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use artserver::config::{Config, Limits};
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, ClientConfig};
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use serde_json::{Value, json};
use tokio::net::TcpListener;

const TOKEN: &str = "test-token-0123456789abcdef0123456789abcdef";

async fn start(limits: Limits) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let config = Config {
        bind: addr,
        token: TOKEN.into(),
        allowed_hosts: vec!["127.0.0.1".into(), "localhost".into()],
        allowed_origins: vec![],
        limits,
    };
    tokio::spawn(artserver::serve(config, listener, std::future::pending()));
    format!("http://{addr}")
}

async fn client(base: &str, token: &str) -> RunningService<RoleClient, ClientConfig> {
    let transport =
        StreamableHttpClientTransport::from_config(StreamableHttpClientTransportConfig::with_uri(format!("{base}/mcp")).auth_header(token));
    ClientConfig::default().serve(transport).await.unwrap()
}

async fn call(client: &RunningService<RoleClient, ClientConfig>, tool: &'static str, args: Value) -> CallToolResult {
    let Value::Object(args) = args else {
        panic!("arguments must be an object")
    };
    client.call_tool(CallToolRequestParams::new(tool).with_arguments(args)).await.unwrap()
}

fn ok(result: &CallToolResult) -> &Value {
    assert_ne!(result.is_error, Some(true), "tool failed: {result:?}");
    result.structured_content.as_ref().unwrap()
}

fn text_of(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn ai_draws_a_rectangle_and_exports_svg_pdf_and_png() {
    let base = start(Limits::default()).await;
    let client = client(&base, TOKEN).await;

    let tools = client.list_all_tools().await.unwrap();
    for name in ["session_create", "document_new", "draw_rectangle", "export_document"] {
        assert!(tools.iter().any(|t| t.name == name), "missing tool {name}");
    }

    let session = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    ok(&call(&client, "document_new", json!({"session": session, "width": 300, "height": 200})).await);
    let rect = call(
        &client,
        "draw_rectangle",
        json!({"session": session, "x": 20, "y": 30, "width": 100, "height": 50, "fill": "#3366ff", "stroke": "#000000", "stroke_width": 2}),
    )
    .await;
    let id = ok(&rect)["id"].as_u64().unwrap();

    let moved = call(&client, "move_objects", json!({"session": session, "ids": [id], "dx": 10, "dy": 0})).await;
    ok(&moved);

    let svg = call(&client, "export_document", json!({"session": session, "format": "svg"})).await;
    ok(&svg);
    let svg_text = text_of(&svg).to_ascii_lowercase();
    assert!(svg_text.contains("<svg"), "{svg_text}");
    assert!(svg_text.contains("#3366ff") || svg_text.contains("#36f"), "{svg_text}");

    let pdf = call(&client, "export_document", json!({"session": session, "format": "pdf"})).await;
    assert!(ok(&pdf)["dataBase64"].as_str().unwrap().starts_with("JVBER"), "not a PDF");

    let png = call(&client, "export_document", json!({"session": session, "format": "png", "scale": 2})).await;
    ok(&png);
    assert!(png.content.iter().any(|c| c.as_image().is_some()), "no image content");

    let undone = call(&client, "undo", json!({"session": session})).await;
    ok(&undone);

    ok(&call(&client, "session_close", json!({"session": session})).await);
    let _ = client.cancel().await;
}

#[tokio::test]
async fn svg_markup_can_be_imported_and_exported_again() {
    let base = start(Limits::default()).await;
    let client = client(&base, TOKEN).await;
    let session = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><circle cx="50" cy="50" r="40" fill="#00aa00"/></svg>"##;
    ok(&call(&client, "import_svg", json!({"session": session, "svg": svg})).await);
    let out = call(&client, "export_document", json!({"session": session, "format": "svg"})).await;
    ok(&out);
    assert!(text_of(&out).to_ascii_lowercase().contains("#00aa00") || text_of(&out).to_ascii_lowercase().contains("#0a0"));
    let _ = client.cancel().await;
}

#[tokio::test]
async fn unknown_sessions_and_unknown_arguments_are_refused() {
    let base = start(Limits::default()).await;
    let client = client(&base, TOKEN).await;
    let args = json!({"session": "0000", "width": 10, "height": 10});
    let Value::Object(args) = args else { unreachable!() };
    let err = client.call_tool(CallToolRequestParams::new("document_new").with_arguments(args)).await;
    assert!(err.is_err(), "an unknown session must be refused");

    let session = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    let args = json!({"session": session, "format": "svg", "path": "/tmp/out.svg"});
    let Value::Object(args) = args else { unreachable!() };
    // 引数の誤りは、プロトコルエラーかツールのエラー結果のどちらかで返る（どちらでも書き出しはされない）
    match client.call_tool(CallToolRequestParams::new("export_document").with_arguments(args)).await {
        Err(_) => {}
        Ok(r) => {
            assert_eq!(r.is_error, Some(true), "a path argument must be refused: {r:?}");
            assert!(text_of(&r).contains("path"), "{}", text_of(&r));
        }
    }
    let _ = client.cancel().await;
}

#[tokio::test]
async fn drawing_without_a_document_reports_a_tool_error() {
    let base = start(Limits::default()).await;
    let client = client(&base, TOKEN).await;
    let session = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    let r = call(
        &client,
        "draw_rectangle",
        json!({"session": session, "x": 0, "y": 0, "width": 1, "height": 1}),
    )
    .await;
    assert_eq!(r.is_error, Some(true), "{r:?}");
    let _ = client.cancel().await;
}

#[tokio::test]
async fn a_call_over_the_time_limit_is_cancelled() {
    let limits = Limits {
        call_timeout: Duration::from_millis(200),
        ..Limits::default()
    };
    let base = start(limits).await;
    let client = client(&base, TOKEN).await;
    let session = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    ok(&call(&client, "document_new", json!({"session": session})).await);
    for i in 0..20 {
        ok(&call(
            &client,
            "draw_ellipse",
            json!({"session": session, "x": i * 10, "y": i * 10, "width": 300, "height": 300, "fill": "#ff8800"}),
        )
        .await);
    }
    let r = call(&client, "export_document", json!({"session": session, "format": "png", "scale": 8})).await;
    assert_eq!(r.is_error, Some(true), "{r:?}");
    assert!(text_of(&r).contains("timed out"), "{}", text_of(&r));
    let _ = client.cancel().await;
}

#[tokio::test]
async fn sessions_are_capped() {
    let limits = Limits {
        max_sessions: 1,
        ..Limits::default()
    };
    let base = start(limits).await;
    let client = client(&base, TOKEN).await;
    ok(&call(&client, "session_create", json!({})).await);
    let r = call(&client, "session_create", json!({})).await;
    assert_eq!(r.is_error, Some(true), "{r:?}");
    let _ = client.cancel().await;
}

fn initialize_body() -> &'static str {
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}"#
}

async fn post(base: &str, headers: &[(&str, &str)], body: String) -> reqwest::Response {
    let mut req = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream");
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    req.body(body).send().await.unwrap()
}

#[tokio::test]
async fn requests_without_the_right_token_get_401() {
    let base = start(Limits::default()).await;
    let none = post(&base, &[], initialize_body().into()).await;
    assert_eq!(none.status(), 401);
    let wrong = post(
        &base,
        &[("Authorization", "Bearer not-the-token-0123456789abcdef0123")],
        initialize_body().into(),
    )
    .await;
    assert_eq!(wrong.status(), 401);
    let bearer = format!("Bearer {TOKEN}");
    let right = post(&base, &[("Authorization", &bearer)], initialize_body().into()).await;
    assert!(right.status().is_success(), "{}", right.status());
}

#[tokio::test]
async fn foreign_origins_and_hosts_are_refused() {
    let base = start(Limits::default()).await;
    let bearer = format!("Bearer {TOKEN}");
    let origin = post(
        &base,
        &[("Authorization", &bearer), ("Origin", "http://evil.example")],
        initialize_body().into(),
    )
    .await;
    assert_eq!(origin.status(), 403);
    let host = post(&base, &[("Authorization", &bearer), ("Host", "evil.example")], initialize_body().into()).await;
    assert!(host.status().is_client_error(), "{}", host.status());
}

#[tokio::test]
async fn oversized_bodies_are_refused() {
    let limits = Limits {
        max_body_bytes: 1024,
        ..Limits::default()
    };
    let base = start(limits).await;
    let bearer = format!("Bearer {TOKEN}");
    let body = format!(r#"{{"jsonrpc":"2.0","id":1,"method":"ping","params":{{"pad":"{}"}}}}"#, "x".repeat(4096));
    let r = post(&base, &[("Authorization", &bearer)], body).await;
    assert_eq!(r.status(), 413);
}

#[tokio::test]
async fn health_check_needs_no_token() {
    let base = start(Limits::default()).await;
    let r = reqwest::get(format!("{base}/healthz")).await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.json::<Value>().await.unwrap()["status"], "ok");
}
