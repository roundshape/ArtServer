//! The table of working sessions (handle ↔ engine executor).
//!
//! MCP 2026-07-28 removed sessions from the protocol, so the working state is identified by a handle that
//! ArtServer issues and passed in tool arguments (docs/design-policy.md section 4.5).

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use vectorcraft_engine::file_access::AutomationRoots;

use crate::auth::Principal;
use crate::engine_host::{EngineHost, EngineLimits, RunSlots};

/// Number of random bytes in a handle.
const HANDLE_BYTES: usize = 32;

/// A failed operation on working sessions.
#[derive(Debug)]
pub enum SessionError {
    /// A handle that doesn't exist, has expired or belongs to another principal (not told apart)
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

/// The table of working sessions.
pub struct Sessions {
    entries: Mutex<HashMap<String, Entry>>,
    max_sessions: usize,
    idle: Duration,
    engine_limits: EngineLimits,
    roots: Arc<AutomationRoots>,
    /// The jobs allowed to run at once, shared by every session's executor
    slots: Arc<RunSlots>,
}

impl Sessions {
    pub fn new(max_sessions: usize, idle: Duration, engine_limits: EngineLimits, roots: Arc<AutomationRoots>, slots: Arc<RunSlots>) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            max_sessions,
            idle,
            engine_limits,
            roots,
            slots,
        }
    }

    fn entries(&self) -> MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Creates a new working session and returns its handle.
    pub fn create(&self, owner: &Principal) -> Result<String, SessionError> {
        self.purge_idle();
        let mut entries = self.entries();
        if entries.len() >= self.max_sessions {
            return Err(SessionError::TooMany(self.max_sessions));
        }
        let handle = new_handle()?;
        let host = EngineHost::spawn(self.engine_limits.clone(), self.roots.clone(), self.slots.clone())
            .map_err(|e| SessionError::Internal(e.to_string()))?;
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

    /// The executor of `owner`'s handle `handle`.
    pub fn get(&self, handle: &str, owner: &Principal) -> Result<Arc<EngineHost>, SessionError> {
        let mut entries = self.entries();
        let entry = entries.get_mut(handle).filter(|e| &e.owner == owner).ok_or(SessionError::Unknown)?;
        entry.last_used = Instant::now();
        Ok(entry.host.clone())
    }

    /// Discards a working session. Its execution thread ends once the requests in progress finish.
    pub fn close(&self, handle: &str, owner: &Principal) -> Result<(), SessionError> {
        let mut entries = self.entries();
        if !entries.get(handle).is_some_and(|e| &e.owner == owner) {
            return Err(SessionError::Unknown);
        }
        entries.remove(handle);
        Ok(())
    }

    /// Discards the working sessions left unused past the idle time and returns how many.
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
            Arc::new(RunSlots::new(2)),
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
