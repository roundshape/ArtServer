//! Files that clients send to ArtServer (docs/design-policy.md section 4.6): the `PUT /uploads/{id}` endpoint
//! that receives one, a page at `GET /uploads/{id}` for sending it from a browser, and the formats accepted.
//!
//! The upload_create tool makes the URL, and open_upload and place_upload pass the contents to the engine as
//! data, so the engine still never touches a file. As with export URLs, the ID in the URL is the key and no
//! token is needed. The format is told from the contents, not from the name, so that only the formats
//! listed here reach the engine's readers.

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::files::{self, FileStore, UploadError, file_name};

/// The formats accepted, as the tools and the page describe them.
pub const ACCEPTED: &str = "PDF (including Illustrator .ai files saved with PDF compatibility), SVG, PNG or JPEG";

/// How many leading bytes are searched for an SVG's root element.
const SVG_SNIFF_BYTES: usize = 4096;

/// A format a file may be sent in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Pdf,
    Svg,
    Png,
    Jpeg,
}

impl Format {
    const ALL: [Format; 4] = [Format::Pdf, Format::Svg, Format::Png, Format::Jpeg];

    /// The format of `bytes`, told from how they start. `None` for anything else, including compressed SVG,
    /// PostScript (EPS and older .ai files) and the other formats the engine could read.
    pub fn sniff(bytes: &[u8]) -> Option<Format> {
        if bytes.starts_with(b"%PDF-") {
            return Some(Format::Pdf);
        }
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Some(Format::Png);
        }
        if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Some(Format::Jpeg);
        }
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(SVG_SNIFF_BYTES)]);
        let head = head.trim_start_matches('\u{feff}').trim_start();
        (head.starts_with('<') && head.contains("<svg")).then_some(Format::Svg)
    }

    pub fn id(self) -> &'static str {
        match self {
            Format::Pdf => "pdf",
            Format::Svg => "svg",
            Format::Png => "png",
            Format::Jpeg => "jpeg",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Pdf => "application/pdf",
            Format::Svg => "image/svg+xml",
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Format::Jpeg => "jpg",
            other => other.id(),
        }
    }

    fn from_mime(mime: &str) -> Option<Format> {
        Self::ALL.into_iter().find(|f| f.mime() == mime)
    }
}

/// The name the engine gets for an uploaded file: the stem of the name the client gave, with the extension of
/// the format the contents are in. A PDF named .ai keeps .ai, so that it opens as an Illustrator file.
pub fn engine_name(given: &str, mime: &str) -> Option<String> {
    let format = Format::from_mime(mime)?;
    let path = std::path::Path::new(given);
    let ai = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("ai"));
    let extension = if format == Format::Pdf && ai { "ai" } else { format.extension() };
    Some(file_name(path.file_stem().and_then(|s| s.to_str()), extension))
}

/// What the upload endpoints use.
#[derive(Clone)]
pub struct Uploads {
    pub store: Arc<FileStore>,
    pub allowed_hosts: Arc<[String]>,
    pub allowed_origins: Arc<[String]>,
    /// --public-url, whose origin also counts as this server's own
    pub public_url: Option<Arc<str>>,
    /// Size limit of one file (`--max-upload-mib`), shown on the page. The router enforces it
    pub max_bytes: usize,
}

impl Uploads {
    /// A response refusing the request when its Host or Origin isn't allowed. An Origin passes when it is on
    /// the allowlist or is this server's own (the page at `GET /uploads/{id}` sends the file from there).
    fn refuse(&self, headers: &HeaderMap) -> Option<Response> {
        if !files::host_allowed(headers, &self.allowed_hosts) {
            return Some((StatusCode::FORBIDDEN, "forbidden: Host header is not allowed").into_response());
        }
        let own = |origin: &str| {
            let origin = origin.trim().trim_end_matches('/');
            self.own_origins(headers).iter().any(|o| o.eq_ignore_ascii_case(origin))
        };
        let origin_ok =
            files::origin_allowed(headers, &self.allowed_origins) || headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()).is_some_and(own);
        (!origin_ok).then(|| (StatusCode::FORBIDDEN, "forbidden: Origin header is not allowed").into_response())
    }

    /// This server's own origins: the one the request reached, and that of --public-url.
    fn own_origins(&self, headers: &HeaderMap) -> Vec<String> {
        let mut own = vec![files::base_url(headers)];
        if let Some(url) = &self.public_url
            && let Ok(uri) = url.parse::<http::Uri>()
            && let (Some(scheme), Some(authority)) = (uri.scheme_str(), uri.authority())
        {
            own.push(format!("{scheme}://{authority}"));
        }
        own
    }
}

/// Receives the file for an upload: its raw bytes as the body of a PUT (`curl -T <file> <url>`). The router
/// limits the body to `--max-upload-mib`.
pub async fn receive(State(u): State<Uploads>, Path(id): Path<String>, headers: HeaderMap, body: Bytes) -> Response {
    if let Some(refused) = u.refuse(&headers) {
        return refused;
    }
    match u.store.upload_received(&id) {
        None => return refusal(&UploadError::Unknown),
        Some(true) => return refusal(&UploadError::AlreadyReceived),
        Some(false) => {}
    }
    let Some(format) = Format::sniff(&body) else {
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, format!("unsupported file: send {ACCEPTED}")).into_response();
    };
    let bytes = body.len();
    match u.store.receive_upload(&id, format.mime(), body) {
        Ok(kept) => Json(json!({
            "bytes": bytes,
            "format": format.id(),
            "expiresInSeconds": kept.as_secs(),
            "next": "Call open_upload or place_upload with this upload.",
        }))
        .into_response(),
        Err(e) => refusal(&e),
    }
}

fn refusal(e: &UploadError) -> Response {
    let status = match e {
        UploadError::Unknown => StatusCode::NOT_FOUND,
        UploadError::AlreadyReceived => StatusCode::CONFLICT,
        UploadError::NotReceived => StatusCode::BAD_REQUEST,
        UploadError::Full { .. } => StatusCode::INSUFFICIENT_STORAGE,
    };
    (status, e.to_string()).into_response()
}

/// A page for sending the file from a browser, for clients that can't send it themselves. Its script, served
/// from this server, sends the file with a PUT to the same URL.
pub async fn page(State(u): State<Uploads>, Path(id): Path<String>, headers: HeaderMap) -> Response {
    if let Some(refused) = u.refuse(&headers) {
        return refused;
    }
    let html = match u.store.upload_received(&id) {
        None => return refusal(&UploadError::Unknown),
        Some(true) => SENT_PAGE.to_string(),
        Some(false) => PAGE.replace("{accepted}", ACCEPTED).replace("{max}", &size_text(u.max_bytes)),
    };
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8")),
            (header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(PAGE_POLICY)),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
            (header::REFERRER_POLICY, HeaderValue::from_static("no-referrer")),
        ],
        html,
    )
        .into_response()
}

/// The script of the page.
pub async fn script() -> Response {
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/javascript; charset=utf-8")),
            (header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
        ],
        SCRIPT,
    )
        .into_response()
}

fn size_text(bytes: usize) -> String {
    const MIB: usize = 1024 * 1024;
    if bytes >= MIB {
        format!("{} MiB", bytes / MIB)
    } else {
        format!("{bytes} bytes")
    }
}

/// The page runs only its own script, which may only talk to this server.
const PAGE_POLICY: &str = "default-src 'none'; script-src 'self'; connect-src 'self'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

const PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Send a file to ArtServer</title>
<style>body{font:16px/1.5 system-ui,sans-serif;margin:2rem auto;max-width:36rem;padding:0 1rem}button{font:inherit;margin-top:1rem}</style>
</head>
<body>
<h1>Send a file to ArtServer</h1>
<p>{accepted}, up to {max}. This link takes one file and expires in a few minutes.</p>
<input type="file" id="file">
<br><button type="button" id="send">Send</button>
<p id="status" role="status"></p>
<script src="upload.js"></script>
</body>
</html>
"#;

const SENT_PAGE: &str = r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Send a file to ArtServer</title></head>
<body><p>The file has been sent. You can close this page.</p></body>
</html>
"#;

const SCRIPT: &str = r#""use strict";
const send = document.getElementById("send");
const status = document.getElementById("status");
send.addEventListener("click", async () => {
  const file = document.getElementById("file").files[0];
  if (!file) {
    status.textContent = "Choose a file first.";
    return;
  }
  send.disabled = true;
  status.textContent = "Sending…";
  try {
    const response = await fetch(location.href, { method: "PUT", body: file });
    if (response.ok) {
      status.textContent = "Sent. You can close this page and tell the AI the file is there.";
    } else {
      status.textContent = "Not sent: " + (await response.text());
      send.disabled = false;
    }
  } catch (e) {
    status.textContent = "Not sent: " + e;
    send.disabled = false;
  }
});
"#;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn formats_are_told_from_the_contents() {
        assert_eq!(Format::sniff(b"%PDF-1.7\n..."), Some(Format::Pdf));
        assert_eq!(Format::sniff(b"\x89PNG\r\n\x1a\n...."), Some(Format::Png));
        assert_eq!(Format::sniff(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10]), Some(Format::Jpeg));
        assert_eq!(Format::sniff(br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#), Some(Format::Svg));
        assert_eq!(
            Format::sniff("\u{feff}\n<?xml version=\"1.0\"?>\n<!-- a -->\n<svg/>".as_bytes()),
            Some(Format::Svg)
        );
        for other in [
            &b"%!PS-Adobe-3.0 EPSF-3.0"[..],
            b"\x1f\x8b\x08\x00",
            b"GIF89a",
            b"8BPS",
            b"<html><body>no image</body></html>",
            b"{\"format\":\"vectorcraft\"}",
            b"",
        ] {
            assert_eq!(Format::sniff(other), None, "{other:?}");
        }
    }

    #[test]
    fn the_engine_gets_the_extension_of_the_contents() {
        assert_eq!(engine_name("logo.png", "image/png").unwrap(), "logo.png");
        assert_eq!(engine_name("photo", "image/jpeg").unwrap(), "photo.jpg");
        assert_eq!(engine_name("art.AI", "application/pdf").unwrap(), "art.ai");
        assert_eq!(engine_name("art.ai", "image/png").unwrap(), "art.png");
        assert_eq!(engine_name("../../etc/passwd.pdf", "application/pdf").unwrap(), "passwd.pdf");
        assert_eq!(engine_name("", "image/svg+xml").unwrap(), "document.svg");
        assert!(engine_name("a.pdf", "").is_none());
    }

    fn uploads(public_url: Option<&str>) -> Uploads {
        Uploads {
            store: Arc::new(FileStore::new(Duration::from_secs(60), 1 << 20)),
            allowed_hosts: vec!["127.0.0.1".to_string(), "example.com".to_string()].into(),
            allowed_origins: Vec::new().into(),
            public_url: public_url.map(Arc::from),
            max_bytes: 1 << 20,
        }
    }

    fn headers(host: &str, origin: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        if let Some(origin) = origin {
            h.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
        }
        h
    }

    #[test]
    fn only_this_servers_own_origin_passes_without_the_allowlist() {
        let u = uploads(None);
        assert!(u.refuse(&headers("127.0.0.1:8471", None)).is_none());
        assert!(u.refuse(&headers("127.0.0.1:8471", Some("http://127.0.0.1:8471"))).is_none());
        assert!(u.refuse(&headers("127.0.0.1:8471", Some("http://evil.example"))).is_some());
        assert!(u.refuse(&headers("127.0.0.1:8471", Some("http://127.0.0.1:9999"))).is_some());
        assert!(u.refuse(&headers("evil.example", None)).is_some());

        let behind_proxy = uploads(Some("https://example.com/artserver"));
        assert!(behind_proxy.refuse(&headers("127.0.0.1:8471", Some("https://example.com"))).is_none());
        assert!(behind_proxy.refuse(&headers("127.0.0.1:8471", Some("http://example.com"))).is_some());
    }
}
