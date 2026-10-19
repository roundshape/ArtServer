//! End-to-end tests. They connect to a server that is actually listening, with rmcp's MCP client and
//! a plain HTTP client.

// The test helpers are outside #[test], so the test allowances apply to the whole file
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
    start_with(limits, None).await
}

async fn start_with(limits: Limits, public_url: Option<&str>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let config = Config {
        bind: addr,
        token: TOKEN.into(),
        allowed_hosts: vec!["127.0.0.1".into(), "localhost".into()],
        allowed_origins: vec![],
        font_dirs: vec![],
        public_url: public_url.map(str::to_string),
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

fn succeeded(result: &CallToolResult) {
    assert_ne!(result.is_error, Some(true), "tool failed: {result:?}");
}

fn ok(result: &CallToolResult) -> &Value {
    succeeded(result);
    result.structured_content.as_ref().unwrap()
}

/// Reads a VectorCraft tool's result (JSON text).
fn json_of(result: &CallToolResult) -> Value {
    succeeded(result);
    serde_json::from_str(&text_of(result)).unwrap_or_else(|e| panic!("not JSON ({e}): {}", text_of(result)))
}

/// The published tools (docs/design-policy.md section 4.7).
const TOOLS: &[&str] = &[
    "session_create",
    "session_close",
    "document_new",
    "import_svg",
    "list_fonts",
    "export_document",
    "upload_create",
    "open_upload",
    "place_upload",
    "draw_shape",
    "draw_path",
    "set_paint",
    "transform",
    "add_text",
    "pathfinder",
    "apply_effect",
    "create_graph",
    "text_wrap",
    "render_preview",
    "doc_inspect",
    "undo",
    "redo",
    "command_list",
    "command_run",
    "command_batch",
];

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
    let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    names.sort_unstable();
    let mut expected = TOOLS.to_vec();
    expected.sort_unstable();
    assert_eq!(names, expected);

    let session = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    ok(&call(&client, "document_new", json!({"session": session, "width": 300, "height": 200})).await);
    let rect = call(
        &client,
        "draw_shape",
        json!({"session": session, "shape": "rectangle", "x": 20, "y": 30, "width": 100, "height": 50, "fill": "#3366ff", "stroke": "#000000", "strokeWidth": 2}),
    )
    .await;
    let id = json_of(&rect)["id"].as_u64().unwrap();

    let moved = call(&client, "transform", json!({"session": session, "ids": [id], "dx": 10})).await;
    succeeded(&moved);

    // With structuredContent on SVG and PNG, Claude Code doesn't show the content
    let svg = call(&client, "export_document", json!({"session": session, "format": "svg"})).await;
    succeeded(&svg);
    assert!(svg.structured_content.is_none(), "{svg:?}");
    let svg_text = text_of(&svg).to_ascii_lowercase();
    assert!(svg_text.contains("<svg"), "{svg_text}");
    assert!(svg_text.contains("#3366ff") || svg_text.contains("#36f"), "{svg_text}");

    // PDF comes back as a URL by default. With delivery: inline it comes back as Base64, as before
    let pdf = call(
        &client,
        "export_document",
        json!({"session": session, "format": "pdf", "delivery": "inline"}),
    )
    .await;
    assert!(ok(&pdf)["dataBase64"].as_str().unwrap().starts_with("JVBER"), "not a PDF");

    let png = call(&client, "export_document", json!({"session": session, "format": "png", "scale": 2})).await;
    succeeded(&png);
    assert!(png.structured_content.is_none(), "{png:?}");
    assert!(png.content.iter().any(|c| c.as_image().is_some()), "no image content");

    let undone = call(&client, "undo", json!({"session": session})).await;
    succeeded(&undone);

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
    succeeded(&out);
    assert!(text_of(&out).to_ascii_lowercase().contains("#00aa00") || text_of(&out).to_ascii_lowercase().contains("#0a0"));
    let _ = client.cancel().await;
}

#[tokio::test]
async fn japanese_text_is_placed_edited_and_exported() {
    let base = start(Limits::default()).await;
    let client = client(&base, TOKEN).await;
    let session = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    ok(&call(&client, "document_new", json!({"session": session, "width": 300, "height": 200})).await);

    let text = call(
        &client,
        "add_text",
        json!({"session": session, "text": "こんにちは", "x": 20, "y": 60, "size": 32, "color": "#cc0000"}),
    )
    .await;
    let id = json_of(&text)["id"].as_u64().unwrap();

    // Replacing the text uses an allowlisted command through command_run
    let changed = call(
        &client,
        "command_run",
        json!({"session": session, "id": "text.setText", "params": {"ids": [id], "text": "こんばんは"}}),
    )
    .await;
    succeeded(&changed);

    let svg = call(&client, "export_document", json!({"session": session, "format": "svg"})).await;
    succeeded(&svg);
    assert!(text_of(&svg).contains("こんばんは"), "{}", text_of(&svg));

    let preview = call(&client, "render_preview", json!({"session": session, "max_side": 300})).await;
    succeeded(&preview);
    assert!(preview.content.iter().any(|c| c.as_image().is_some()), "no preview image");
    let _ = client.cancel().await;
}

#[tokio::test]
async fn fonts_can_be_listed_and_filtered() {
    let base = start(Limits::default()).await;
    let client = client(&base, TOKEN).await;
    let session = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    // The bundled Inter font exists in every environment
    let fonts = call(&client, "list_fonts", json!({"session": session, "query": "inter"})).await;
    let families = ok(&fonts)["families"].as_array().unwrap().clone();
    assert!(families.iter().any(|f| f == "Inter"), "{families:?}");
    assert!(
        families.iter().all(|f| f.as_str().unwrap().to_lowercase().contains("inter")),
        "{families:?}"
    );
    let styles = call(&client, "list_fonts", json!({"session": session, "family": "Inter"})).await;
    assert!(!ok(&styles)["styles"].as_array().unwrap().is_empty());

    // Built-in families that the OS lacks are marked, in the list and for one family alike. Whether Inter is
    // among them depends on whether the machine has it installed
    let bundled = ok(&fonts)["bundled"].as_array().unwrap().clone();
    assert!(bundled.iter().all(|b| families.contains(b)), "{bundled:?} {families:?}");
    let inter_bundled = bundled.iter().any(|b| b == "Inter");
    assert_eq!(ok(&styles)["bundled"], inter_bundled);
    let all = call(&client, "list_fonts", json!({"session": session})).await;
    let all_bundled = ok(&all)["bundled"].as_array().unwrap().clone();
    assert!(all_bundled.len() <= 4 && all_bundled.len() >= bundled.len(), "{all_bundled:?}");
    let _ = client.cancel().await;
}

#[tokio::test]
async fn command_tools_reach_only_allowed_commands() {
    let base = start(Limits::default()).await;
    let client = client(&base, TOKEN).await;
    let session = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    ok(&call(&client, "document_new", json!({"session": session})).await);
    for (id, params) in [
        ("prefs.set", json!({"key": "units", "value": "mm"})),
        ("document.export", json!({"format": "svg"})),
        ("text.addFontFiles", json!({"files": ["/tmp/x.ttf"]})),
    ] {
        let r = call(&client, "command_run", json!({"session": session, "id": id, "params": params})).await;
        assert_eq!(r.is_error, Some(true), "{id}: {r:?}");
        assert!(text_of(&r).contains("not allowed"), "{id}: {}", text_of(&r));
    }
    let list = call(&client, "command_list", json!({"session": session, "filter": "text."})).await;
    let ids: Vec<String> = json_of(&list)
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.iter().any(|id| id == "text.setStyle"), "{ids:?}");
    assert!(!ids.iter().any(|id| id == "text.addFontFiles"), "{ids:?}");

    // VectorCraft's tools also refuse an unknown working session
    let args = json!({"session": "0000", "shape": "rectangle", "x": 0, "y": 0, "width": 1, "height": 1});
    let Value::Object(args) = args else { unreachable!() };
    assert!(
        client
            .call_tool(CallToolRequestParams::new("draw_shape").with_arguments(args))
            .await
            .is_err()
    );
    let _ = client.cancel().await;
}

#[tokio::test]
async fn the_first_batch_of_document_commands_works_through_command_run() {
    // One command of each group opened in the first batch (docs/design-policy.md section 4.7), run as the AI runs them
    let base = start(Limits::default()).await;
    let client = client(&base, TOKEN).await;
    let session = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    ok(&call(&client, "document_new", json!({"session": session, "width": 400, "height": 300})).await);
    let mut ids = Vec::new();
    for (x, y) in [(20, 20), (60, 40), (20, 150), (200, 150), (250, 20), (270, 40)] {
        let shape = json!({"session": session, "shape": "rectangle", "x": x, "y": y, "width": 60, "height": 40, "fill": "#3366ff"});
        ids.push(json_of(&call(&client, "draw_shape", shape).await)["id"].as_u64().unwrap());
    }
    let run = |id: &'static str, params: Value| {
        let args = json!({"session": session, "id": id, "params": params});
        let client = &client;
        async move { call(client, "command_run", args).await }
    };

    succeeded(&run("artboard.new", json!({"x": 420, "y": 0, "width": 200, "height": 200, "name": "second"})).await);
    succeeded(&run("artboard.setProps", json!({"index": 1, "name": "renamed"})).await);

    succeeded(&run("transparency.set", json!({"ids": [ids[0]], "opacity": 50})).await);
    let info = json_of(&run("transparency.info", json!({"ids": [ids[0]]})).await);
    assert_eq!(info["opacity"].as_f64(), Some(50.0), "{info}");
    succeeded(&run("appearance.addFill", json!({"ids": [ids[0]]})).await);

    succeeded(&run("swatch.new", json!({"name": "Brand Blue", "color": "#3366ff"})).await);
    let swatches = text_of(&run("swatch.list", json!({})).await);
    assert!(swatches.contains("Brand Blue"), "{swatches}");

    succeeded(&run("select.set", json!({"ids": [ids[1]]})).await);
    succeeded(&run("symbol.new", json!({"name": "Logo"})).await);
    succeeded(&run("symbol.place", json!({"name": "Logo", "x": 300, "y": 250})).await);

    succeeded(&run("object.blend.make", json!({"ids": [ids[2], ids[3]], "steps": 3})).await);
    succeeded(&run("select.set", json!({"ids": [ids[4], ids[5]]})).await);
    succeeded(&run("object.compoundPath.make", json!({})).await);

    succeeded(&run("guide.list", json!({})).await);
    succeeded(&run("graphicStyle.list", json!({})).await);

    // What the first batch left out stays refused
    for id in [
        "artboard.copy",
        "graphicStyle.libraries",
        "transparency.viewOpacityMask",
        "perspective.presets.export",
    ] {
        let r = run(id, json!({})).await;
        assert!(text_of(&r).contains("not allowed"), "{id}: {}", text_of(&r));
    }
    let svg = text_of(&call(&client, "export_document", json!({"session": session, "format": "svg"})).await);
    assert!(svg.contains("<svg"), "{svg}");
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
    // A wrong argument comes back as either a protocol error or a tool error result (nothing is exported either way)
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
        "draw_shape",
        json!({"session": session, "shape": "rectangle", "x": 0, "y": 0, "width": 1, "height": 1}),
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
        succeeded(
            &call(
                &client,
                "draw_shape",
                json!({"session": session, "shape": "ellipse", "x": i * 10, "y": i * 10, "width": 300, "height": 300, "fill": "#ff8800"}),
            )
            .await,
        );
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

async fn get(url: &str, headers: &[(&str, &str)]) -> reqwest::Response {
    let mut req = reqwest::Client::new().get(url);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    req.send().await.unwrap()
}

#[tokio::test]
async fn large_exports_come_back_as_download_urls() {
    let base = start(Limits::default()).await;
    let client = client(&base, TOKEN).await;
    let session = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    ok(&call(
        &client,
        "document_new",
        json!({"session": session, "width": 300, "height": 200, "name": "ポスター"}),
    )
    .await);
    succeeded(
        &call(
            &client,
            "draw_shape",
            json!({"session": session, "shape": "rectangle", "x": 10, "y": 10, "width": 100, "height": 50, "fill": "#3366ff"}),
        )
        .await,
    );

    // PDF is always a URL. The reply is only a small JSON
    let pdf = call(&client, "export_document", json!({"session": session, "format": "pdf"})).await;
    let download = ok(&pdf)["download"].clone();
    let url = download["url"].as_str().unwrap().to_string();
    assert!(url.starts_with(&base) && url.contains("/files/"), "{url}");
    assert_eq!(download["name"], "ポスター.pdf");
    assert!(ok(&pdf).get("dataBase64").is_none());

    // The ID in the URL is the key, so it can be fetched without a token, as in a browser, any number of times until it expires
    let got = get(&url, &[]).await;
    assert_eq!(got.status(), 200);
    assert_eq!(got.headers()["content-type"], "application/pdf");
    let disposition = got.headers()["content-disposition"].to_str().unwrap().to_string();
    assert!(
        disposition.starts_with("attachment;") && disposition.contains("filename*=UTF-8''"),
        "{disposition}"
    );
    assert_eq!(got.headers()["x-content-type-options"], "nosniff");
    assert_eq!(got.headers()["content-security-policy"], "sandbox");
    assert!(got.bytes().await.unwrap().starts_with(b"%PDF"), "not a PDF");
    assert_eq!(get(&url, &[]).await.status(), 200);

    // A wrong ID, or a Host or Origin that isn't allowed
    assert_eq!(get(&format!("{base}/files/0000"), &[]).await.status(), 404);
    assert_eq!(get(&url, &[("Host", "evil.example")]).await.status(), 403);
    assert_eq!(get(&url, &[("Origin", "http://evil.example")]).await.status(), 403);

    // SVG can also become a URL with delivery: url
    let svg = call(
        &client,
        "export_document",
        json!({"session": session, "format": "svg", "delivery": "url"}),
    )
    .await;
    let svg_url = ok(&svg)["download"]["url"].as_str().unwrap().to_string();
    let body = get(&svg_url, &[]).await.text().await.unwrap();
    assert!(body.contains("<svg"), "{body}");

    // Closing the working session drops what it stored
    ok(&call(&client, "session_close", json!({"session": session})).await);
    assert_eq!(get(&url, &[]).await.status(), 404);
    let _ = client.cancel().await;
}

#[tokio::test]
async fn download_urls_follow_the_public_url() {
    // A reverse proxy passes https://example.com/artserver/ to ArtServer's /
    let base = start_with(Limits::default(), Some("https://example.com/artserver")).await;
    let client = client(&base, TOKEN).await;
    let session = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    ok(&call(&client, "document_new", json!({"session": session})).await);
    let pdf = call(&client, "export_document", json!({"session": session, "format": "pdf"})).await;
    let download = ok(&pdf)["download"].clone();
    let path = download["path"].as_str().unwrap().to_string();
    assert_eq!(download["url"].as_str().unwrap(), format!("https://example.com/artserver{path}"));
    // ArtServer itself serves /files/… (the proxy strips the prefix)
    assert_eq!(get(&format!("{base}{path}"), &[]).await.status(), 200);
    let _ = client.cancel().await;
}

async fn put(url: &str, headers: &[(&str, &str)], body: Vec<u8>) -> reqwest::Response {
    let mut req = reqwest::Client::new().put(url);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    req.body(body).send().await.unwrap()
}

/// Makes an upload with upload_create and returns its id and URL.
async fn new_upload(client: &RunningService<RoleClient, ClientConfig>, session: &str, name: &str) -> (String, String) {
    let upload = ok(&call(client, "upload_create", json!({"session": session, "name": name})).await).clone();
    assert_eq!(upload["method"], "PUT");
    (
        upload["upload"].as_str().unwrap().to_string(),
        upload["url"].as_str().unwrap().to_string(),
    )
}

#[tokio::test]
async fn uploaded_files_can_be_opened_and_placed() {
    let base = start(Limits::default()).await;
    let client = client(&base, TOKEN).await;
    let session = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    ok(&call(&client, "document_new", json!({"session": session, "width": 300, "height": 200})).await);
    succeeded(
        &call(
            &client,
            "draw_shape",
            json!({"session": session, "shape": "rectangle", "x": 10, "y": 10, "width": 100, "height": 50, "fill": "#3366ff"}),
        )
        .await,
    );
    let pdf_url = ok(&call(&client, "export_document", json!({"session": session, "format": "pdf"})).await)["download"]["url"]
        .as_str()
        .unwrap()
        .to_string();
    let pdf = get(&pdf_url, &[]).await.bytes().await.unwrap().to_vec();
    let png = call(&client, "export_document", json!({"session": session, "format": "png"})).await;
    let png = png.content.iter().find_map(|c| c.as_image()).unwrap().data.clone();
    let png = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, png).unwrap();

    let (upload, url) = new_upload(&client, &session, "saved.pdf").await;
    assert!(url.starts_with(&format!("{base}/uploads/")), "{url}");
    // Opening before the file is sent says so
    let early = call(&client, "open_upload", json!({"session": session, "upload": upload})).await;
    assert_eq!(early.is_error, Some(true), "{early:?}");
    assert!(text_of(&early).contains("not been sent"), "{}", text_of(&early));

    let sent = put(&url, &[], pdf.clone()).await;
    assert_eq!(sent.status(), 200);
    let sent: Value = sent.json().await.unwrap();
    assert_eq!((sent["format"].as_str(), sent["bytes"].as_u64()), (Some("pdf"), Some(pdf.len() as u64)));
    assert_eq!(put(&url, &[], pdf).await.status(), 409, "an upload takes one file");

    // This server's PDF carries the document's editing data, so it opens as the document it was
    let opened = call(&client, "open_upload", json!({"session": session, "upload": upload})).await;
    let opened = ok(&opened);
    assert_eq!(opened["restored"], true, "{opened}");
    assert_eq!(opened["warnings"], json!([]), "{opened}");
    let svg = call(&client, "export_document", json!({"session": session, "format": "svg"})).await;
    let svg = text_of(&svg).to_ascii_lowercase();
    assert!(svg.contains("#3366ff") || svg.contains("#36f"), "{svg}");

    // A PNG goes into the active document, embedded
    let (logo, logo_url) = new_upload(&client, &session, "logo.png").await;
    assert_eq!(put(&logo_url, &[], png).await.status(), 200);
    let placed = call(&client, "place_upload", json!({"session": session, "upload": logo, "at": [150, 100]})).await;
    let placed = ok(&placed);
    assert!(!placed["ids"].as_array().unwrap().is_empty(), "{placed}");
    assert_eq!(placed["linked"], false, "{placed}");

    // Another session can't use the upload, and closing the session drops it
    let other = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    ok(&call(&client, "document_new", json!({"session": other})).await);
    let stolen = call(&client, "place_upload", json!({"session": other, "upload": logo})).await;
    assert_eq!(stolen.is_error, Some(true), "{stolen:?}");
    ok(&call(&client, "session_close", json!({"session": session})).await);
    assert_eq!(get(&logo_url, &[]).await.status(), 404);
    let _ = client.cancel().await;
}

#[tokio::test]
async fn ai_and_vectorcraft_exports_open_again_as_they_were() {
    let base = start(Limits::default()).await;
    let client = client(&base, TOKEN).await;
    let session = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    ok(&call(
        &client,
        "document_new",
        json!({"session": session, "width": 300, "height": 200, "name": "poster"}),
    )
    .await);
    succeeded(
        &call(
            &client,
            "draw_shape",
            json!({"session": session, "shape": "rectangle", "x": 10, "y": 10, "width": 100, "height": 50, "fill": "#3366ff"}),
        )
        .await,
    );
    succeeded(
        &call(
            &client,
            "command_run",
            json!({"session": session, "id": "layer.new", "params": {"name": "Layer 2"}}),
        )
        .await,
    );
    let layers = |inspected: &Value| -> Vec<String> {
        inspected["layers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["name"].as_str().unwrap().to_string())
            .collect()
    };
    let original = layers(&json_of(&call(&client, "doc_inspect", json!({"session": session})).await));
    assert_eq!(original.len(), 2, "{original:?}");

    // The .ai is PDF-compatible (a PDF inside), and the .vectorcraft is compressed
    for (format, magic) in [("ai", &b"%PDF-"[..]), ("vectorcraft", &b"\x1f\x8b"[..])] {
        let exported = call(&client, "export_document", json!({"session": session, "format": format})).await;
        let download = ok(&exported)["download"].clone();
        let name = format!("poster.{format}");
        assert_eq!(download["name"], name.as_str(), "binary formats always come back as a URL");
        let got = get(download["url"].as_str().unwrap(), &[]).await;
        assert_eq!(got.headers()["content-type"], "application/octet-stream");
        let bytes = got.bytes().await.unwrap().to_vec();
        assert!(bytes.starts_with(magic), "{format}: {:?}", &bytes[..bytes.len().min(8)]);
        // No PDF layers in the .ai: Illustrator would make each one a clip group that is hard to select
        if format == "ai" {
            assert!(!bytes.windows(13).any(|w| w == b"/OCProperties"), "the .ai has PDF layers");
        }

        let (upload, url) = new_upload(&client, &session, &name).await;
        assert_eq!(put(&url, &[], bytes).await.status(), 200, "{format}");
        let opened = call(&client, "open_upload", json!({"session": session, "upload": upload})).await;
        let opened = ok(&opened);
        assert_eq!(opened["format"], format, "{opened}");
        assert_eq!(opened["warnings"], json!([]), "{opened}");
        if format == "ai" {
            assert_eq!(opened["restored"], true, "the .ai carries the editing data: {opened}");
        }
        // The document's layers come back from the editing data, PDF layers or not
        let reopened = layers(&json_of(&call(&client, "doc_inspect", json!({"session": session})).await));
        assert_eq!(reopened, original, "{format}");
        let svg = call(&client, "export_document", json!({"session": session, "format": "svg"})).await;
        let svg = text_of(&svg).to_ascii_lowercase();
        assert!(svg.contains("#3366ff") || svg.contains("#36f"), "{format}: {svg}");
    }
    let _ = client.cancel().await;
}

#[tokio::test]
async fn uploads_are_checked() {
    let limits = Limits {
        max_upload_bytes: 4096,
        ..Limits::default()
    };
    let base = start(limits).await;
    let client = client(&base, TOKEN).await;
    let session = ok(&call(&client, "session_create", json!({})).await)["session"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, url) = new_upload(&client, &session, "a.svg").await;
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#.to_vec();

    // Only the accepted formats, within the size limit
    assert_eq!(put(&url, &[], b"GIF89a\x01\x00\x01\x00".to_vec()).await.status(), 415);
    let mut big = b"%PDF-1.7\n".to_vec();
    big.resize(8192, b' ');
    assert_eq!(put(&url, &[], big).await.status(), 413);

    // A wrong ID, or a Host or Origin that isn't allowed
    assert_eq!(put(&format!("{base}/uploads/0000"), &[], svg.clone()).await.status(), 404);
    assert_eq!(put(&url, &[("Host", "evil.example")], svg.clone()).await.status(), 403);
    assert_eq!(put(&url, &[("Origin", "http://evil.example")], svg.clone()).await.status(), 403);

    // The page for browsers runs only its own script, which sends the file from this server's own origin
    let page = get(&url, &[]).await;
    assert_eq!(page.status(), 200);
    assert!(page.headers()["content-type"].to_str().unwrap().starts_with("text/html"));
    let policy = page.headers()["content-security-policy"].to_str().unwrap().to_string();
    assert!(policy.contains("script-src 'self'") && policy.contains("connect-src 'self'"), "{policy}");
    assert!(page.text().await.unwrap().contains(r#"<script src="upload.js">"#));
    let script = get(&format!("{base}/uploads/upload.js"), &[]).await;
    assert_eq!(script.status(), 200);
    assert!(script.text().await.unwrap().contains(r#"method: "PUT""#));
    assert_eq!(put(&url, &[("Origin", base.as_str())], svg).await.status(), 200);
    assert!(get(&url, &[]).await.text().await.unwrap().contains("has been sent"));

    // import_svg also takes only SVG
    let r = call(&client, "import_svg", json!({"session": session, "svg": "%PDF-1.4\n%%EOF"})).await;
    assert_eq!(r.is_error, Some(true), "{r:?}");
    let _ = client.cancel().await;
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
