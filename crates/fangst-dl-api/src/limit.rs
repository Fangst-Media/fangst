//! Backoff for password attempts, per client address.
//!
//! An attempt counts as failed the moment it starts and is forgiven when it succeeds, so firing
//! many attempts in parallel gets no more guesses than sending them one by one.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv6Addr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginLimits {
    /// Failed attempts allowed before the first wait.
    pub free_attempts: u32,
    /// The first wait; each further failure doubles it.
    pub base_delay: Duration,
    pub max_delay: Duration,
    /// A client with no failures for this long starts over.
    pub forget_after: Duration,
    /// Upper bound on clients remembered at once, so a flood of addresses can't exhaust memory.
    pub max_clients: usize,
}

impl Default for LoginLimits {
    fn default() -> Self {
        Self {
            // Enough for a few typos; after that 1 s, 2 s, 4 s ... up to 15 minutes per guess.
            free_attempts: 5,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_mins(15),
            forget_after: Duration::from_hours(1),
            max_clients: 10_000,
        }
    }
}

struct Entry {
    /// Every failure from this client, whatever account it was against; this sets the wait.
    failures: u32,
    /// The same failures per account name, so a success forgives only the ones against that
    /// account. Signing in to your own account must not reset guesses against someone else's.
    by_account: HashMap<String, u32>,
    blocked_until: Instant,
    last: Instant,
}

/// Account names tracked per client. Failures beyond this still count toward the wait; they just
/// can't be forgiven, which only matters to a client trying many names.
const MAX_ACCOUNTS_PER_CLIENT: usize = 32;

fn account_key(account: &str) -> String {
    account.trim().to_lowercase()
}

pub(crate) struct Limiter {
    limits: LoginLimits,
    clients: Mutex<HashMap<IpAddr, Entry>>,
}

impl Limiter {
    pub(crate) fn new(limits: LoginLimits) -> Self {
        Self {
            limits,
            clients: Mutex::new(HashMap::new()),
        }
    }

    /// Records an attempt from `client` against `account`, or says how long it must wait first.
    pub(crate) fn attempt(
        &self,
        client: IpAddr,
        account: &str,
        now: Instant,
    ) -> Result<(), Duration> {
        let client = bucket(client);
        let mut clients = self.clients.lock().expect("limiter lock");
        if !clients.contains_key(&client) && clients.len() >= self.limits.max_clients {
            self.evict(&mut clients, now);
        }
        let entry = clients.entry(client).or_insert(Entry {
            failures: 0,
            by_account: HashMap::new(),
            blocked_until: now,
            last: now,
        });
        if now.duration_since(entry.last) >= self.limits.forget_after {
            entry.failures = 0;
            entry.by_account.clear();
            entry.blocked_until = now;
        }
        if entry.blocked_until > now {
            return Err(entry.blocked_until - now);
        }
        entry.failures = entry.failures.saturating_add(1);
        let account = account_key(account);
        if let Some(count) = entry.by_account.get_mut(&account) {
            *count = count.saturating_add(1);
        } else if entry.by_account.len() < MAX_ACCOUNTS_PER_CLIENT {
            entry.by_account.insert(account, 1);
        }
        entry.last = now;
        if let Some(over) = entry.failures.checked_sub(self.limits.free_attempts)
            && over > 0
        {
            entry.blocked_until = now + self.delay(over);
        }
        Ok(())
    }

    /// The password for `account` was right: failures against that account are forgiven.
    pub(crate) fn succeeded(&self, client: IpAddr, account: &str, now: Instant) {
        self.forgive(client, account, u32::MAX, now);
    }

    /// The attempt never got checked (the server was busy), so it doesn't count.
    pub(crate) fn not_checked(&self, client: IpAddr, account: &str, now: Instant) {
        self.forgive(client, account, 1, now);
    }

    fn forgive(&self, client: IpAddr, account: &str, up_to: u32, now: Instant) {
        let client = bucket(client);
        let mut clients = self.clients.lock().expect("limiter lock");
        let Some(entry) = clients.get_mut(&client) else {
            return;
        };
        let account = account_key(account);
        let Some(count) = entry.by_account.get_mut(&account) else {
            return;
        };
        let forgiven = (*count).min(up_to);
        *count -= forgiven;
        if *count == 0 {
            entry.by_account.remove(&account);
        }
        entry.failures = entry.failures.saturating_sub(forgiven);
        if entry.failures <= self.limits.free_attempts {
            entry.blocked_until = now;
        }
        if entry.failures == 0 {
            clients.remove(&client);
        }
    }

    fn delay(&self, over: u32) -> Duration {
        let factor = 2u32.saturating_pow(over - 1);
        self.limits
            .base_delay
            .saturating_mul(factor)
            .min(self.limits.max_delay)
    }

    fn evict(&self, clients: &mut HashMap<IpAddr, Entry>, now: Instant) {
        clients.retain(|_, e| {
            now.duration_since(e.last) < self.limits.forget_after || e.blocked_until > now
        });
        if clients.len() >= self.limits.max_clients
            && let Some(oldest) = clients
                .iter()
                .min_by_key(|(_, e)| e.last)
                .map(|(ip, _)| *ip)
        {
            clients.remove(&oldest);
        }
    }
}

/// One IPv6 host usually owns a whole /64, so it counts as one client.
fn bucket(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                IpAddr::V4(v4)
            } else {
                let s = v6.segments();
                IpAddr::V6(Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter() -> Limiter {
        Limiter::new(LoginLimits {
            free_attempts: 2,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(4),
            forget_after: Duration::from_secs(100),
            max_clients: 3,
        })
    }

    const A: IpAddr = IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 1));
    const B: IpAddr = IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 2));

    #[test]
    fn waits_double_after_the_free_attempts_up_to_the_cap() {
        let l = limiter();
        let t = Instant::now();
        assert!(l.attempt(A, "u", t).is_ok());
        assert!(l.attempt(A, "u", t).is_ok());
        assert!(
            l.attempt(A, "u", t).is_ok(),
            "third attempt runs, then waits"
        );
        assert_eq!(l.attempt(A, "u", t), Err(Duration::from_secs(1)));
        let t = t + Duration::from_secs(1);
        assert!(l.attempt(A, "u", t).is_ok());
        assert_eq!(l.attempt(A, "u", t), Err(Duration::from_secs(2)));
        let t = t + Duration::from_secs(2);
        assert!(l.attempt(A, "u", t).is_ok());
        let t = t + Duration::from_secs(4);
        assert!(l.attempt(A, "u", t).is_ok());
        assert_eq!(l.attempt(A, "u", t), Err(Duration::from_secs(4)), "capped");
        assert!(
            l.attempt(B, "u", t).is_ok(),
            "other clients are not affected"
        );
    }

    #[test]
    fn success_and_time_forgive() {
        let l = limiter();
        let t = Instant::now();
        for _ in 0..3 {
            l.attempt(A, "u", t).unwrap();
        }
        assert!(l.attempt(A, "u", t).is_err());
        l.succeeded(A, "u", t);
        assert!(l.attempt(A, "u", t).is_ok());

        for _ in 0..3 {
            l.attempt(B, "u", t).unwrap();
        }
        assert!(l.attempt(B, "u", t).is_err());
        assert!(l.attempt(B, "u", t + Duration::from_secs(100)).is_ok());
    }

    #[test]
    fn signing_in_to_one_account_does_not_forgive_guesses_at_another() {
        let l = limiter();
        let t = Instant::now();
        l.attempt(A, "victim", t).unwrap();
        l.attempt(A, "victim", t).unwrap();
        l.attempt(A, "mine", t).unwrap();
        l.succeeded(A, "MINE ", t);
        l.attempt(A, "victim", t).unwrap();
        assert!(
            l.attempt(A, "victim", t).is_err(),
            "the two victim guesses still count"
        );
    }

    #[test]
    fn unchecked_attempts_do_not_count() {
        let l = limiter();
        let t = Instant::now();
        for _ in 0..3 {
            l.attempt(A, "u", t).unwrap();
        }
        l.not_checked(A, "u", t);
        assert!(l.attempt(A, "u", t).is_ok(), "back under the free attempts");
    }

    #[test]
    fn ipv6_counts_per_64_and_memory_is_bounded() {
        let l = limiter();
        let t = Instant::now();
        let one: IpAddr = "2001:db8::1".parse().unwrap();
        let same_net: IpAddr = "2001:db8::ffff".parse().unwrap();
        for _ in 0..3 {
            l.attempt(one, "u", t).unwrap();
        }
        assert!(l.attempt(same_net, "u", t).is_err());

        for i in 0..10u8 {
            let _ = l.attempt(IpAddr::V4(std::net::Ipv4Addr::new(10, 1, 0, i)), "u", t);
        }
        assert!(l.clients.lock().unwrap().len() <= 3);
    }
}
