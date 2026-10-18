//! 大きい書き出し結果の一時的な預かり場所と、それを取りに来る `GET /files/{id}` の窓口
//! （docs/design-policy.md 4.6節）。
//!
//! 中身はメモリにだけ持ち、ディスクには書かない。URL の ID（256ビットの乱数）を合鍵にして、
//! トークンなしで取れる（ブラウザや、curl を使えないクライアントでもリンクから取れるように）。
//! 期限が過ぎたものと、閉じた作業セッションのものは捨てる。

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// 1つの作業セッションが同時に預けられるファイルの数。超えたら古いものから捨てる。
const MAX_FILES_PER_SESSION: usize = 8;

/// ID の乱数のバイト数。URL がそのまま合鍵になるので、推測できない長さにする。
const ID_BYTES: usize = 32;

/// ファイル名の文字数の上限（拡張子を除く）。
const MAX_NAME_CHARS: usize = 100;

/// 預けられなかった理由。
#[derive(Debug)]
pub enum FileError {
    /// 全体の合計の上限を超える
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
                "the server is already holding {} MiB of exports; download or let earlier ones expire, or close sessions",
                limit_bytes / (1024 * 1024)
            ),
            FileError::Internal(m) => write!(f, "internal error: {m}"),
        }
    }
}

/// 預けた結果。
#[derive(Debug)]
pub struct Deposit {
    pub id: String,
    pub expires_in: Duration,
}

/// 預かっているファイル。
#[derive(Debug, Clone)]
pub struct StoredFile {
    pub name: String,
    pub mime: &'static str,
    pub data: Bytes,
}

struct Entry {
    session: String,
    file: StoredFile,
    expires: Instant,
}

/// 預かり場所。
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

    /// 作業セッション `session` の書き出し結果を預かり、取りに来るための ID を返す。
    pub fn put(&self, session: &str, file: StoredFile) -> Result<Deposit, FileError> {
        let mut entries = self.entries();
        let now = Instant::now();
        entries.retain(|_, e| e.expires > now);
        while entries.values().filter(|e| e.session == session).count() >= MAX_FILES_PER_SESSION {
            let oldest = entries
                .iter()
                .filter(|(_, e)| e.session == session)
                .min_by_key(|(_, e)| e.expires)
                .map(|(id, _)| id.clone());
            let Some(id) = oldest else { break };
            entries.remove(&id);
        }
        let held: usize = entries.values().map(|e| e.file.data.len()).sum();
        if held.saturating_add(file.data.len()) > self.max_bytes {
            return Err(FileError::Full { limit_bytes: self.max_bytes });
        }
        let id = new_id()?;
        entries.insert(
            id.clone(),
            Entry {
                session: session.to_string(),
                file,
                expires: now + self.ttl,
            },
        );
        Ok(Deposit { id, expires_in: self.ttl })
    }

    /// 期限内のファイル。期限までは何度でも取れる（表示したリンクを先読みするクライアントでも消えないように）。
    pub fn get(&self, id: &str) -> Option<StoredFile> {
        let entries = self.entries();
        entries.get(id).filter(|e| e.expires > Instant::now()).map(|e| e.file.clone())
    }

    /// 閉じた作業セッションのファイルを捨て、その数を返す。
    pub fn remove_session(&self, session: &str) -> usize {
        let mut entries = self.entries();
        let before = entries.len();
        entries.retain(|_, e| e.session != session);
        before - entries.len()
    }

    /// 期限の過ぎたファイルを捨て、その数を返す。
    pub fn purge_expired(&self) -> usize {
        let mut entries = self.entries();
        let before = entries.len();
        let now = Instant::now();
        entries.retain(|_, e| e.expires > now);
        before - entries.len()
    }
}

fn new_id() -> Result<String, FileError> {
    let mut bytes = [0u8; ID_BYTES];
    getrandom::fill(&mut bytes).map_err(|e| FileError::Internal(format!("no randomness: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// 文書の名前からファイル名を作る。パスの区切りや制御文字は使わない。
pub fn file_name(title: Option<&str>, extension: &str) -> String {
    let stem: String = title
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '/' | '\\' | '"' | ':' | '*' | '?' | '<' | '>' | '|'))
        .take(MAX_NAME_CHARS)
        .collect();
    let stem = stem.trim().trim_matches('.');
    let stem = if stem.is_empty() { "document" } else { stem };
    format!("{stem}.{extension}")
}

/// `GET /files/{id}` の窓口が使うもの。
#[derive(Clone)]
pub struct Downloads {
    pub store: Arc<FileStore>,
    pub allowed_hosts: Arc<[String]>,
    pub allowed_origins: Arc<[String]>,
}

/// 預かったファイルを返す。トークンは求めず、URL の ID だけで取れる。Host と Origin は `/mcp` と
/// 同じ許可リストで検査する（rmcp の検査は `/mcp` にしか掛からないため）。中身はブラウザの中で
/// 開かず、必ずダウンロードさせる（SVG のスクリプトなどを、この窓口のオリジンで動かさないため）。
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

/// ASCII の代わりの名前と、UTF-8 の名前（RFC 6266 / RFC 8187）の両方を書く。
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

/// Host が許可リストにあるか。リストの項目にポートがあればポートも見る（rmcp と同じ考え方）。
fn host_allowed(headers: &HeaderMap, allowed: &[String]) -> bool {
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

/// Origin が付いていなければ通す。付いていれば、許可リストにあるものだけを通す。
fn origin_allowed(headers: &HeaderMap, allowed: &[String]) -> bool {
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
        // 期限までは何度でも取れる
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
        // ほかの作業セッションの分は数えない
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
    fn file_names_come_from_the_document_title() {
        assert_eq!(file_name(Some("ポスター 2026"), "pdf"), "ポスター 2026.pdf");
        assert_eq!(file_name(Some("../etc/passwd"), "svg"), "etcpasswd.svg");
        assert_eq!(file_name(Some("  "), "png"), "document.png");
        assert_eq!(file_name(None, "pdf"), "document.pdf");
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
