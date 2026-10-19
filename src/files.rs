//! A temporary store for files passed between clients and the engine: large exports, which the
//! `GET /files/{id}` endpoint here serves, and uploaded files, which `PUT /uploads/{id}` (src/uploads.rs)
//! receives (docs/design-policy.md section 4.6).
//!
//! The contents are kept only in memory, never on disk. The ID in the URL (256 random bits) is the key, so a
//! file can be fetched or sent without a token (so that browsers and clients that can't run curl can use the
//! link). Expired files and those of closed working sessions are dropped.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// Exports, and uploads, that one working session may keep at once. Past that, the oldest go first.
const MAX_FILES_PER_SESSION: usize = 8;

/// Number of random bytes in an ID. The URL itself is the key, so it must be too long to guess.
const ID_BYTES: usize = 32;

/// Maximum number of characters in a file name (without the extension).
const MAX_NAME_CHARS: usize = 100;

/// Extensions of the files this server opens or writes. A document opened from a file takes the file's name as
/// its title, extension included, which a file name made from the title drops.
const FILE_EXTENSIONS: &[&str] = &["svg", "pdf", "ai", "png", "jpg", "jpeg", "vectorcraft"];

/// Why a file couldn't be stored.
#[derive(Debug)]
pub enum FileError {
    /// It would go over the total limit
    Full {
        limit_bytes: usize,
    },
    Internal(String),
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileError::Full { limit_bytes } => write!(
                f,
                "the server is already holding {} MiB of exports and uploads; download or let earlier ones expire, or close sessions",
                limit_bytes / (1024 * 1024)
            ),
            FileError::Internal(m) => write!(f, "internal error: {m}"),
        }
    }
}

/// Why an upload couldn't be received or used.
#[derive(Debug, PartialEq, Eq)]
pub enum UploadError {
    /// No such upload: unknown, expired or of another working session (not told apart)
    Unknown,
    /// The upload already has its file
    AlreadyReceived,
    /// The file hasn't been sent yet
    NotReceived,
    /// It would go over the total limit
    Full { limit_bytes: usize },
}

impl fmt::Display for UploadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UploadError::Unknown => f.write_str("unknown or expired upload; get a new URL with upload_create"),
            UploadError::AlreadyReceived => f.write_str("this upload already has its file; get a new URL with upload_create to send another"),
            UploadError::NotReceived => f.write_str("the file has not been sent yet: send it to the upload URL with an HTTP PUT first"),
            UploadError::Full { limit_bytes } => write!(
                f,
                "the server is already holding {} MiB of exports and uploads; let earlier ones expire, or close sessions",
                limit_bytes / (1024 * 1024)
            ),
        }
    }
}

/// The result of storing a file.
#[derive(Debug)]
pub struct Deposit {
    pub id: String,
    pub expires_in: Duration,
}

/// A stored file.
#[derive(Debug, Clone)]
pub struct StoredFile {
    pub name: String,
    pub mime: &'static str,
    pub data: Bytes,
}

/// What a stored file is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// An export, served by `GET /files/{id}`
    Export,
    /// A file a client sends with `PUT /uploads/{id}`. Until `received`, the entry holds only its name
    Upload { received: bool },
}

impl Kind {
    fn is_upload(self) -> bool {
        matches!(self, Kind::Upload { .. })
    }
}

struct Entry {
    session: String,
    kind: Kind,
    file: StoredFile,
    expires: Instant,
}

/// The file store.
pub struct FileStore {
    entries: Mutex<HashMap<String, Entry>>,
    ttl: Duration,
    max_bytes: usize,
}

impl FileStore {
    pub fn new(ttl: Duration, max_bytes: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
            max_bytes,
        }
    }

    fn entries(&self) -> MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Stores an export of working session `session` and returns the ID to fetch it with.
    pub fn put(&self, session: &str, file: StoredFile) -> Result<Deposit, FileError> {
        self.insert(session, Kind::Export, file)
    }

    /// Makes an upload for working session `session` and returns the ID to send the file to. `name` is the
    /// file's name as the client gave it.
    pub fn reserve_upload(&self, session: &str, name: &str) -> Result<Deposit, FileError> {
        let slot = StoredFile {
            name: name.chars().take(MAX_NAME_CHARS * 2).collect(),
            mime: "",
            data: Bytes::new(),
        };
        self.insert(session, Kind::Upload { received: false }, slot)
    }

    fn insert(&self, session: &str, kind: Kind, file: StoredFile) -> Result<Deposit, FileError> {
        let mut entries = self.entries();
        let now = Instant::now();
        entries.retain(|_, e| e.expires > now);
        let same = |e: &Entry| e.session == session && e.kind.is_upload() == kind.is_upload();
        while entries.values().filter(|e| same(e)).count() >= MAX_FILES_PER_SESSION {
            let oldest = entries
                .iter()
                .filter(|(_, e)| same(e))
                .min_by_key(|(_, e)| e.expires)
                .map(|(id, _)| id.clone());
            let Some(id) = oldest else { break };
            entries.remove(&id);
        }
        if held_bytes(&entries).saturating_add(file.data.len()) > self.max_bytes {
            return Err(FileError::Full { limit_bytes: self.max_bytes });
        }
        let id = new_id()?;
        entries.insert(
            id.clone(),
            Entry {
                session: session.to_string(),
                kind,
                file,
                expires: now + self.ttl,
            },
        );
        Ok(Deposit { id, expires_in: self.ttl })
    }

    /// An export that hasn't expired. It can be fetched any number of times until then (so that clients that
    /// prefetch the links they show don't make it disappear).
    pub fn get(&self, id: &str) -> Option<StoredFile> {
        let entries = self.entries();
        entries
            .get(id)
            .filter(|e| e.kind == Kind::Export && e.expires > Instant::now())
            .map(|e| e.file.clone())
    }

    /// Puts the file sent for upload `id` in place, once, and returns how long it is kept from now.
    pub fn receive_upload(&self, id: &str, mime: &'static str, data: Bytes) -> Result<Duration, UploadError> {
        let mut entries = self.entries();
        let now = Instant::now();
        let held = held_bytes(&entries);
        let entry = entries
            .get_mut(id)
            .filter(|e| e.kind.is_upload() && e.expires > now)
            .ok_or(UploadError::Unknown)?;
        if entry.kind == (Kind::Upload { received: true }) {
            return Err(UploadError::AlreadyReceived);
        }
        if held.saturating_add(data.len()) > self.max_bytes {
            return Err(UploadError::Full { limit_bytes: self.max_bytes });
        }
        entry.kind = Kind::Upload { received: true };
        entry.file.mime = mime;
        entry.file.data = data;
        entry.expires = now + self.ttl;
        Ok(self.ttl)
    }

    /// The file sent for upload `id` of working session `session`. It can be used any number of times until it
    /// expires (to place the same image again, for example).
    pub fn upload(&self, id: &str, session: &str) -> Result<StoredFile, UploadError> {
        let entries = self.entries();
        let entry = entries
            .get(id)
            .filter(|e| e.kind.is_upload() && e.session == session && e.expires > Instant::now())
            .ok_or(UploadError::Unknown)?;
        match entry.kind {
            Kind::Upload { received: true } => Ok(entry.file.clone()),
            _ => Err(UploadError::NotReceived),
        }
    }

    /// Whether upload `id` has its file (`Some(true)`) or still waits for it (`Some(false)`). `None` when
    /// there is no such upload.
    pub fn upload_received(&self, id: &str) -> Option<bool> {
        let entries = self.entries();
        match entries.get(id).filter(|e| e.expires > Instant::now())?.kind {
            Kind::Upload { received } => Some(received),
            Kind::Export => None,
        }
    }

    /// Drops the files of a closed working session and returns how many.
    pub fn remove_session(&self, session: &str) -> usize {
        let mut entries = self.entries();
        let before = entries.len();
        entries.retain(|_, e| e.session != session);
        before - entries.len()
    }

    /// Drops the expired files and returns how many.
    pub fn purge_expired(&self) -> usize {
        let mut entries = self.entries();
        let before = entries.len();
        let now = Instant::now();
        entries.retain(|_, e| e.expires > now);
        before - entries.len()
    }
}

fn held_bytes(entries: &HashMap<String, Entry>) -> usize {
    entries.values().map(|e| e.file.data.len()).sum()
}

fn new_id() -> Result<String, FileError> {
    let mut bytes = [0u8; ID_BYTES];
    getrandom::fill(&mut bytes).map_err(|e| FileError::Internal(format!("no randomness: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Makes a file name from the document's name, without path separators or control characters, and without
/// the extension of a file it was opened from.
pub fn file_name(title: Option<&str>, extension: &str) -> String {
    let title = title.unwrap_or_default();
    let title = match title.rsplit_once('.') {
        Some((stem, ext)) if FILE_EXTENSIONS.iter().any(|e| e.eq_ignore_ascii_case(ext)) => stem,
        _ => title,
    };
    let stem: String = title
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '/' | '\\' | '"' | ':' | '*' | '?' | '<' | '>' | '|'))
        .take(MAX_NAME_CHARS)
        .collect();
    let stem = stem.trim().trim_matches('.');
    let stem = if stem.is_empty() { "document" } else { stem };
    format!("{stem}.{extension}")
}

/// What the `GET /files/{id}` endpoint uses.
#[derive(Clone)]
pub struct Downloads {
    pub store: Arc<FileStore>,
    pub allowed_hosts: Arc<[String]>,
    pub allowed_origins: Arc<[String]>,
}

/// Returns a stored file. No token is required; the ID in the URL is enough. Host and Origin are checked
/// against the same allowlists as `/mcp` (rmcp's checks apply only to `/mcp`). The contents are never opened
/// inside the browser but always downloaded (so that scripts in an SVG, for example, never run on this
/// endpoint's origin).
pub async fn download(State(d): State<Downloads>, Path(id): Path<String>, headers: HeaderMap) -> Response {
    if !host_allowed(&headers, &d.allowed_hosts) {
        return (StatusCode::FORBIDDEN, "forbidden: Host header is not allowed").into_response();
    }
    if !origin_allowed(&headers, &d.allowed_origins) {
        return (StatusCode::FORBIDDEN, "forbidden: Origin header is not allowed").into_response();
    }
    let Some(file) = d.store.get(&id) else {
        return (StatusCode::NOT_FOUND, "not found or expired").into_response();
    };
    let disposition = HeaderValue::from_str(&content_disposition(&file.name)).unwrap_or(HeaderValue::from_static("attachment"));
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(file.mime)),
            (header::CONTENT_DISPOSITION, disposition),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
            (header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox")),
            (header::REFERRER_POLICY, HeaderValue::from_static("no-referrer")),
        ],
        file.data,
    )
        .into_response()
}

/// Writes both an ASCII fallback name and the UTF-8 name (RFC 6266 / RFC 8187).
fn content_disposition(name: &str) -> String {
    let ascii: String = name.chars().map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '_' }).collect();
    let encoded: String = name
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => char::from(b).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

/// The URL of the ArtServer a request reached (such as `http://127.0.0.1:8471`), from its Host, which the
/// endpoints have already checked against the allowlist. Behind a reverse proxy, the https of
/// X-Forwarded-Proto is used. Where the proxy adds a path prefix (such as /artserver/) or rewrites Host, use
/// --public-url.
pub fn base_url(headers: &HeaderMap) -> String {
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("127.0.0.1");
    let scheme = match headers.get("x-forwarded-proto").and_then(|v| v.to_str().ok()) {
        Some(p) if p.eq_ignore_ascii_case("https") => "https",
        _ => "http",
    };
    format!("{scheme}://{host}")
}

/// Is the Host on the allowlist? When an allowlist entry has a port, the port is compared too (as rmcp does).
pub(crate) fn host_allowed(headers: &HeaderMap, allowed: &[String]) -> bool {
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .and_then(|h| http::uri::Authority::try_from(h).ok())
    else {
        return false;
    };
    let (name, port) = (normalize_host(host.host()), host.port_u16());
    allowed.iter().any(|a| {
        let a = a.trim();
        let (allowed_name, allowed_port) = match http::uri::Authority::try_from(a) {
            Ok(x) => (normalize_host(x.host()), x.port_u16()),
            Err(_) => (normalize_host(a), None),
        };
        allowed_name == name && allowed_port.is_none_or(|p| port == Some(p))
    })
}

/// Passes a request without an Origin. One with an Origin passes only when it is on the allowlist.
pub(crate) fn origin_allowed(headers: &HeaderMap, allowed: &[String]) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let origin = origin.trim().trim_end_matches('/');
    allowed.iter().any(|a| a.trim().trim_end_matches('/').eq_ignore_ascii_case(origin))
}

fn normalize_host(host: &str) -> String {
    host.trim_matches('[').trim_matches(']').to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(bytes: usize) -> StoredFile {
        StoredFile {
            name: "a.pdf".into(),
            mime: "application/pdf",
            data: Bytes::from(vec![b'x'; bytes]),
        }
    }

    #[test]
    fn files_are_found_by_their_id_until_they_expire() {
        let store = FileStore::new(Duration::from_secs(60), 1 << 20);
        let deposit = store.put("s1", file(10)).unwrap();
        assert_eq!(deposit.id.len(), ID_BYTES * 2);
        // Can be fetched any number of times until it expires
        assert_eq!(store.get(&deposit.id).unwrap().data.len(), 10);
        assert!(store.get(&deposit.id).is_some());
        assert!(store.get("unknown").is_none());
    }

    #[test]
    fn expired_files_are_gone() {
        let store = FileStore::new(Duration::ZERO, 1 << 20);
        let deposit = store.put("s1", file(10)).unwrap();
        assert!(store.get(&deposit.id).is_none());
        assert_eq!(store.purge_expired(), 1);
    }

    #[test]
    fn a_session_keeps_only_its_latest_files() {
        let store = FileStore::new(Duration::from_secs(60), 1 << 20);
        let ids: Vec<String> = (0..MAX_FILES_PER_SESSION + 2).map(|_| store.put("s1", file(1)).unwrap().id).collect();
        assert!(store.get(&ids[0]).is_none(), "the oldest is dropped");
        assert!(store.get(&ids[1]).is_none());
        assert!(ids[2..].iter().all(|id| store.get(id).is_some()));
        // Files of other working sessions don't count
        assert!(store.put("s2", file(1)).is_ok());
    }

    #[test]
    fn the_total_size_is_capped() {
        let store = FileStore::new(Duration::from_secs(60), 100);
        store.put("s1", file(60)).unwrap();
        assert!(matches!(store.put("s2", file(60)), Err(FileError::Full { .. })));
        assert!(store.put("s2", file(40)).is_ok());
    }

    #[test]
    fn closing_a_session_drops_its_files() {
        let store = FileStore::new(Duration::from_secs(60), 1 << 20);
        let mine = store.put("s1", file(1)).unwrap();
        let other = store.put("s2", file(1)).unwrap();
        assert_eq!(store.remove_session("s1"), 1);
        assert!(store.get(&mine.id).is_none());
        assert!(store.get(&other.id).is_some());
    }

    #[test]
    fn an_upload_takes_one_file_for_its_own_session() {
        let store = FileStore::new(Duration::from_secs(60), 1 << 20);
        let upload = store.reserve_upload("s1", "logo.png").unwrap();
        assert_eq!(store.upload_received(&upload.id), Some(false));
        assert_eq!(store.upload(&upload.id, "s1").unwrap_err(), UploadError::NotReceived);

        store.receive_upload(&upload.id, "image/png", Bytes::from_static(b"png")).unwrap();
        assert_eq!(store.upload_received(&upload.id), Some(true));
        let sent = store.upload(&upload.id, "s1").unwrap();
        assert_eq!((sent.name.as_str(), sent.mime, &sent.data[..]), ("logo.png", "image/png", &b"png"[..]));
        // Usable again until it expires, but never sent twice, used by another session or served as an export
        assert!(store.upload(&upload.id, "s1").is_ok());
        assert_eq!(
            store.receive_upload(&upload.id, "image/png", Bytes::from_static(b"x")).unwrap_err(),
            UploadError::AlreadyReceived
        );
        assert_eq!(store.upload(&upload.id, "s2").unwrap_err(), UploadError::Unknown);
        assert!(store.get(&upload.id).is_none());

        // An export is never taken for an upload
        let export = store.put("s1", file(1)).unwrap();
        assert_eq!(store.upload_received(&export.id), None);
        assert_eq!(
            store.receive_upload(&export.id, "image/png", Bytes::from_static(b"x")).unwrap_err(),
            UploadError::Unknown
        );
        assert_eq!(store.upload(&export.id, "s1").unwrap_err(), UploadError::Unknown);
    }

    #[test]
    fn uploads_share_the_limits_of_the_store() {
        let store = FileStore::new(Duration::from_secs(60), 100);
        store.put("s1", file(60)).unwrap();
        let upload = store.reserve_upload("s1", "a.pdf").unwrap();
        assert!(matches!(
            store.receive_upload(&upload.id, "application/pdf", Bytes::from(vec![b'x'; 60])),
            Err(UploadError::Full { .. })
        ));
        store.receive_upload(&upload.id, "application/pdf", Bytes::from(vec![b'x'; 40])).unwrap();
        assert!(matches!(store.put("s1", file(1)), Err(FileError::Full { .. })));

        // Uploads and exports are counted apart, and closing the session drops both
        let store = FileStore::new(Duration::from_secs(60), 1 << 20);
        let first = store.reserve_upload("s1", "a.png").unwrap();
        for _ in 0..MAX_FILES_PER_SESSION {
            store.put("s1", file(1)).unwrap();
        }
        assert_eq!(store.upload_received(&first.id), Some(false));
        for _ in 0..MAX_FILES_PER_SESSION {
            store.reserve_upload("s1", "b.png").unwrap();
        }
        assert_eq!(store.upload_received(&first.id), None, "the oldest upload is dropped");
        assert_eq!(store.remove_session("s1"), 2 * MAX_FILES_PER_SESSION);

        let expired = FileStore::new(Duration::ZERO, 1 << 20);
        let upload = expired.reserve_upload("s1", "a.png").unwrap();
        assert_eq!(
            expired.receive_upload(&upload.id, "image/png", Bytes::from_static(b"x")).unwrap_err(),
            UploadError::Unknown
        );
    }

    #[test]
    fn the_base_url_comes_from_the_request() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:8471"));
        assert_eq!(base_url(&headers), "http://127.0.0.1:8471");
        headers.insert("x-forwarded-proto", HeaderValue::from_static("HTTPS"));
        assert_eq!(base_url(&headers), "https://127.0.0.1:8471");
    }

    #[test]
    fn file_names_come_from_the_document_title() {
        assert_eq!(file_name(Some("ポスター 2026"), "pdf"), "ポスター 2026.pdf");
        assert_eq!(file_name(Some("../etc/passwd"), "svg"), "etcpasswd.svg");
        assert_eq!(file_name(Some("  "), "png"), "document.png");
        assert_eq!(file_name(None, "pdf"), "document.pdf");
        // A document opened from a file is titled with the file's name
        assert_eq!(file_name(Some("poster.ai"), "vectorcraft"), "poster.vectorcraft");
        assert_eq!(file_name(Some("logo.PNG"), "pdf"), "logo.pdf");
        assert_eq!(file_name(Some("v1.2 draft"), "pdf"), "v1.2 draft.pdf");
        let disposition = content_disposition("ポスター.pdf");
        assert!(
            disposition.starts_with("attachment; filename=\"____.pdf\"; filename*=UTF-8''%E3%83%9D"),
            "{disposition}"
        );
    }

    #[test]
    fn hosts_and_origins_follow_the_allowlists() {
        let allowed: Vec<String> = vec!["localhost".into(), "127.0.0.1".into(), "::1".into(), "example.com:8443".into()];
        let host = |h: &str| {
            let mut m = HeaderMap::new();
            m.insert(header::HOST, HeaderValue::from_str(h).unwrap());
            m
        };
        assert!(host_allowed(&host("127.0.0.1:8471"), &allowed));
        assert!(host_allowed(&host("LOCALHOST"), &allowed));
        assert!(host_allowed(&host("[::1]:8471"), &allowed));
        assert!(host_allowed(&host("example.com:8443"), &allowed));
        assert!(!host_allowed(&host("example.com:80"), &allowed));
        assert!(!host_allowed(&host("evil.example"), &allowed));
        assert!(!host_allowed(&HeaderMap::new(), &allowed));

        let mut with_origin = HeaderMap::new();
        assert!(origin_allowed(&with_origin, &[]));
        with_origin.insert(header::ORIGIN, HeaderValue::from_static("http://evil.example"));
        assert!(!origin_allowed(&with_origin, &[]));
        assert!(origin_allowed(&with_origin, &["http://evil.example/".to_string()]));
    }
}
