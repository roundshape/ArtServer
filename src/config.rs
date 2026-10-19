//! Startup settings. Read from CLI arguments and environment variables into a validated [`Config`].

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;

/// Minimum length of the access token, so that it is hard to guess.
const MIN_TOKEN_LEN: usize = 32;

/// Command-line arguments. Most values can also be given as environment variables.
#[derive(Debug, Parser)]
#[command(name = "artserver", version, about = "Headless vector graphics server driven over MCP (Streamable HTTP)")]
pub struct Cli {
    /// Listen address. Loopback only by default
    #[arg(long, env = "ARTSERVER_BIND", default_value = "127.0.0.1:8471")]
    pub bind: SocketAddr,

    /// Allow listening on non-loopback addresses (only behind a reverse proxy that terminates TLS)
    #[arg(long)]
    pub allow_public_bind: bool,

    /// File containing the access token. If not given, the environment variable ARTSERVER_TOKEN is used
    #[arg(long, env = "ARTSERVER_TOKEN_FILE")]
    pub token_file: Option<PathBuf>,

    /// Host headers to accept (comma-separated)
    #[arg(long, env = "ARTSERVER_ALLOWED_HOSTS", value_delimiter = ',', default_value = "localhost,127.0.0.1,::1")]
    pub allowed_hosts: Vec<String>,

    /// Origins to accept (comma-separated). If empty, every request with an Origin header is refused
    #[arg(long, env = "ARTSERVER_ALLOWED_ORIGINS", value_delimiter = ',')]
    pub allowed_origins: Vec<String>,

    /// Number of working sessions held at once
    #[arg(long, env = "ARTSERVER_MAX_SESSIONS", default_value_t = Limits::default().max_sessions)]
    pub max_sessions: usize,

    /// Seconds before an unused working session is discarded
    #[arg(long, env = "ARTSERVER_SESSION_IDLE_SECS", default_value_t = Limits::default().session_idle.as_secs())]
    pub session_idle_secs: u64,

    /// Time limit for one tool call (seconds). Past it, the engine is asked to cancel
    #[arg(long, env = "ARTSERVER_CALL_TIMEOUT_SECS", default_value_t = Limits::default().call_timeout.as_secs())]
    pub call_timeout_secs: u64,

    /// Maximum request body size (bytes)
    #[arg(long, env = "ARTSERVER_MAX_BODY_BYTES", default_value_t = Limits::default().max_body_bytes)]
    pub max_body_bytes: usize,

    /// Number of documents one working session can have open at once
    #[arg(long, env = "ARTSERVER_MAX_DOCUMENTS", default_value_t = Limits::default().max_documents)]
    pub max_documents: usize,

    /// Undo history limit per document (MiB)
    #[arg(long, env = "ARTSERVER_HISTORY_MIB", default_value_t = Limits::default().history_bytes / MIB)]
    pub history_mib: usize,

    /// Rendering threads per export (0: render on the calling thread only)
    #[arg(long, env = "ARTSERVER_RENDER_THREADS", default_value_t = Limits::default().render_threads)]
    pub render_threads: u16,

    /// Seconds large exports are kept (docs/design-policy.md section 4.6)
    #[arg(long, env = "ARTSERVER_FILE_TTL_SECS", default_value_t = Limits::default().file_ttl.as_secs())]
    pub file_ttl_secs: u64,

    /// Total size limit of the kept exports (MiB). Kept in memory
    #[arg(long, env = "ARTSERVER_MAX_FILE_MIB", default_value_t = Limits::default().max_file_bytes / MIB)]
    pub max_file_mib: usize,

    /// Font folders to use in addition to the fonts installed in the OS (comma-separated), for fonts the
    /// organization provides (docs/design-policy.md section 1.1)
    #[arg(long, env = "ARTSERVER_FONT_DIRS", value_delimiter = ',')]
    pub font_dirs: Vec<PathBuf>,

    /// ArtServer's URL as seen from outside (e.g. https://example.com/artserver). Export URLs are made by adding
    /// /files/… to it. Give it when running behind a reverse proxy. If not given, URLs are made from the incoming
    /// Host and X-Forwarded-Proto. Its host name is added to --allowed-hosts automatically
    #[arg(long, env = "ARTSERVER_PUBLIC_URL")]
    pub public_url: Option<String>,
}

const MIB: usize = 1024 * 1024;

/// Validated settings.
#[derive(Clone, Debug)]
pub struct Config {
    pub bind: SocketAddr,
    pub token: String,
    pub allowed_hosts: Vec<String>,
    pub allowed_origins: Vec<String>,
    /// Extra font folders to load (absolute paths)
    pub font_dirs: Vec<PathBuf>,
    /// ArtServer's URL as seen from outside (without a trailing /). Used for export URLs
    pub public_url: Option<String>,
    pub limits: Limits,
}

/// Resource limits.
#[derive(Clone, Debug)]
pub struct Limits {
    pub max_sessions: usize,
    pub session_idle: Duration,
    pub call_timeout: Duration,
    pub max_body_bytes: usize,
    /// Requests one working session can have waiting. Past that, "busy" is returned
    pub queue_depth: usize,
    pub max_documents: usize,
    pub history_bytes: usize,
    pub render_threads: u16,
    /// How long large exports are kept
    pub file_ttl: Duration,
    /// Total size limit of the kept exports (bytes)
    pub max_file_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_sessions: 8,
            session_idle: Duration::from_secs(30 * 60),
            call_timeout: Duration::from_secs(60),
            max_body_bytes: 16 * MIB,
            queue_depth: 8,
            max_documents: 8,
            history_bytes: 64 * MIB,
            render_threads: 2,
            file_ttl: Duration::from_secs(10 * 60),
            max_file_bytes: 256 * MIB,
        }
    }
}

impl Cli {
    /// Validates the arguments, reads the token and makes a [`Config`].
    pub fn resolve(self) -> Result<Config, String> {
        if !self.bind.ip().is_loopback() && !self.allow_public_bind {
            return Err(format!(
                "refusing to listen on non-loopback address {}; pass --allow-public-bind only behind a TLS-terminating reverse proxy",
                self.bind
            ));
        }
        let token = match &self.token_file {
            Some(path) => std::fs::read_to_string(path).map_err(|e| format!("cannot read the token file {}: {e}", path.display()))?,
            None => std::env::var("ARTSERVER_TOKEN").map_err(|_| "no access token: set ARTSERVER_TOKEN or pass --token-file".to_string())?,
        };
        let token = token.trim().to_string();
        validate_token(&token)?;
        // rmcp lets every Host through when its Host allowlist is empty, so an empty list is not accepted
        let mut allowed_hosts = non_empty(self.allowed_hosts);
        if allowed_hosts.is_empty() {
            return Err("--allowed-hosts must name at least one host".to_string());
        }
        let public_url = public_url(self.public_url)?;
        // A host declared as the outside name passes even when the reverse proxy passes it on as Host unchanged
        if let Some((_, host)) = &public_url
            && !allowed_hosts.iter().any(|h| h.eq_ignore_ascii_case(host))
        {
            allowed_hosts.push(host.clone());
        }
        let limits = Limits {
            max_sessions: self.max_sessions.max(1),
            session_idle: Duration::from_secs(self.session_idle_secs.max(1)),
            call_timeout: Duration::from_secs(self.call_timeout_secs.max(1)),
            max_body_bytes: self.max_body_bytes,
            max_documents: self.max_documents.max(1),
            history_bytes: self.history_mib.saturating_mul(MIB),
            render_threads: self.render_threads,
            file_ttl: Duration::from_secs(self.file_ttl_secs.max(1)),
            max_file_bytes: self.max_file_mib.max(1).saturating_mul(MIB),
            ..Limits::default()
        };
        let font_dirs = font_dirs(self.font_dirs)?;
        Ok(Config {
            bind: self.bind,
            token,
            allowed_hosts,
            allowed_origins: non_empty(self.allowed_origins),
            font_dirs,
            public_url: public_url.map(|(url, _)| url),
            limits,
        })
    }
}

/// Checks `--public-url` and makes it (the URL without a trailing /, the host name in lower case). Only a
/// full http or https URL is accepted, without user info, query or fragment.
fn public_url(raw: Option<String>) -> Result<Option<(String, String)>, String> {
    let Some(raw) = raw.map(|r| r.trim().to_string()).filter(|r| !r.is_empty()) else {
        return Ok(None);
    };
    let invalid = || format!("--public-url must be an absolute http or https URL without a query, such as https://example.com/artserver: {raw}");
    let uri: http::Uri = raw.parse().map_err(|_| invalid())?;
    let authority = uri
        .authority()
        .filter(|a| matches!(uri.scheme_str(), Some("http" | "https")) && !a.as_str().contains('@') && uri.query().is_none())
        .ok_or_else(invalid)?;
    let host = authority.host().trim_matches(['[', ']']).to_ascii_lowercase();
    Ok(Some((raw.trim_end_matches('/').to_string(), host)))
}

/// Checks the font folders and makes them absolute. A folder that doesn't exist is reported as a startup error.
fn font_dirs(dirs: Vec<PathBuf>) -> Result<Vec<PathBuf>, String> {
    dirs.into_iter()
        .filter(|d| !d.as_os_str().is_empty())
        .map(|d| match std::fs::canonicalize(&d) {
            Ok(path) if path.is_dir() => Ok(path),
            Ok(_) => Err(format!("the font folder {} is not a directory", d.display())),
            Err(e) => Err(format!("cannot use the font folder {}: {e}", d.display())),
        })
        .collect()
}

fn non_empty(values: Vec<String>) -> Vec<String> {
    values.into_iter().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect()
}

/// Is the token long enough, and made only of characters a header can carry?
pub fn validate_token(token: &str) -> Result<(), String> {
    if token.len() < MIN_TOKEN_LEN {
        return Err(format!("the access token must be at least {MIN_TOKEN_LEN} characters"));
    }
    if !token.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("the access token must consist of visible ASCII characters".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(args: &[&str]) -> Cli {
        Cli::parse_from(std::iter::once("artserver").chain(args.iter().copied()))
    }

    #[test]
    fn public_bind_needs_explicit_permission() {
        let err = cli(&["--bind", "0.0.0.0:8471", "--token-file", "/nonexistent"]).resolve().unwrap_err();
        assert!(err.contains("non-loopback"), "{err}");
    }

    #[test]
    fn short_or_non_ascii_tokens_are_refused() {
        assert!(validate_token("short").is_err());
        assert!(validate_token(&"あ".repeat(40)).is_err());
        assert!(validate_token(&"x".repeat(32)).is_ok());
    }

    #[test]
    fn an_empty_host_allowlist_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "t".repeat(40)).unwrap();
        let err = cli(&["--token-file", path.to_str().unwrap(), "--allowed-hosts", ""])
            .resolve()
            .unwrap_err();
        assert!(err.contains("allowed-hosts"), "{err}");
    }

    #[test]
    fn token_is_read_from_the_file_and_trimmed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, format!("{}\n", "t".repeat(40))).unwrap();
        let config = cli(&["--token-file", path.to_str().unwrap()]).resolve().unwrap();
        assert_eq!(config.token, "t".repeat(40));
        assert!(config.bind.ip().is_loopback());
    }

    #[test]
    fn font_folders_must_exist_and_become_absolute() {
        let dir = tempfile::tempdir().unwrap();
        let token = dir.path().join("token");
        std::fs::write(&token, "t".repeat(40)).unwrap();
        let fonts = dir.path().join("fonts");
        std::fs::create_dir(&fonts).unwrap();
        let config = cli(&["--token-file", token.to_str().unwrap(), "--font-dirs", fonts.to_str().unwrap()])
            .resolve()
            .unwrap();
        assert_eq!(config.font_dirs, vec![std::fs::canonicalize(&fonts).unwrap()]);

        let missing = dir.path().join("missing");
        let err = cli(&["--token-file", token.to_str().unwrap(), "--font-dirs", missing.to_str().unwrap()])
            .resolve()
            .unwrap_err();
        assert!(err.contains("font folder"), "{err}");
    }

    #[test]
    fn the_public_url_is_checked_and_its_host_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let token = dir.path().join("token");
        std::fs::write(&token, "t".repeat(40)).unwrap();
        let token = token.to_str().unwrap();
        let config = cli(&["--token-file", token, "--public-url", "https://Art.Example.com/artserver/"])
            .resolve()
            .unwrap();
        assert_eq!(config.public_url.as_deref(), Some("https://Art.Example.com/artserver"));
        assert!(config.allowed_hosts.iter().any(|h| h == "art.example.com"), "{:?}", config.allowed_hosts);
        assert!(cli(&["--token-file", token]).resolve().unwrap().public_url.is_none());
        for bad in [
            "ftp://example.com",
            "/artserver",
            "example.com",
            "https://example.com/?a=1",
            "https://user@example.com",
        ] {
            let err = cli(&["--token-file", token, "--public-url", bad]).resolve().unwrap_err();
            assert!(err.contains("--public-url"), "{bad}: {err}");
        }
    }
}
