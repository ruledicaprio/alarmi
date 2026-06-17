//! Per-device circuit breaker. Isolates unreachable controllers so a dead site
//! doesn't burn the poll budget every cycle. Closed -> Open (after N consecutive
//! failures) -> HalfOpen (after cooldown, one probe) -> Closed on success.

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State { Closed, Open, HalfOpen }

#[derive(Debug)]
pub struct Breaker {
    state: State,
    consecutive_failures: u32,
    open_until: Option<Instant>,
    threshold: u32,
    cooldown: Duration,
}

impl Breaker {
    pub fn new(threshold: u32, cooldown: Duration) -> Self {
        Self { state: State::Closed, consecutive_failures: 0, open_until: None, threshold, cooldown }
    }

    /// May we attempt a poll now? Transitions Open->HalfOpen when cooldown elapsed.
    pub fn allow(&mut self, now: Instant) -> bool {
        match self.state {
            State::Closed | State::HalfOpen => true,
            State::Open => {
                if self.open_until.map_or(true, |t| now >= t) {
                    self.state = State::HalfOpen;
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn on_success(&mut self) {
        self.state = State::Closed;
        self.consecutive_failures = 0;
        self.open_until = None;
    }

    pub fn on_failure(&mut self, now: Instant) {
        self.consecutive_failures += 1;
        if self.state == State::HalfOpen || self.consecutive_failures >= self.threshold {
            self.state = State::Open;
            self.open_until = Some(now + self.cooldown);
        }
    }

    #[allow(dead_code)] // used in tests / future status logging
    pub fn state(&self) -> State { self.state }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn b() -> Breaker { Breaker::new(3, Duration::from_secs(300)) }

    #[test]
    fn opens_after_threshold() {
        let mut br = b(); let now = Instant::now();
        assert!(br.allow(now));
        br.on_failure(now); br.on_failure(now);
        assert_eq!(br.state(), State::Closed); // 2 < 3
        br.on_failure(now);
        assert_eq!(br.state(), State::Open);    // 3 >= 3
        assert!(!br.allow(now));                // still in cooldown
    }

    #[test]
    fn halfopen_then_recover() {
        let mut br = b(); let t0 = Instant::now();
        for _ in 0..3 { br.on_failure(t0); }
        let later = t0 + Duration::from_secs(301);
        assert!(br.allow(later));               // -> HalfOpen
        assert_eq!(br.state(), State::HalfOpen);
        br.on_success();
        assert_eq!(br.state(), State::Closed);
    }

    #[test]
    fn halfopen_failure_reopens() {
        let mut br = b(); let t0 = Instant::now();
        for _ in 0..3 { br.on_failure(t0); }
        let later = t0 + Duration::from_secs(301);
        br.allow(later);                        // HalfOpen
        br.on_failure(later);
        assert_eq!(br.state(), State::Open);
    }
}
