//! Per-connection abuse guards (spec NFR6, §6.6, risk R6). The demo is open
//! and auth-free by design, so these are light bounds — a rate cap on ops, a
//! message-size cap, and a cap on how many distinct documents one client IP
//! may hold open at once — not a security boundary. The slug remains the only
//! capability.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Largest WebSocket message the server will accept (spec §6.6). Enforced by
/// the axum upgrade's `max_message_size`; oversized frames close the socket.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;

/// Sustained op rate allowed per connection (spec §6.6). A well-behaved ot.js
/// client keeps one op in flight, so this is only reached by a broken or
/// hostile client.
pub const OPS_PER_SEC: f64 = 100.0;

/// Distinct documents one client IP may hold open concurrently (spec §6.6).
pub const DOCS_PER_IP: usize = 10;

/// A refilling token bucket, one per connection (so it needs no locking).
/// Tokens accrue continuously at `refill_per_sec` up to `capacity`.
#[derive(Debug)]
pub struct TokenBucket {
    capacity: f64,
    tokens: f64,
    refill_per_sec: f64,
    last: Instant,
}

impl TokenBucket {
    /// A bucket with the given sustained rate; capacity equals one second of
    /// tokens, so a burst of `rate` is allowed before throttling begins.
    pub fn new(refill_per_sec: f64) -> Self {
        Self {
            capacity: refill_per_sec,
            tokens: refill_per_sec,
            refill_per_sec,
            last: Instant::now(),
        }
    }

    /// The op-rate bucket (spec §6.6): 100/s, burst 100.
    pub fn ops() -> Self {
        Self::new(OPS_PER_SEC)
    }

    /// Try to spend one token, refilling for elapsed time first. `false` means
    /// the caller is over its rate and should be throttled.
    pub fn try_take(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Caps the number of distinct documents held open per client IP. Connections
/// acquire an [`IpGuard`] on join and drop it on disconnect; the guard's
/// lifetime is the connection's.
#[derive(Debug)]
pub struct IpLimiter {
    docs_per_ip: usize,
    // ip -> (docId -> live connection count for that doc)
    active: Mutex<HashMap<IpAddr, HashMap<String, usize>>>,
}

impl Default for IpLimiter {
    fn default() -> Self {
        Self::new(DOCS_PER_IP)
    }
}

impl IpLimiter {
    pub fn new(docs_per_ip: usize) -> Self {
        Self {
            docs_per_ip,
            active: Mutex::new(HashMap::new()),
        }
    }

    /// Reserve a slot for `ip` connecting to `doc_id`. Returns `None` only when
    /// this would be a *new* document for an IP already at its distinct-doc
    /// cap; additional connections to a document the IP already holds always
    /// succeed. The returned guard releases the slot on drop.
    pub fn try_acquire(self: &Arc<Self>, ip: IpAddr, doc_id: &str) -> Option<IpGuard> {
        let mut active = self.active.lock().expect("ip limiter mutex");
        let docs = active.entry(ip).or_default();
        if !docs.contains_key(doc_id) && docs.len() >= self.docs_per_ip {
            // Drop the empty entry we may have just created for this ip.
            if docs.is_empty() {
                active.remove(&ip);
            }
            return None;
        }
        *docs.entry(doc_id.to_string()).or_insert(0) += 1;
        Some(IpGuard {
            limiter: Arc::clone(self),
            ip,
            doc_id: doc_id.to_string(),
        })
    }
}

/// Releases one IP/document slot when dropped (spec §6.6).
#[derive(Debug)]
pub struct IpGuard {
    limiter: Arc<IpLimiter>,
    ip: IpAddr,
    doc_id: String,
}

impl Drop for IpGuard {
    fn drop(&mut self) {
        let mut active = self.limiter.active.lock().expect("ip limiter mutex");
        if let Some(docs) = active.get_mut(&self.ip) {
            if let Some(count) = docs.get_mut(&self.doc_id) {
                *count -= 1;
                if *count == 0 {
                    docs.remove(&self.doc_id);
                }
            }
            if docs.is_empty() {
                active.remove(&self.ip);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(n: u8) -> IpAddr {
        IpAddr::from([127, 0, 0, n])
    }

    #[test]
    fn bucket_allows_a_full_burst_then_throttles() {
        let mut bucket = TokenBucket::new(100.0);
        // Capacity is 100; the 101st take in the same instant fails.
        for _ in 0..100 {
            assert!(bucket.try_take());
        }
        assert!(!bucket.try_take());
    }

    #[test]
    fn bucket_refills_over_time() {
        let mut bucket = TokenBucket::new(100.0);
        for _ in 0..100 {
            assert!(bucket.try_take());
        }
        assert!(!bucket.try_take());
        // Rewind the clock 100 ms → ~10 tokens accrue at 100/s.
        bucket.last = Instant::now() - std::time::Duration::from_millis(100);
        let mut refilled = 0;
        while bucket.try_take() {
            refilled += 1;
        }
        assert!((9..=11).contains(&refilled), "refilled {refilled}");
    }

    #[test]
    fn limiter_caps_distinct_docs_per_ip() {
        let limiter = Arc::new(IpLimiter::new(2));
        let a = limiter.try_acquire(ip(1), "doc-a").expect("first doc");
        let b = limiter.try_acquire(ip(1), "doc-b").expect("second doc");
        // Third distinct doc for the same IP is rejected.
        assert!(limiter.try_acquire(ip(1), "doc-c").is_none());
        // A different IP is unaffected.
        assert!(limiter.try_acquire(ip(2), "doc-c").is_some());

        // Freeing a slot lets a new doc through.
        drop(a);
        let c = limiter.try_acquire(ip(1), "doc-c").expect("after release");
        drop((b, c));
    }

    #[test]
    fn extra_connections_to_a_held_doc_do_not_consume_slots() {
        let limiter = Arc::new(IpLimiter::new(1));
        let first = limiter.try_acquire(ip(1), "doc-a").expect("first");
        // Same doc, second tab: allowed even though the cap is 1 distinct doc.
        let second = limiter.try_acquire(ip(1), "doc-a").expect("second tab");
        // A different doc is still capped out.
        assert!(limiter.try_acquire(ip(1), "doc-b").is_none());
        drop((first, second));
        // Both released: the IP is clean and a new doc is admitted.
        assert!(limiter.try_acquire(ip(1), "doc-b").is_some());
    }
}
