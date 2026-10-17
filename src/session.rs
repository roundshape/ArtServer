//! 作業セッションの表（ハンドル ↔ エンジン実行器）。
//!
//! MCP 2026-07-28 版ではプロトコル上のセッションがなくなったため、作業の状態は ArtServer が
//! 発行するハンドルで識別し、ツールの引数で受け渡す（docs/design-policy.md 4.5節）。

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use vectorcraft_engine::file_access::AutomationRoots;

use crate::auth::Principal;
use crate::engine_host::{EngineHost, EngineLimits};

/// ハンドルの乱数のバイト数。
const HANDLE_BYTES: usize = 32;

/// 作業セッションの操作の失敗。
#[derive(Debug)]
pub enum SessionError {
    /// 存在しない、期限切れ、または別の主体のハンドル（区別して見せない）
    Unknown,
    TooMany(usize),
    Internal(String),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionError::Unknown => f.write_str("unknown or expired session; create one with session_create"),
            SessionError::TooMany(n) => write!(f, "the server already holds {n} sessions; close one with session_close"),
            SessionError::Internal(m) => write!(f, "internal error: {m}"),
        }
    }
}

struct Entry {
    owner: Principal,
    host: Arc<EngineHost>,
    last_used: Instant,
}

/// 作業セッションの表。
pub struct Sessions {
    entries: Mutex<HashMap<String, Entry>>,
    max_sessions: usize,
    idle: Duration,
    engine_limits: EngineLimits,
    roots: Arc<AutomationRoots>,
}

impl Sessions {
    pub fn new(max_sessions: usize, idle: Duration, engine_limits: EngineLimits, roots: Arc<AutomationRoots>) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            max_sessions,
            idle,
            engine_limits,
            roots,
        }
    }

    fn entries(&self) -> MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 新しい作業セッションを作り、そのハンドルを返す。
    pub fn create(&self, owner: &Principal) -> Result<String, SessionError> {
        self.purge_idle();
        let mut entries = self.entries();
        if entries.len() >= self.max_sessions {
            return Err(SessionError::TooMany(self.max_sessions));
        }
        let handle = new_handle()?;
        let host = EngineHost::spawn(self.engine_limits.clone(), self.roots.clone()).map_err(|e| SessionError::Internal(e.to_string()))?;
        entries.insert(
            handle.clone(),
            Entry {
                owner: owner.clone(),
                host: Arc::new(host),
                last_used: Instant::now(),
            },
        );
        Ok(handle)
    }

    /// `owner` のハンドル `handle` の実行器。
    pub fn get(&self, handle: &str, owner: &Principal) -> Result<Arc<EngineHost>, SessionError> {
        let mut entries = self.entries();
        let entry = entries.get_mut(handle).filter(|e| &e.owner == owner).ok_or(SessionError::Unknown)?;
        entry.last_used = Instant::now();
        Ok(entry.host.clone())
    }

    /// 作業セッションを破棄する。実行中の要求が終わると実行スレッドも終わる。
    pub fn close(&self, handle: &str, owner: &Principal) -> Result<(), SessionError> {
        let mut entries = self.entries();
        if !entries.get(handle).is_some_and(|e| &e.owner == owner) {
            return Err(SessionError::Unknown);
        }
        entries.remove(handle);
        Ok(())
    }

    /// 使われないまま期限を過ぎた作業セッションを破棄し、その数を返す。
    pub fn purge_idle(&self) -> usize {
        let mut entries = self.entries();
        let before = entries.len();
        entries.retain(|_, e| e.last_used.elapsed() < self.idle);
        before - entries.len()
    }

    pub fn len(&self) -> usize {
        self.entries().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn new_handle() -> Result<String, SessionError> {
    let mut bytes = [0u8; HANDLE_BYTES];
    getrandom::fill(&mut bytes).map_err(|e| SessionError::Internal(format!("no randomness: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sessions(dir: &tempfile::TempDir, max: usize, idle: Duration) -> Sessions {
        let roots = AutomationRoots::new(Some(dir.path()), None).unwrap().unwrap();
        Sessions::new(
            max,
            idle,
            EngineLimits {
                queue_depth: 2,
                max_documents: 2,
                history_bytes: 1 << 20,
            },
            roots,
        )
    }

    #[test]
    fn handles_are_bound_to_their_owner() {
        let dir = tempfile::tempdir().unwrap();
        let s = sessions(&dir, 4, Duration::from_secs(60));
        let alice = Principal::new("alice");
        let bob = Principal::new("bob");
        let h = s.create(&alice).unwrap();
        assert_eq!(h.len(), HANDLE_BYTES * 2);
        assert!(s.get(&h, &alice).is_ok());
        assert!(matches!(s.get(&h, &bob), Err(SessionError::Unknown)));
        assert!(matches!(s.close(&h, &bob), Err(SessionError::Unknown)));
        s.close(&h, &alice).unwrap();
        assert!(matches!(s.get(&h, &alice), Err(SessionError::Unknown)));
    }

    #[test]
    fn the_number_of_sessions_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        let s = sessions(&dir, 1, Duration::from_secs(60));
        let p = Principal::new("p");
        s.create(&p).unwrap();
        assert!(matches!(s.create(&p), Err(SessionError::TooMany(1))));
    }

    #[test]
    fn idle_sessions_are_purged() {
        let dir = tempfile::tempdir().unwrap();
        let s = sessions(&dir, 2, Duration::from_millis(1));
        let p = Principal::new("p");
        let h = s.create(&p).unwrap();
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(s.purge_idle(), 1);
        assert!(matches!(s.get(&h, &p), Err(SessionError::Unknown)));
    }
}
