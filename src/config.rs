//! 起動設定。CLI 引数と環境変数から読み、検証済みの [`Config`] にする。

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;

/// アクセストークンの最小長。推測されにくい長さを求める。
const MIN_TOKEN_LEN: usize = 32;

/// コマンドライン引数。値の多くは環境変数でも渡せる。
#[derive(Debug, Parser)]
#[command(name = "artserver", version, about = "Headless vector graphics server driven over MCP (Streamable HTTP)")]
pub struct Cli {
    /// 待ち受けアドレス。既定はループバックのみ
    #[arg(long, env = "ARTSERVER_BIND", default_value = "127.0.0.1:8471")]
    pub bind: SocketAddr,

    /// ループバック以外での待ち受けを許可する（TLS を終端するリバースプロキシの背後に置く場合だけ使う）
    #[arg(long)]
    pub allow_public_bind: bool,

    /// アクセストークンを書いたファイル。未指定なら環境変数 ARTSERVER_TOKEN を使う
    #[arg(long, env = "ARTSERVER_TOKEN_FILE")]
    pub token_file: Option<PathBuf>,

    /// 受け付ける Host ヘッダ（カンマ区切り）
    #[arg(long, env = "ARTSERVER_ALLOWED_HOSTS", value_delimiter = ',', default_value = "localhost,127.0.0.1,::1")]
    pub allowed_hosts: Vec<String>,

    /// 受け付ける Origin（カンマ区切り）。空なら Origin ヘッダの付いた要求をすべて拒否する
    #[arg(long, env = "ARTSERVER_ALLOWED_ORIGINS", value_delimiter = ',')]
    pub allowed_origins: Vec<String>,

    /// 同時に持てる作業セッション数
    #[arg(long, env = "ARTSERVER_MAX_SESSIONS", default_value_t = Limits::default().max_sessions)]
    pub max_sessions: usize,

    /// 使われない作業セッションを破棄するまでの秒数
    #[arg(long, env = "ARTSERVER_SESSION_IDLE_SECS", default_value_t = Limits::default().session_idle.as_secs())]
    pub session_idle_secs: u64,

    /// 1回のツール呼び出しの制限時間（秒）。超えるとエンジンにキャンセルを求める
    #[arg(long, env = "ARTSERVER_CALL_TIMEOUT_SECS", default_value_t = Limits::default().call_timeout.as_secs())]
    pub call_timeout_secs: u64,

    /// 要求本文の上限（バイト）
    #[arg(long, env = "ARTSERVER_MAX_BODY_BYTES", default_value_t = Limits::default().max_body_bytes)]
    pub max_body_bytes: usize,

    /// 1つの作業セッションで同時に開ける文書数
    #[arg(long, env = "ARTSERVER_MAX_DOCUMENTS", default_value_t = Limits::default().max_documents)]
    pub max_documents: usize,

    /// 文書1つあたりの Undo 履歴の上限（MiB）
    #[arg(long, env = "ARTSERVER_HISTORY_MIB", default_value_t = Limits::default().history_bytes / MIB)]
    pub history_mib: usize,

    /// 書き出し1回あたりの描画スレッド数（0: 呼び出したスレッドだけで描く）
    #[arg(long, env = "ARTSERVER_RENDER_THREADS", default_value_t = Limits::default().render_threads)]
    pub render_threads: u16,

    /// OS に入っているフォントに加えて使うフォントのフォルダ（カンマ区切り）。組織で用意した
    /// フォントを置く用途（docs/design-policy.md 1.1節）
    #[arg(long, env = "ARTSERVER_FONT_DIRS", value_delimiter = ',')]
    pub font_dirs: Vec<PathBuf>,
}

const MIB: usize = 1024 * 1024;

/// 検証済みの設定。
#[derive(Clone, Debug)]
pub struct Config {
    pub bind: SocketAddr,
    pub token: String,
    pub allowed_hosts: Vec<String>,
    pub allowed_origins: Vec<String>,
    /// 追加で読み込むフォントのフォルダ（絶対パス）
    pub font_dirs: Vec<PathBuf>,
    pub limits: Limits,
}

/// 資源の上限。
#[derive(Clone, Debug)]
pub struct Limits {
    pub max_sessions: usize,
    pub session_idle: Duration,
    pub call_timeout: Duration,
    pub max_body_bytes: usize,
    /// 1つの作業セッションで実行待ちにできる要求の数。超えると「混雑中」を返す
    pub queue_depth: usize,
    pub max_documents: usize,
    pub history_bytes: usize,
    pub render_threads: u16,
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
        }
    }
}

impl Cli {
    /// 引数を検証し、トークンを読み込んで [`Config`] にする。
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
        // rmcp は Host の許可リストが空だとすべての Host を通すので、空は認めない
        let allowed_hosts = non_empty(self.allowed_hosts);
        if allowed_hosts.is_empty() {
            return Err("--allowed-hosts must name at least one host".to_string());
        }
        let limits = Limits {
            max_sessions: self.max_sessions.max(1),
            session_idle: Duration::from_secs(self.session_idle_secs.max(1)),
            call_timeout: Duration::from_secs(self.call_timeout_secs.max(1)),
            max_body_bytes: self.max_body_bytes,
            max_documents: self.max_documents.max(1),
            history_bytes: self.history_mib.saturating_mul(MIB),
            render_threads: self.render_threads,
            ..Limits::default()
        };
        let font_dirs = font_dirs(self.font_dirs)?;
        Ok(Config {
            bind: self.bind,
            token,
            allowed_hosts,
            allowed_origins: non_empty(self.allowed_origins),
            font_dirs,
            limits,
        })
    }
}

/// フォントのフォルダを確かめ、絶対パスにする。存在しないフォルダは起動時の誤りとして知らせる。
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

/// トークンが短すぎないか、ヘッダに載せられる文字だけか。
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
}
