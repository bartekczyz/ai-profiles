//! Counting attempts, to slow down guessing: at pairing secrets, and at
//! tokens. Kept in memory; a restart forgets, which costs a guesser nothing
//! they could use (a token is 256 random bits, a pairing secret expires).

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// At most `max` events per `window` for each key.
pub struct RateLimit<K> {
    max: usize,
    window: Duration,
    events: Mutex<HashMap<K, VecDeque<Instant>>>,
}

impl<K: Eq + Hash + Clone> RateLimit<K> {
    pub fn new(max: usize, window: Duration) -> Self {
        RateLimit {
            max,
            window,
            events: Mutex::new(HashMap::new()),
        }
    }

    /// Count an event for `key`, if there is room for it.
    pub fn allow(&self, key: &K) -> bool {
        self.allow_at(key, Instant::now())
    }

    fn allow_at(&self, key: &K, now: Instant) -> bool {
        let Ok(mut events) = self.events.lock() else {
            return false;
        };
        let queue = events.entry(key.clone()).or_default();
        while queue
            .front()
            .is_some_and(|at| now.duration_since(*at) >= self.window)
        {
            queue.pop_front();
        }
        if queue.len() >= self.max {
            return false;
        }
        queue.push_back(now);
        true
    }
}

/// After `max` failures in `window`, refuse a key outright for `lockout`.
pub struct Lockout<K> {
    failures: RateLimit<K>,
    lockout: Duration,
    locked: Mutex<HashMap<K, Instant>>,
}

impl<K: Eq + Hash + Clone> Lockout<K> {
    pub fn new(max: usize, window: Duration, lockout: Duration) -> Self {
        Lockout {
            failures: RateLimit::new(max, window),
            lockout,
            locked: Mutex::new(HashMap::new()),
        }
    }

    pub fn is_locked(&self, key: &K) -> bool {
        self.is_locked_at(key, Instant::now())
    }

    fn is_locked_at(&self, key: &K, now: Instant) -> bool {
        let Ok(mut locked) = self.locked.lock() else {
            return true;
        };
        match locked.get(key) {
            Some(until) if *until > now => true,
            Some(_) => {
                locked.remove(key);
                false
            }
            None => false,
        }
    }

    pub fn fail(&self, key: &K) {
        self.fail_at(key, Instant::now());
    }

    fn fail_at(&self, key: &K, now: Instant) {
        if !self.failures.allow_at(key, now) {
            if let Ok(mut locked) = self.locked.lock() {
                locked.insert(key.clone(), now + self.lockout);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_the_limit_per_window_and_key() {
        let limit = RateLimit::new(2, Duration::from_secs(60));
        let start = Instant::now();
        assert!(limit.allow_at(&"a", start));
        assert!(limit.allow_at(&"a", start));
        assert!(!limit.allow_at(&"a", start));
        assert!(limit.allow_at(&"b", start));
        assert!(limit.allow_at(&"a", start + Duration::from_secs(61)));
    }

    #[test]
    fn locks_out_after_too_many_failures_then_lets_go() {
        let lockout = Lockout::new(2, Duration::from_secs(60), Duration::from_secs(300));
        let start = Instant::now();
        lockout.fail_at(&"ip", start);
        lockout.fail_at(&"ip", start);
        assert!(!lockout.is_locked_at(&"ip", start));
        lockout.fail_at(&"ip", start);
        assert!(lockout.is_locked_at(&"ip", start + Duration::from_secs(10)));
        assert!(!lockout.is_locked_at(&"other", start));
        assert!(!lockout.is_locked_at(&"ip", start + Duration::from_secs(301)));
    }
}
