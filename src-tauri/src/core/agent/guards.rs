//! Concurrent spawn guards for multi-agent sessions.
//!
//! Limits the total number of sub-agents per user session and manages
//! a pool of agent nicknames with automatic recycling.

use crate::protocol::error::{CodexError, ErrorCode};
use rand::prelude::IndexedRandom;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Session-scoped guard that limits concurrent sub-agent spawns and
/// manages the agent nickname pool.
///
/// Shared by all agents in the same user session.
#[derive(Default)]
pub struct Guards {
    active_agents: Mutex<ActiveAgents>,
    total_count: AtomicUsize,
}

#[derive(Default)]
struct ActiveAgents {
    threads_set: HashSet<String>,
    thread_agent_nicknames: HashMap<String, String>,
    used_agent_nicknames: HashSet<String>,
    nickname_reset_count: usize,
}

/// Depth helpers for nested thread spawns.
pub fn next_thread_spawn_depth(current_depth: usize) -> usize {
    current_depth.saturating_add(1)
}

pub fn exceeds_thread_spawn_depth_limit(depth: usize, max_depth: usize) -> bool {
    depth > max_depth
}

impl Guards {
    /// Try to reserve a spawn slot. Returns a [`SpawnReservation`] RAII guard
    /// that releases the slot on drop if not committed.
    pub fn reserve_spawn_slot(
        self: &Arc<Self>,
        max_threads: Option<usize>,
    ) -> Result<SpawnReservation, CodexError> {
        if let Some(max_threads) = max_threads {
            if !self.try_increment_spawned(max_threads) {
                return Err(CodexError::new(
                    ErrorCode::SessionError,
                    format!("Agent limit reached: max {max_threads} concurrent agents"),
                ));
            }
        } else {
            self.total_count.fetch_add(1, Ordering::AcqRel);
        }
        Ok(SpawnReservation {
            state: Arc::clone(self),
            active: true,
            reserved_agent_nickname: None,
        })
    }

    /// Release a previously committed thread slot.
    pub fn release_spawned_thread(&self, thread_id: &str) {
        let removed = {
            let mut active = self
                .active_agents
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let removed = active.threads_set.remove(thread_id);
            active.thread_agent_nicknames.remove(thread_id);
            removed
        };
        if removed {
            self.total_count.fetch_sub(1, Ordering::AcqRel);
        }
    }

    fn register_spawned_thread(&self, thread_id: &str, agent_nickname: Option<String>) {
        let mut active = self
            .active_agents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        active.threads_set.insert(thread_id.to_string());
        if let Some(nickname) = agent_nickname {
            active.used_agent_nicknames.insert(nickname.clone());
            active
                .thread_agent_nicknames
                .insert(thread_id.to_string(), nickname);
        }
    }

    fn reserve_agent_nickname(&self, names: &[&str], preferred: Option<&str>) -> Option<String> {
        let mut active = self
            .active_agents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let nickname = if let Some(preferred) = preferred {
            preferred.to_string()
        } else {
            if names.is_empty() {
                return None;
            }
            let available: Vec<&str> = names
                .iter()
                .copied()
                .filter(|n| !active.used_agent_nicknames.contains(*n))
                .collect();
            if let Some(name) = available.choose(&mut rand::rng()) {
                (*name).to_string()
            } else {
                // Pool exhausted — reset and pick again.
                active.used_agent_nicknames.clear();
                active.nickname_reset_count += 1;
                names.choose(&mut rand::rng())?.to_string()
            }
        };

        active.used_agent_nicknames.insert(nickname.clone());
        Some(nickname)
    }

    fn try_increment_spawned(&self, max_threads: usize) -> bool {
        let mut current = self.total_count.load(Ordering::Acquire);
        loop {
            if current >= max_threads {
                return false;
            }
            match self.total_count.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(updated) => current = updated,
            }
        }
    }

    /// Visible for testing: how many times the nickname pool was reset.
    #[cfg(test)]
    pub(crate) fn nickname_reset_count(&self) -> usize {
        self.active_agents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .nickname_reset_count
    }
}

/// RAII reservation for a spawn slot. If dropped without calling [`commit`],
/// the slot is automatically released.
pub struct SpawnReservation {
    state: Arc<Guards>,
    active: bool,
    reserved_agent_nickname: Option<String>,
}

impl std::fmt::Debug for SpawnReservation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpawnReservation")
            .field("active", &self.active)
            .field("reserved_agent_nickname", &self.reserved_agent_nickname)
            .finish_non_exhaustive()
    }
}

impl SpawnReservation {
    /// Reserve a nickname from the pool.
    pub fn reserve_agent_nickname(&mut self, names: &[&str]) -> Result<String, CodexError> {
        self.reserve_agent_nickname_with_preference(names, None)
    }

    /// Reserve a nickname, optionally preferring a specific one.
    pub fn reserve_agent_nickname_with_preference(
        &mut self,
        names: &[&str],
        preferred: Option<&str>,
    ) -> Result<String, CodexError> {
        let nickname = self
            .state
            .reserve_agent_nickname(names, preferred)
            .ok_or_else(|| {
                CodexError::new(
                    ErrorCode::SessionError,
                    "No available agent nicknames".to_string(),
                )
            })?;
        self.reserved_agent_nickname = Some(nickname.clone());
        Ok(nickname)
    }

    /// Commit the reservation, registering the thread as active.
    pub fn commit(self, thread_id: &str) {
        self.commit_with_agent_nickname(thread_id, None);
    }

    /// Commit with an explicit nickname override.
    pub fn commit_with_agent_nickname(mut self, thread_id: &str, agent_nickname: Option<String>) {
        let nickname = self.reserved_agent_nickname.take().or(agent_nickname);
        self.state.register_spawned_thread(thread_id, nickname);
        self.active = false;
    }
}

impl Drop for SpawnReservation {
    fn drop(&mut self) {
        if self.active {
            self.state.total_count.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservation_drop_releases_slot() {
        let guards = Arc::new(Guards::default());
        let reservation = guards.reserve_spawn_slot(Some(1)).expect("reserve slot");
        drop(reservation);

        let reservation = guards.reserve_spawn_slot(Some(1)).expect("slot released");
        drop(reservation);
    }

    #[test]
    fn commit_holds_slot_until_release() {
        let guards = Arc::new(Guards::default());
        let reservation = guards.reserve_spawn_slot(Some(1)).expect("reserve slot");
        let thread_id = uuid::Uuid::new_v4().to_string();
        reservation.commit(&thread_id);

        let err = guards.reserve_spawn_slot(Some(1)).unwrap_err();
        assert_eq!(err.code, ErrorCode::SessionError);

        guards.release_spawned_thread(&thread_id);
        let reservation = guards.reserve_spawn_slot(Some(1)).expect("slot released");
        drop(reservation);
    }

    #[test]
    fn release_ignores_unknown_thread_id() {
        let guards = Arc::new(Guards::default());
        let reservation = guards.reserve_spawn_slot(Some(1)).expect("reserve slot");
        let thread_id = uuid::Uuid::new_v4().to_string();
        reservation.commit(&thread_id);

        guards.release_spawned_thread(&uuid::Uuid::new_v4().to_string());

        // Slot should still be held.
        let err = guards.reserve_spawn_slot(Some(1)).unwrap_err();
        assert_eq!(err.code, ErrorCode::SessionError);

        guards.release_spawned_thread(&thread_id);
        let reservation = guards.reserve_spawn_slot(Some(1)).expect("slot released");
        drop(reservation);
    }

    #[test]
    fn release_is_idempotent_for_registered_threads() {
        let guards = Arc::new(Guards::default());
        let reservation = guards.reserve_spawn_slot(Some(1)).expect("reserve slot");
        let first_id = uuid::Uuid::new_v4().to_string();
        reservation.commit(&first_id);

        guards.release_spawned_thread(&first_id);

        let reservation = guards.reserve_spawn_slot(Some(1)).expect("slot reused");
        let second_id = uuid::Uuid::new_v4().to_string();
        reservation.commit(&second_id);

        // Double-release of first_id should be a no-op.
        guards.release_spawned_thread(&first_id);
        let err = guards.reserve_spawn_slot(Some(1)).unwrap_err();
        assert_eq!(err.code, ErrorCode::SessionError);

        guards.release_spawned_thread(&second_id);
        let reservation = guards.reserve_spawn_slot(Some(1)).expect("slot released");
        drop(reservation);
    }

    #[test]
    fn failed_spawn_keeps_nickname_marked_used() {
        let guards = Arc::new(Guards::default());
        let mut reservation = guards.reserve_spawn_slot(None).expect("reserve slot");
        let name = reservation
            .reserve_agent_nickname(&["alpha"])
            .expect("reserve name");
        assert_eq!(name, "alpha");
        drop(reservation);

        let mut reservation = guards.reserve_spawn_slot(None).expect("reserve slot");
        let name = reservation
            .reserve_agent_nickname(&["alpha", "beta"])
            .expect("unused name preferred");
        assert_eq!(name, "beta");
    }

    #[test]
    fn agent_nickname_resets_used_pool_when_exhausted() {
        let guards = Arc::new(Guards::default());
        let mut first = guards.reserve_spawn_slot(None).expect("reserve first");
        let first_name = first.reserve_agent_nickname(&["alpha"]).expect("reserve");
        let first_id = uuid::Uuid::new_v4().to_string();
        first.commit(&first_id);
        assert_eq!(first_name, "alpha");

        let mut second = guards.reserve_spawn_slot(None).expect("reserve second");
        let second_name = second
            .reserve_agent_nickname(&["alpha"])
            .expect("reused after pool reset");
        assert_eq!(second_name, "alpha");
        assert_eq!(guards.nickname_reset_count(), 1);
    }

    #[test]
    fn released_nickname_stays_used_until_pool_reset() {
        let guards = Arc::new(Guards::default());

        let mut first = guards.reserve_spawn_slot(None).expect("reserve first");
        let first_name = first.reserve_agent_nickname(&["alpha"]).expect("reserve");
        let first_id = uuid::Uuid::new_v4().to_string();
        first.commit(&first_id);
        assert_eq!(first_name, "alpha");

        guards.release_spawned_thread(&first_id);

        let mut second = guards.reserve_spawn_slot(None).expect("reserve second");
        let second_name = second
            .reserve_agent_nickname(&["alpha", "beta"])
            .expect("released name still marked used");
        assert_eq!(second_name, "beta");
        let second_id = uuid::Uuid::new_v4().to_string();
        second.commit(&second_id);
        guards.release_spawned_thread(&second_id);

        let mut third = guards.reserve_spawn_slot(None).expect("reserve third");
        let third_name = third
            .reserve_agent_nickname(&["alpha", "beta"])
            .expect("pool reset permits duplicate");
        assert!(third_name == "alpha" || third_name == "beta");
        assert_eq!(guards.nickname_reset_count(), 1);
    }

    #[test]
    fn depth_helpers() {
        assert_eq!(next_thread_spawn_depth(0), 1);
        assert_eq!(next_thread_spawn_depth(5), 6);
        assert!(exceeds_thread_spawn_depth_limit(3, 2));
        assert!(!exceeds_thread_spawn_depth_limit(2, 2));
        assert!(!exceeds_thread_spawn_depth_limit(1, 2));
    }
}
