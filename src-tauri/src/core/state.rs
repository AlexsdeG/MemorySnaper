use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessingStateSnapshot {
    pub is_paused: bool,
    pub is_stopped: bool,
    /// True only after `resume_processing_session` has been called in this
    /// process lifetime. Stays false after a cold restart, allowing
    /// `get_processing_session_overview` to distinguish "nothing running" from
    /// "actively running but not paused/stopped".
    pub is_session_active: bool,
}

static IS_PAUSED: AtomicBool = AtomicBool::new(false);
static IS_STOPPED: AtomicBool = AtomicBool::new(false);
static IS_SESSION_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn snapshot() -> ProcessingStateSnapshot {
    ProcessingStateSnapshot {
        is_paused: IS_PAUSED.load(Ordering::SeqCst),
        is_stopped: IS_STOPPED.load(Ordering::SeqCst),
        is_session_active: IS_SESSION_ACTIVE.load(Ordering::SeqCst),
    }
}

pub fn set_paused(value: bool) {
    IS_PAUSED.store(value, Ordering::SeqCst);
}

pub fn set_stopped(value: bool) {
    IS_STOPPED.store(value, Ordering::SeqCst);
}

pub fn set_session_active(value: bool) {
    IS_SESSION_ACTIVE.store(value, Ordering::SeqCst);
}

pub fn reset() {
    set_paused(false);
    set_stopped(false);
    set_session_active(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reflects_atomic_updates() {
        reset();
        assert_eq!(
            snapshot(),
            ProcessingStateSnapshot {
                is_paused: false,
                is_stopped: false,
                is_session_active: false,
            }
        );

        set_paused(true);
        assert_eq!(
            snapshot(),
            ProcessingStateSnapshot {
                is_paused: true,
                is_stopped: false,
                is_session_active: false,
            }
        );

        set_stopped(true);
        assert_eq!(
            snapshot(),
            ProcessingStateSnapshot {
                is_paused: true,
                is_stopped: true,
                is_session_active: false,
            }
        );

        set_session_active(true);
        assert_eq!(
            snapshot(),
            ProcessingStateSnapshot {
                is_paused: true,
                is_stopped: true,
                is_session_active: true,
            }
        );

        reset();
        assert_eq!(
            snapshot(),
            ProcessingStateSnapshot {
                is_paused: false,
                is_stopped: false,
                is_session_active: false,
            }
        );
    }
}
