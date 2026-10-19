//! MCP tool definitions.
//!
//! ArtServer's tools (working sessions, creating, importing and exporting documents, uploads, the font list)
//! are defined here and turned into sequences of engine commands. Other editing is done by VectorCraft's tools,
//! published as they are and run on GuardedBackend in the working session's executor
//! (docs/design-policy.md section 4.7).
//!
//! The doc comments of the argument structs are shown to the AI as is, as the descriptions in the tools'
//! input schemas.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::body::Bytes;
use base64::Engine as _;
use http::request::Parts;
use rmcp::handler::server::router::tool::{ToolRoute, ToolRouter};
use rmcp::handler::server::tool::{Extension, ToolCallContext};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResponse, CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig, Tool};
use rmcp::{ErrorData, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use vectorcraft_engine::cancel::CancelToken;
use vectorcraft_mcp::ToolResult;

use crate::auth::Principal;
use crate::engine_host::{HostError, Step};
use crate::files::{FileStore, StoredFile, base_url, file_name};
use crate::session::{SessionError, Sessions};
use crate::uploads::{self, Format};

const INSTRUCTIONS: &str = "ArtServer edits vector documents. Call session_create first and pass the returned `session` \
to every other tool. Create a document with document_new (or import_svg), draw and edit with the dedicated tools \
(draw_shape, draw_path, add_text, transform, set_paint, pathfinder, apply_effect, create_graph, text_wrap), check the \
result with doc_inspect or render_preview, then export_document. For anything else, find a command with command_list \
and run it with command_run (or several with command_batch); only the commands command_list shows are available. \
Pass object ids explicitly as `ids` rather than relying on the selection. Coordinates are in points (1/72 inch), \
origin at the artboard's top-left, y down. list_fonts tells which font families this server has. Large exports \
come back as a download URL valid for a few minutes: open it in a browser or fetch it with an HTTP GET (for example \
`curl -o out.pdf <url>`); no token is needed, and anyone with the URL can download the file until it expires. To use a \
file the user has (PDF, .ai, SVG, PNG, JPEG or .vectorcraft), call upload_create, send the file to the returned URL with an HTTP PUT \
(for example `curl -T logo.png '<url>'`) or ask the user to open the URL in a browser, then open it as a document with \
open_upload or put it into the active document with place_upload.";

/// The VectorCraft tools ArtServer publishes (docs/design-policy.md section 4.7).
const VECTORCRAFT_TOOLS: &[&str] = &[
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

/// The description of the working session handle argument added to VectorCraft's tools.
const SESSION_DESCRIPTION: &str = "Session handle returned by session_create.";

/// Maximum scale of a PNG export.
const MAX_PNG_SCALE: f64 = 16.0;

/// With delivery auto, the largest result put in the reply (docs/design-policy.md section 4.6). It goes into
/// the AI's conversation as is, so text is kept well below what reaches Claude Code (about 25,000 tokens).
/// PDF, .ai and .vectorcraft are always a URL, because the AI can't read them.
const INLINE_SVG_BYTES: usize = 32 * 1024;
const INLINE_PNG_BYTES: usize = 256 * 1024;

/// The future that the entry point of VectorCraft's tools returns.
type ToolFuture<'a> = Pin<Box<dyn Future<Output = Result<CallToolResponse, ErrorData>> + Send + 'a>>;

/// The MCP server. Made per request (it holds references to the shared table of working sessions).
#[derive(Clone)]
pub struct ArtServer {
    sessions: Arc<Sessions>,
    files: Arc<FileStore>,
    /// ArtServer's URL as seen from outside (--public-url). Used for export and upload URLs
    public_url: Option<Arc<str>>,
    call_timeout: Duration,
    /// Size limit of one uploaded file, told to the AI with the upload URL
    max_upload_bytes: usize,
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
pub struct UploadCreateArgs {
    /// Session handle returned by session_create.
    pub session: String,
    /// The file's name, such as logo.ai. It becomes the document's title, and a .ai name keeps an Illustrator
    /// file's meaning. The format itself is told from the contents.
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenUploadArgs {
    /// Session handle returned by session_create.
    pub session: String,
    /// The `upload` returned by upload_create, once the file has been sent.
    pub upload: String,
    /// PDF and .ai only: the pages to open, such as "1-3, 5" (default all; one artboard each).
    pub pages: Option<String>,
    /// PDF and .ai only: text comes in as editable text in the file's fonts (default), or as outlines.
    pub text_as: Option<TextAs>,
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TextAs {
    Text,
    Outlines,
}

impl TextAs {
    fn as_str(self) -> &'static str {
        match self {
            TextAs::Text => "text",
            TextAs::Outlines => "outlines",
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaceUploadArgs {
    /// Session handle returned by session_create.
    pub session: String,
    /// The `upload` returned by upload_create, once the file has been sent.
    pub upload: String,
    /// Centre of the placed art as [x, y] in points (default: the centre of the first artboard).
    pub at: Option<[f64; 2]>,
    /// Fit the art inside [x, y, width, height] in points, keeping its aspect ratio (wins over `at`).
    pub rect: Option<[f64; 4]>,
    /// PDF and .ai only: the page to place (default 1).
    pub page: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListFontsArgs {
    /// Session handle returned by session_create.
    pub session: String,
    /// Only families whose name contains this text (case-insensitive).
    pub query: Option<String>,
    /// A family name: list its styles instead of the families.
    pub family: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Svg,
    Pdf,
    Png,
    Ai,
    Vectorcraft,
}

impl ExportFormat {
    fn as_str(self) -> &'static str {
        match self {
            ExportFormat::Svg => "svg",
            ExportFormat::Pdf => "pdf",
            ExportFormat::Png => "png",
            ExportFormat::Ai => "ai",
            ExportFormat::Vectorcraft => "vectorcraft",
        }
    }

    fn mime(self) -> &'static str {
        match self {
            ExportFormat::Svg => "image/svg+xml",
            ExportFormat::Pdf => "application/pdf",
            ExportFormat::Png => "image/png",
            // No standard type: a generic one keeps browsers from renaming the downloaded file
            ExportFormat::Ai | ExportFormat::Vectorcraft => "application/octet-stream",
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportArgs {
    /// Session handle returned by session_create.
    pub session: String,
    /// svg, pdf, png, ai (an Illustrator file saved with PDF compatibility: Illustrator opens it as PDF artwork, while
    /// this server and VectorCraft open it fully editable) or vectorcraft (VectorCraft's native file).
    pub format: ExportFormat,
    /// PNG only: pixels per point (default 1, at most 16).
    pub scale: Option<f64>,
    /// SVG only: convert text to outlines so the file needs no fonts to display.
    pub outline_text: Option<bool>,
    /// How to return the file: auto (default: SVG up to 32 KB and PNG up to 256 KB inline; PDF, ai, vectorcraft and
    /// anything larger as a download URL), inline, or url.
    pub delivery: Option<Delivery>,
}

/// How an export is returned.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Delivery {
    #[default]
    Auto,
    Inline,
    Url,
}

impl Delivery {
    /// Should a `format` result of `bytes` bytes go in the reply?
    fn inline(self, format: ExportFormat, bytes: usize) -> bool {
        match (self, format) {
            (Delivery::Inline, _) => true,
            (Delivery::Url, _) | (Delivery::Auto, ExportFormat::Pdf | ExportFormat::Ai | ExportFormat::Vectorcraft) => false,
            (Delivery::Auto, ExportFormat::Svg) => bytes <= INLINE_SVG_BYTES,
            (Delivery::Auto, ExportFormat::Png) => bytes <= INLINE_PNG_BYTES,
        }
    }
}

impl ArtServer {
    pub fn new(
        sessions: Arc<Sessions>,
        files: Arc<FileStore>,
        public_url: Option<Arc<str>>,
        call_timeout: Duration,
        max_upload_bytes: usize,
    ) -> Self {
        let mut tool_router = Self::tool_router();
        for tool in vectorcraft_tools() {
            tool_router.add_route(ToolRoute::new_dyn(tool.clone(), call_vectorcraft_tool));
        }
        Self {
            sessions,
            files,
            public_url,
            call_timeout,
            max_upload_bytes,
            tool_router,
        }
    }

    /// Checks that `handle` is a working session of the request's principal (which also counts as using it).
    fn check_session(&self, parts: &Parts, handle: &str) -> Result<(), ErrorData> {
        let owner = principal(parts)?;
        self.sessions.get(handle, &owner).map_err(session_error).map(|_| ())
    }

    /// The file sent for `upload` of working session `session`, as the engine takes it: a name with the
    /// extension of its format, and its contents in Base64.
    fn uploaded(&self, session: &str, upload: &str) -> Result<(String, String), CallToolResult> {
        let file = self.files.upload(upload, session).map_err(|e| tool_error(e.to_string()))?;
        let name = uploads::engine_name(&file.name, file.mime).ok_or_else(|| tool_error("the uploaded file has no known format".to_string()))?;
        Ok((name, base64::engine::general_purpose::STANDARD.encode(&file.data)))
    }

    /// Runs `steps` in working session `handle`. A failed tool run comes back as `Err(CallToolResult)`
    /// (an error result shown to the AI); a fault in the request itself as `ErrorData`.
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
        Ok(self.within_limits(host.run(steps, token.clone()), &token, ct).await)
    }

    /// Runs one step and returns its result as is, as the structured result.
    async fn exec_one(&self, parts: &Parts, handle: &str, step: Step, ct: &CancellationToken) -> Result<CallToolResult, ErrorData> {
        Ok(match self.exec(parts, handle, vec![step], ct).await? {
            Ok(mut results) => CallToolResult::structured(results.pop().unwrap_or(Value::Null)),
            Err(e) => e,
        })
    }

    /// Runs VectorCraft's tool `name` in working session `handle`.
    async fn run_vectorcraft_tool(
        &self,
        parts: &Parts,
        handle: &str,
        name: String,
        args: Value,
        ct: &CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        let owner = principal(parts)?;
        let host = self.sessions.get(handle, &owner).map_err(session_error)?;
        let token = CancelToken::new();
        Ok(match self.within_limits(host.run_tool(name, args, token.clone()), &token, ct).await {
            Ok(result) => tool_result(result),
            Err(e) => e,
        })
    }

    /// Waits for `run`, watching the time limit and the client's cancellation. When either cuts it short,
    /// asks the engine through `token` to stop.
    async fn within_limits<T>(
        &self,
        run: impl Future<Output = Result<T, HostError>>,
        token: &CancelToken,
        ct: &CancellationToken,
    ) -> Result<T, CallToolResult> {
        tokio::select! {
            r = run => r.map_err(|e| tool_error(e.to_string())),
            _ = ct.cancelled() => {
                token.cancel();
                Err(tool_error(HostError::Cancelled.to_string()))
            }
            _ = tokio::time::sleep(self.call_timeout) => {
                token.cancel();
                Err(tool_error(format!("timed out after {} s; the operation was cancelled", self.call_timeout.as_secs_f64())))
            }
        }
    }

    /// The part of an export or upload URL before /files/ or /uploads/: --public-url if given, else the URL of the
    /// ArtServer this request reached.
    fn public_base(&self, parts: &Parts) -> String {
        match &self.public_url {
            Some(url) => url.to_string(),
            None => base_url(&parts.headers),
        }
    }

    /// Puts an export in the file store and returns the URL to fetch it from (docs/design-policy.md section 4.6).
    fn deposit(&self, parts: &Parts, session: &str, format: ExportFormat, title: Option<&str>, exported: Value) -> Result<CallToolResult, ErrorData> {
        let data = match exported
            .get("dataBase64")
            .and_then(Value::as_str)
            .and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok())
        {
            Some(data) => data,
            None => return Ok(tool_error("the export produced no data".to_string())),
        };
        let name = file_name(title, format.as_str());
        let size = data.len();
        let file = StoredFile {
            name: name.clone(),
            mime: format.mime(),
            data: Bytes::from(data),
        };
        let deposit = match self.files.put(session, file) {
            Ok(deposit) => deposit,
            Err(e) => return Ok(tool_error(e.to_string())),
        };
        let path = format!("/files/{}", deposit.id);
        Ok(CallToolResult::structured(json!({
            "format": format.as_str(),
            "bytes": size,
            "warnings": exported.get("warnings").cloned().unwrap_or(json!([])),
            "download": {
                "url": format!("{}{path}", self.public_base(parts)),
                "path": path,
                "name": name,
                "expiresInSeconds": deposit.expires_in.as_secs(),
                "note": "No token is needed: anyone with the URL can download the file until it expires.",
            },
        })))
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
        self.files.remove_session(&a.session);
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
        // The engine tells formats from the contents, so anything but SVG would reach another reader
        if Format::sniff(a.svg.as_bytes()) != Some(Format::Svg) {
            return Ok(tool_error("`svg` must be SVG markup".to_string()));
        }
        let stem = a.name.as_deref().unwrap_or("imported");
        let params = json!({
            "name": format!("{stem}.svg"),
            "dataBase64": base64::engine::general_purpose::STANDARD.encode(a.svg.as_bytes()),
        });
        self.exec_one(&parts, &a.session, Step::new("document.open", params), &ct).await
    }

    #[tool(
        description = "Get a URL to send a file to this server: PDF (including Illustrator .ai files saved with PDF \
compatibility), SVG, PNG, JPEG or VectorCraft (.vectorcraft). Send the file's raw bytes to it with an HTTP PUT (for example `curl -T logo.png '<url>'`), \
or give the URL to the user to open in a browser and choose the file there. Then open it as a new document with open_upload, \
or put it into the active document with place_upload, passing the returned `upload`. The URL takes one file and expires in \
a few minutes; no token is needed."
    )]
    async fn upload_create(
        &self,
        Parameters(a): Parameters<UploadCreateArgs>,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        self.check_session(&parts, &a.session)?;
        let deposit = match self.files.reserve_upload(&a.session, a.name.as_deref().unwrap_or("upload")) {
            Ok(deposit) => deposit,
            Err(e) => return Ok(tool_error(e.to_string())),
        };
        let path = format!("/uploads/{}", deposit.id);
        Ok(CallToolResult::structured(json!({
            "upload": deposit.id,
            "url": format!("{}{path}", self.public_base(&parts)),
            "path": path,
            "method": "PUT",
            "maxBytes": self.max_upload_bytes,
            "expiresInSeconds": deposit.expires_in.as_secs(),
            "accepts": uploads::ACCEPTED,
            "note": "Send the file's raw bytes with an HTTP PUT, for example: curl -T <file> '<url>'. Or open the URL in a \
        browser and choose the file there. No token is needed; the URL takes one file.",
        })))
    }

    #[tool(
        description = "Open a file sent through upload_create as a new document and make it the active document. A PDF or \
.ai that carries its editing data (as this server's PDF exports do) opens fully editable; other files open as their artwork."
    )]
    async fn open_upload(
        &self,
        Parameters(a): Parameters<OpenUploadArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        self.check_session(&parts, &a.session)?;
        let (name, data) = match self.uploaded(&a.session, &a.upload) {
            Ok(file) => file,
            Err(e) => return Ok(e),
        };
        let mut params = json!({"name": name, "dataBase64": data});
        insert_opt(&mut params, "pages", a.pages.map(Value::from));
        insert_opt(&mut params, "textAs", a.text_as.map(|t| Value::from(t.as_str())));
        self.exec_one(&parts, &a.session, Step::new("document.open", params), &ct).await
    }

    #[tool(
        description = "Place a file sent through upload_create into the active document: an image as it is, or a PDF, .ai, \
SVG or .vectorcraft file as one group. The file is embedded, never linked. Returns the ids of what was placed and its size in points."
    )]
    async fn place_upload(
        &self,
        Parameters(a): Parameters<PlaceUploadArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        self.check_session(&parts, &a.session)?;
        let (name, data) = match self.uploaded(&a.session, &a.upload) {
            Ok(file) => file,
            Err(e) => return Ok(e),
        };
        let mut params = json!({"name": name, "dataBase64": data, "link": false});
        insert_opt(&mut params, "at", a.at.map(|p| json!(p)));
        insert_opt(&mut params, "rect", a.rect.map(|r| json!(r)));
        insert_opt(&mut params, "page", a.page.map(Value::from));
        self.exec_one(&parts, &a.session, Step::new("file.place", params), &ct).await
    }

    #[tool(
        description = "List the font families this server can use (bundled, installed on the server and from its configured font \
folders), or the styles of one family. Use these names for add_text and text styles; a family the server lacks is drawn with a fallback font."
    )]
    async fn list_fonts(
        &self,
        Parameters(a): Parameters<ListFontsArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        let mut params = json!({});
        insert_opt(&mut params, "family", a.family.map(Value::from));
        let mut results = match self.exec(&parts, &a.session, vec![Step::new("text.fontList", params)], &ct).await? {
            Ok(results) => results,
            Err(e) => return Ok(e),
        };
        let mut out = results.pop().unwrap_or(Value::Null);
        if let Some(o) = out.as_object_mut()
            && let Some(Value::Array(families)) = o.get_mut("families")
        {
            if let Some(q) = a.query.map(|q| q.to_lowercase()) {
                families.retain(|f| f.as_str().is_some_and(|n| n.to_lowercase().contains(&q)));
            }
            let count = families.len();
            o.insert("count".to_string(), Value::from(count));
        }
        Ok(CallToolResult::structured(out))
    }

    #[tool(
        description = "Export the active document as SVG, PNG, PDF, Illustrator .ai (saved with PDF compatibility) or \
VectorCraft (.vectorcraft). Small SVG (up to 32 KB) comes back as text and small PNG (up to 256 KB) as an image; PDF, .ai, \
.vectorcraft and larger files come back as a download URL valid for a few minutes, which a browser or an HTTP GET can fetch \
without a token. `delivery` overrides the choice (inline returns the binary formats as base64)."
    )]
    async fn export_document(
        &self,
        Parameters(a): Parameters<ExportArgs>,
        Extension(parts): Extension<Parts>,
        ct: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        let mut params = json!({"format": a.format.as_str()});
        // Compressed native files are smaller, and VectorCraft opens them as it opens plain ones
        if matches!(a.format, ExportFormat::Vectorcraft) {
            insert_opt(&mut params, "compress", Some(Value::Bool(true)));
        }
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
        // The document's name (title) becomes the file name when the export is passed by URL
        let steps = vec![Step::new("document.inspect", json!({"depth": 0})), Step::new("document.export", params)];
        let mut results = match self.exec(&parts, &a.session, steps, &ct).await? {
            Ok(results) => results,
            Err(e) => return Ok(e),
        };
        let exported = results.pop().unwrap_or(Value::Null);
        let title = results.pop().and_then(|doc| doc.get("title").and_then(Value::as_str).map(str::to_string));
        let bytes = exported
            .get("bytes")
            .and_then(Value::as_u64)
            .and_then(|b| usize::try_from(b).ok())
            .unwrap_or(usize::MAX);
        if a.delivery.unwrap_or_default().inline(a.format, bytes) {
            return Ok(export_result(a.format, exported));
        }
        self.deposit(&parts, &a.session, a.format, title.as_deref(), exported)
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

/// The definitions of the VectorCraft tools ArtServer publishes. Each gets the working session handle
/// `session` as a required argument.
fn vectorcraft_tools() -> &'static [Tool] {
    static TOOLS: OnceLock<Vec<Tool>> = OnceLock::new();
    TOOLS.get_or_init(|| {
        vectorcraft_mcp::tool_definitions()
            .into_iter()
            .filter(|d| d.get("name").and_then(Value::as_str).is_some_and(|n| VECTORCRAFT_TOOLS.contains(&n)))
            .filter_map(|mut d| {
                add_session_argument(&mut d);
                serde_json::from_value::<Tool>(d)
                    .inspect_err(|e| tracing::error!("cannot publish a VectorCraft tool: {e}"))
                    .ok()
            })
            .collect()
    })
}

fn add_session_argument(def: &mut Value) {
    let Some(schema) = def.get_mut("inputSchema").and_then(Value::as_object_mut) else {
        return;
    };
    if let Some(props) = schema.entry("properties").or_insert_with(|| json!({})).as_object_mut() {
        props.insert("session".to_string(), json!({"type": "string", "description": SESSION_DESCRIPTION}));
    }
    if let Some(required) = schema.entry("required").or_insert_with(|| json!([])).as_array_mut() {
        required.insert(0, json!("session"));
    }
}

/// The entry point of VectorCraft's tools. Takes out `session` and passes the remaining arguments to the tool as is.
fn call_vectorcraft_tool(ctx: ToolCallContext<'_, ArtServer>) -> ToolFuture<'_> {
    Box::pin(async move {
        let mut args = ctx.arguments.unwrap_or_default();
        let session = match args.remove("session") {
            Some(Value::String(s)) => s,
            _ => return Err(ErrorData::invalid_params("missing string argument `session`", None)),
        };
        let parts = ctx
            .request_context
            .extensions
            .get::<Parts>()
            .ok_or_else(|| ErrorData::internal_error("request was not authenticated", None))?;
        let result = ctx
            .service
            .run_vectorcraft_tool(parts, &session, ctx.name.into_owned(), Value::Object(args), &ctx.request_context.ct)
            .await?;
        Ok(result.into())
    })
}

/// Turns a VectorCraft tool result into an MCP result, without structuredContent (with it, Claude Code and
/// other clients don't show the content; the same idea as export_result).
fn tool_result(r: ToolResult) -> CallToolResult {
    let content: Result<Vec<ContentBlock>, _> = r.content.into_iter().map(serde_json::from_value).collect();
    match content {
        Ok(content) if r.is_error => CallToolResult::error(content),
        Ok(content) => CallToolResult::success(content),
        Err(e) => tool_error(format!("the tool returned content this server cannot pass on: {e}")),
    }
}

/// Turns an engine export into the MCP result that suits its format.
///
/// SVG and PNG return only the content, without `structuredContent`: Claude Code and other clients show
/// `structuredContent` when it is there and hide the content. Warnings, when there are any, are added as a
/// second text.
fn export_result(format: ExportFormat, mut r: Value) -> CallToolResult {
    let data = match r.as_object_mut().and_then(|o| o.remove("dataBase64")) {
        Some(Value::String(data)) => data,
        _ => return tool_error("the export produced no data".to_string()),
    };
    let warnings = r.get("warnings").cloned();
    let body = match format {
        ExportFormat::Svg => match base64::engine::general_purpose::STANDARD
            .decode(&data)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
        {
            Some(svg) => ContentBlock::text(svg),
            None => return tool_error("the SVG export was not valid UTF-8".to_string()),
        },
        ExportFormat::Png => ContentBlock::image(data, "image/png"),
        ExportFormat::Pdf | ExportFormat::Ai | ExportFormat::Vectorcraft => {
            return CallToolResult::structured(json!({
                "format": format.as_str(),
                "bytes": r.get("bytes").cloned(),
                "warnings": warnings,
                "dataBase64": data,
            }));
        }
    };
    let mut content = vec![body];
    if let Some(w) = warnings.filter(|w| w.as_array().is_some_and(|a| !a.is_empty())) {
        content.push(ContentBlock::text(json!({"warnings": w}).to_string()));
    }
    CallToolResult::success(content)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_output(data: &[u8], warnings: Value) -> Value {
        json!({
            "dataBase64": base64::engine::general_purpose::STANDARD.encode(data),
            "bytes": data.len(),
            "warnings": warnings,
        })
    }

    #[test]
    fn svg_is_returned_as_text_without_structured_content() {
        let r = export_result(ExportFormat::Svg, engine_output(b"<svg/>", json!([])));
        assert_eq!(r.is_error, Some(false));
        assert!(r.structured_content.is_none());
        assert_eq!(r.content.len(), 1);
        assert_eq!(r.content[0].as_text().unwrap().text, "<svg/>");
    }

    #[test]
    fn png_is_returned_as_an_image_without_structured_content() {
        let r = export_result(ExportFormat::Png, engine_output(b"\x89PNG", json!([])));
        assert!(r.structured_content.is_none());
        assert_eq!(r.content.len(), 1);
        assert!(r.content[0].as_image().is_some());
    }

    #[test]
    fn warnings_follow_the_body_as_a_second_text() {
        let r = export_result(ExportFormat::Svg, engine_output(b"<svg/>", json!(["missing font: Foo"])));
        assert!(r.structured_content.is_none());
        assert_eq!(r.content.len(), 2);
        let w: Value = serde_json::from_str(&r.content[1].as_text().unwrap().text).unwrap();
        assert_eq!(w, json!({"warnings": ["missing font: Foo"]}));
    }

    #[test]
    fn every_chosen_vectorcraft_tool_is_published_with_a_session_argument() {
        let tools = vectorcraft_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(names.len(), VECTORCRAFT_TOOLS.len(), "{names:?}");
        for name in VECTORCRAFT_TOOLS {
            assert!(names.contains(name), "{name} is missing");
        }
        for tool in tools {
            let required = tool.input_schema.get("required").and_then(Value::as_array).cloned().unwrap_or_default();
            assert!(required.contains(&json!("session")), "{}", tool.name);
            assert!(tool.input_schema["properties"].get("session").is_some(), "{}", tool.name);
            // Brand policy: tool descriptions don't present themselves as VectorCraft (docs/design-policy.md chapter 9)
            let description = tool.description.as_deref().unwrap_or_default();
            assert!(!description.contains("VectorCraft"), "{}: {description}", tool.name);
        }
    }

    #[test]
    fn vectorcraft_results_keep_their_content_and_error_flag() {
        let ok = tool_result(ToolResult::text("done"));
        assert_eq!(ok.is_error, Some(false));
        assert!(ok.structured_content.is_none());
        assert_eq!(ok.content[0].as_text().unwrap().text, "done");

        let err = tool_result(ToolResult::error("nope"));
        assert_eq!(err.is_error, Some(true));

        let image = tool_result(ToolResult {
            content: vec![json!({"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"})],
            is_error: false,
        });
        assert!(image.content[0].as_image().is_some());
    }
}
