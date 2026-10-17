//! MCP のツール定義。ツール1つをエンジンのコマンド列に変換し、作業セッションの実行器に渡す。
//!
//! 引数の構造体のドキュメントコメントは、そのままツールの入力スキーマの説明として AI に
//! 見せるため英語で書く。

use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use http::request::Parts;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::Extension;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ErrorData, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use vectorcraft_engine::cancel::CancelToken;

use crate::auth::Principal;
use crate::engine_host::{HostError, Step};
use crate::session::{SessionError, Sessions};

const INSTRUCTIONS: &str = "ArtServer edits vector documents. Call session_create first and pass the returned `session` \
to every other tool. Create a document with document_new (or import_svg), draw with the draw_* tools, then \
export_document. Coordinates are in points (1/72 inch), origin at the artboard's top-left, y down. Object ids \
returned by draw_* tools are used by set_paint and move_objects.";

/// PNG 書き出しの倍率の上限。
const MAX_PNG_SCALE: f64 = 16.0;

/// MCP サーバー本体。要求ごとに作られる（中身は共有の作業セッション表への参照）。
#[derive(Clone)]
pub struct ArtServer {
    sessions: Arc<Sessions>,
    call_timeout: Duration,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionArgs {
    /// Session handle returned by session_create.
    pub session: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentNewArgs {
    /// Session handle returned by session_create.
    pub session: String,
    /// Artboard width in points (default 612).
    pub width: Option<f64>,
    /// Artboard height in points (default 792).
    pub height: Option<f64>,
    /// Document name.
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportSvgArgs {
    /// Session handle returned by session_create.
    pub session: String,
    /// The SVG markup to open as a new document. Linked files are not read.
    pub svg: String,
    /// Document name (default "imported").
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RectangleArgs {
    /// Session handle returned by session_create.
    pub session: String,
    /// Left edge in points.
    pub x: f64,
    /// Top edge in points.
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Corner radius in points.
    pub radius: Option<f64>,
    /// Fill colour as "#rrggbb", or "none".
    pub fill: Option<String>,
    /// Stroke colour as "#rrggbb", or "none".
    pub stroke: Option<String>,
    /// Stroke weight in points.
    pub stroke_width: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EllipseArgs {
    /// Session handle returned by session_create.
    pub session: String,
    /// Left edge of the bounding box in points.
    pub x: f64,
    /// Top edge of the bounding box in points.
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Fill colour as "#rrggbb", or "none".
    pub fill: Option<String>,
    /// Stroke colour as "#rrggbb", or "none".
    pub stroke: Option<String>,
    /// Stroke weight in points.
    pub stroke_width: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PolygonArgs {
    /// Session handle returned by session_create.
    pub session: String,
    /// Centre x in points.
    pub cx: f64,
    /// Centre y in points.
    pub cy: f64,
    /// Distance from the centre to each corner, in points.
    pub radius: f64,
    /// Number of sides (3-1000, default 6).
    pub sides: Option<u32>,
    /// Fill colour as "#rrggbb", or "none".
    pub fill: Option<String>,
    /// Stroke colour as "#rrggbb", or "none".
    pub stroke: Option<String>,
    /// Stroke weight in points.
    pub stroke_width: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StarArgs {
    /// Session handle returned by session_create.
    pub session: String,
    /// Centre x in points.
    pub cx: f64,
    /// Centre y in points.
    pub cy: f64,
    /// Radius of the outer points, in points.
    pub outer_radius: f64,
    /// Radius of the inner corners, in points.
    pub inner_radius: f64,
    /// Number of points (default 5).
    pub points: Option<u32>,
    /// Fill colour as "#rrggbb", or "none".
    pub fill: Option<String>,
    /// Stroke colour as "#rrggbb", or "none".
    pub stroke: Option<String>,
    /// Stroke weight in points.
    pub stroke_width: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineArgs {
    /// Session handle returned by session_create.
    pub session: String,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    /// Stroke colour as "#rrggbb", or "none".
    pub stroke: Option<String>,
    /// Stroke weight in points.
    pub stroke_width: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetPaintArgs {
    /// Session handle returned by session_create.
    pub session: String,
    /// Ids of the objects to paint.
    pub ids: Vec<u64>,
    /// Fill colour as "#rrggbb", or "none".
    pub fill: Option<String>,
    /// Stroke colour as "#rrggbb", or "none".
    pub stroke: Option<String>,
    /// Stroke weight in points.
    pub stroke_width: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveArgs {
    /// Session handle returned by session_create.
    pub session: String,
    /// Ids of the objects to move.
    pub ids: Vec<u64>,
    /// Horizontal offset in points.
    pub dx: f64,
    /// Vertical offset in points (positive is down).
    pub dy: f64,
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Svg,
    Pdf,
    Png,
}

impl ExportFormat {
    fn as_str(self) -> &'static str {
        match self {
            ExportFormat::Svg => "svg",
            ExportFormat::Pdf => "pdf",
            ExportFormat::Png => "png",
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportArgs {
    /// Session handle returned by session_create.
    pub session: String,
    pub format: ExportFormat,
    /// PNG only: pixels per point (default 1, at most 16).
    pub scale: Option<f64>,
    /// SVG only: convert text to outlines so the file needs no fonts to display.
    pub outline_text: Option<bool>,
}

impl ArtServer {
    pub fn new(sessions: Arc<Sessions>, call_timeout: Duration) -> Self {
        Self {
            sessions,
            call_timeout,
            tool_router: Self::tool_router(),
        }
    }

    /// 作業セッション `handle` で `steps` を実行する。ツール実行の失敗は `Err(CallToolResult)`
    /// （AI に見せるエラー結果）、要求そのものの誤りは `ErrorData` で返す。
    async fn exec(
        &self,
        parts: &Parts,
        handle: &str,
        steps: Vec<Step>,
        ct: &CancellationToken,
    ) -> Result<Result<Vec<Value>, CallToolResult>, ErrorData> {
        let owner = principal(parts)?;
        let host = self.sessions.get(handle, &owner).map_err(session_error)?;
        let token = CancelToken::new();
        let outcome = tokio::select! {
            r = host.run(steps, token.clone()) => r,
            _ = ct.cancelled() => {
                token.cancel();
                Err(HostError::Cancelled)
            }
            _ = tokio::time::sleep(self.call_timeout) => {
                token.cancel();
                return Ok(Err(tool_error(format!("timed out after {} s; the operation was cancelled", self.call_timeout.as_secs_f64()))));
            }
        };
        Ok(outcome.map_err(|e| tool_error(e.to_string())))
    }

    /// 1ステップだけ実行し、その結果をそのまま構造化結果として返す。
    async fn exec_one(&self, parts: &Parts, handle: &str, step: Step, ct: &CancellationToken) -> Result<CallToolResult, ErrorData> {
        Ok(match self.exec(parts, handle, vec![step], ct).await? {
            Ok(mut results) => CallToolResult::structured(results.pop().unwrap_or(Value::Null)),
            Err(e) => e,
        })
    }

    /// 図形を作るステップと、その図形に塗りと線を設定するステップを1回で実行する。
    async fn draw(&self, parts: &Parts, handle: &str, create: Step, paint: PaintArgs, ct: &CancellationToken) -> Result<CallToolResult, ErrorData> {
        let mut steps = vec![create];
        steps.extend(paint.steps(json!(["$0.id"]), true));
        Ok(match self.exec(parts, handle, steps, ct).await? {
            Ok(results) => CallToolResult::structured(json!({"id": results.first().and_then(|r| r.get("id")).cloned().unwrap_or(Value::Null)})),
            Err(e) => e,
        })
    }
}

#[tool_router]
impl ArtServer {
    #[tool(description = "Create a working session and return its handle. Every other tool takes this handle as `session`.")]
    async fn session_create(&self, Extension(parts): Extension<Parts>) -> Result<CallToolResult, ErrorData> {
        let owner = principal(&parts)?;
        Ok(match self.sessions.create(&owner) {
            Ok(handle) => CallToolResult::structured(json!({"session": handle})),
            Err(e @ SessionError::TooMany(_)) => tool_error(e.to_string()),
            Err(e) => return Err(session_error(e)),
        })
    }

    #[tool(description = "Close a working session and discard its documents.")]
    async fn session_close(&self, Parameters(a): Parameters<SessionArgs>, Extension(parts): Extension<Parts>) -> Result<CallToolResult, ErrorData> {
        let owner = principal(&parts)?;
        self.sessions.close(&a.session, &owner).map_err(session_error)?;
        Ok(CallToolResult::structured(json!({"closed": true})))
    }

    #[tool(description = "Create a new document with one artboard and make it the active document of the session.")]
    async fn document_new(
        &self,
        Parameters(a): Parameters<DocumentNewArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        let mut params = json!({});
        insert_opt(&mut params, "width", a.width.map(Value::from));
        insert_opt(&mut params, "height", a.height.map(Value::from));
        insert_opt(&mut params, "name", a.name.map(Value::from));
        self.exec_one(&parts, &a.session, Step::new("file.new", params), &ct).await
    }

    #[tool(description = "Open SVG markup as a new document and make it the active document. Linked files are never read.")]
    async fn import_svg(
        &self,
        Parameters(a): Parameters<ImportSvgArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        let stem = a.name.as_deref().unwrap_or("imported");
        let params = json!({
            "name": format!("{stem}.svg"),
            "dataBase64": base64::engine::general_purpose::STANDARD.encode(a.svg.as_bytes()),
        });
        self.exec_one(&parts, &a.session, Step::new("document.open", params), &ct).await
    }

    #[tool(description = "Draw a rectangle in the active document. Returns the new object's id.")]
    async fn draw_rectangle(
        &self,
        Parameters(a): Parameters<RectangleArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        let mut params = json!({"x": a.x, "y": a.y, "width": a.width, "height": a.height});
        insert_opt(&mut params, "radius", a.radius.map(Value::from));
        let paint = PaintArgs {
            fill: a.fill,
            stroke: a.stroke,
            stroke_width: a.stroke_width,
        };
        self.draw(&parts, &a.session, Step::new("shape.rectangle", params), paint, &ct).await
    }

    #[tool(description = "Draw an ellipse in the active document, fitted to the given box. Returns the new object's id.")]
    async fn draw_ellipse(
        &self,
        Parameters(a): Parameters<EllipseArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        let params = json!({"x": a.x, "y": a.y, "width": a.width, "height": a.height});
        let paint = PaintArgs {
            fill: a.fill,
            stroke: a.stroke,
            stroke_width: a.stroke_width,
        };
        self.draw(&parts, &a.session, Step::new("shape.ellipse", params), paint, &ct).await
    }

    #[tool(description = "Draw a regular polygon in the active document. Returns the new object's id.")]
    async fn draw_polygon(
        &self,
        Parameters(a): Parameters<PolygonArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        let mut params = json!({"cx": a.cx, "cy": a.cy, "radius": a.radius});
        insert_opt(&mut params, "sides", a.sides.map(Value::from));
        let paint = PaintArgs {
            fill: a.fill,
            stroke: a.stroke,
            stroke_width: a.stroke_width,
        };
        self.draw(&parts, &a.session, Step::new("shape.polygon", params), paint, &ct).await
    }

    #[tool(description = "Draw a star in the active document. Returns the new object's id.")]
    async fn draw_star(
        &self,
        Parameters(a): Parameters<StarArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        let mut params = json!({"cx": a.cx, "cy": a.cy, "radius1": a.outer_radius, "radius2": a.inner_radius});
        insert_opt(&mut params, "points", a.points.map(Value::from));
        let paint = PaintArgs {
            fill: a.fill,
            stroke: a.stroke,
            stroke_width: a.stroke_width,
        };
        self.draw(&parts, &a.session, Step::new("shape.star", params), paint, &ct).await
    }

    #[tool(description = "Draw a straight line segment in the active document. Returns the new object's id.")]
    async fn draw_line(
        &self,
        Parameters(a): Parameters<LineArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        let params = json!({"x1": a.x1, "y1": a.y1, "x2": a.x2, "y2": a.y2});
        let paint = PaintArgs {
            fill: None,
            stroke: a.stroke,
            stroke_width: a.stroke_width,
        };
        self.draw(&parts, &a.session, Step::new("shape.line", params), paint, &ct).await
    }

    #[tool(description = "Set the fill, stroke colour and/or stroke weight of objects.")]
    async fn set_paint(
        &self,
        Parameters(a): Parameters<SetPaintArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        let paint = PaintArgs {
            fill: a.fill,
            stroke: a.stroke,
            stroke_width: a.stroke_width,
        };
        let steps = paint.steps(json!(a.ids), false);
        if steps.is_empty() {
            return Err(ErrorData::invalid_params("give at least one of fill, stroke or stroke_width", None));
        }
        Ok(match self.exec(&parts, &a.session, steps, &ct).await? {
            Ok(_) => CallToolResult::structured(json!({"ids": a.ids})),
            Err(e) => e,
        })
    }

    #[tool(description = "Move objects by an offset.")]
    async fn move_objects(
        &self,
        Parameters(a): Parameters<MoveArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        let params = json!({"ids": a.ids, "dx": a.dx, "dy": a.dy});
        self.exec_one(&parts, &a.session, Step::new("object.move", params), &ct).await
    }

    #[tool(description = "Describe the active document: its layer tree with object ids, kinds, bounds and paint, artboards, selection and history.")]
    async fn inspect_document(
        &self,
        Parameters(a): Parameters<SessionArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        self.exec_one(&parts, &a.session, Step::new("document.inspect", json!({})), &ct).await
    }

    #[tool(description = "Export the active document as SVG (returned as text), PNG (returned as an image) or PDF (returned as base64).")]
    async fn export_document(
        &self,
        Parameters(a): Parameters<ExportArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        let mut params = json!({"format": a.format.as_str()});
        if let Some(scale) = a.scale {
            if !matches!(a.format, ExportFormat::Png) {
                return Err(ErrorData::invalid_params("scale applies to PNG only", None));
            }
            if !(scale > 0.0 && scale <= MAX_PNG_SCALE) {
                return Err(ErrorData::invalid_params(
                    format!("scale must be greater than 0 and at most {MAX_PNG_SCALE}"),
                    None,
                ));
            }
            insert_opt(&mut params, "scale", Some(Value::from(scale)));
        }
        if let Some(outline) = a.outline_text {
            if !matches!(a.format, ExportFormat::Svg) {
                return Err(ErrorData::invalid_params("outline_text applies to SVG only", None));
            }
            insert_opt(&mut params, "outlineText", Some(Value::from(outline)));
        }
        let mut results = match self.exec(&parts, &a.session, vec![Step::new("document.export", params)], &ct).await? {
            Ok(results) => results,
            Err(e) => return Ok(e),
        };
        Ok(export_result(a.format, results.pop().unwrap_or(Value::Null)))
    }

    #[tool(description = "Undo the last change in the active document.")]
    async fn undo(
        &self,
        Parameters(a): Parameters<SessionArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        self.exec_one(&parts, &a.session, Step::new("edit.undo", json!({})), &ct).await
    }

    #[tool(description = "Redo the last undone change in the active document.")]
    async fn redo(
        &self,
        Parameters(a): Parameters<SessionArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        self.exec_one(&parts, &a.session, Step::new("edit.redo", json!({})), &ct).await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ArtServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("artserver", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

/// 塗りと線の指定。
struct PaintArgs {
    fill: Option<String>,
    stroke: Option<String>,
    stroke_width: Option<f64>,
}

impl PaintArgs {
    /// `targets`（id の配列、または `["$0.id"]` のような参照）に塗りと線を設定するステップ。
    fn steps(self, targets: Value, referring: bool) -> Vec<Step> {
        let step = |command, params| {
            if referring {
                Step::referring(command, params)
            } else {
                Step::new(command, params)
            }
        };
        let mut steps = Vec::new();
        if let Some(fill) = self.fill {
            steps.push(step("paint.setFill", paint_params(&fill, &targets)));
        }
        if let Some(stroke) = self.stroke {
            steps.push(step("paint.setStroke", paint_params(&stroke, &targets)));
        }
        if let Some(weight) = self.stroke_width {
            steps.push(step("stroke.set", json!({"weight": weight, "ids": targets})));
        }
        steps
    }
}

fn paint_params(colour: &str, targets: &Value) -> Value {
    if colour.eq_ignore_ascii_case("none") {
        json!({"none": true, "ids": targets})
    } else {
        json!({"color": colour, "ids": targets})
    }
}

/// エンジンの書き出し結果を、形式に合った MCP の結果にする。
fn export_result(format: ExportFormat, mut r: Value) -> CallToolResult {
    let data = match r.as_object_mut().and_then(|o| o.remove("dataBase64")) {
        Some(Value::String(data)) => data,
        _ => return tool_error("the export produced no data".to_string()),
    };
    let summary = json!({"format": format.as_str(), "bytes": r.get("bytes").cloned(), "warnings": r.get("warnings").cloned()});
    let mut result = match format {
        ExportFormat::Svg => match base64::engine::general_purpose::STANDARD
            .decode(&data)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
        {
            Some(svg) => CallToolResult::success(vec![ContentBlock::text(svg)]),
            None => return tool_error("the SVG export was not valid UTF-8".to_string()),
        },
        ExportFormat::Png => CallToolResult::success(vec![ContentBlock::image(data, "image/png")]),
        ExportFormat::Pdf => {
            let mut s = summary.clone();
            s["dataBase64"] = Value::String(data);
            return CallToolResult::structured(s);
        }
    };
    result.structured_content = Some(summary);
    result
}

fn insert_opt(params: &mut Value, key: &str, value: Option<Value>) {
    if let (Some(v), Some(o)) = (value, params.as_object_mut()) {
        o.insert(key.to_string(), v);
    }
}

fn principal(parts: &Parts) -> Result<Principal, ErrorData> {
    parts
        .extensions
        .get::<Principal>()
        .cloned()
        .ok_or_else(|| ErrorData::internal_error("request was not authenticated", None))
}

fn session_error(e: SessionError) -> ErrorData {
    match e {
        SessionError::Internal(m) => ErrorData::internal_error(m, None),
        other => ErrorData::invalid_params(other.to_string(), None),
    }
}

fn tool_error(message: String) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
}
