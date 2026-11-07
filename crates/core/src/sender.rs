//! Repeated sending of a block of data (Autosend, Send file),
//! see `docs/spec/send-dialog.md`.
//!
//! [`SendJob`] is a small state machine with no I/O: the caller asks it
//! when to queue the next repetition ([`SendJob::poll`]) and reports bytes
//! the port has sent ([`SendJob::on_sent`]).

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Times(u32),
    Forever,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    /// Waiting for the delay before the next repetition.
    Waiting {
        until: Instant,
    },
    /// A repetition is queued and being sent.
    Sending,
    Finished,
    Stopped,
}

#[derive(Debug, Clone)]
pub struct SendJob {
    data: Vec<u8>,
    repeat: Repeat,
    delay: Duration,
    state: JobState,
    /// Repetitions fully sent.
    done: u32,
    /// Bytes of the current repetition sent so far.
    sent_in_rep: usize,
    total_sent: u64,
    started: Instant,
    /// When the job finished or was stopped; freezes the elapsed time.
    ended: Option<Instant>,
    /// Time spent actually sending (excludes delays), for the speed.
    send_time: Duration,
    rep_started: Option<Instant>,
}

impl SendJob {
    /// A job that starts sending at once. Empty data finishes immediately.
    pub fn new(data: Vec<u8>, repeat: Repeat, delay: Duration, now: Instant) -> Self {
        let finished = data.is_empty() || repeat == Repeat::Times(0);
        Self {
            data,
            repeat,
            delay,
            state: if finished {
                JobState::Finished
            } else {
                JobState::Waiting { until: now }
            },
            done: 0,
            sent_in_rep: 0,
            total_sent: 0,
            started: now,
            ended: finished.then_some(now),
            send_time: Duration::ZERO,
            rep_started: None,
        }
    }

    pub fn state(&self) -> JobState {
        self.state
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, JobState::Waiting { .. } | JobState::Sending)
    }

    /// Data to queue now, if a repetition is due.
    pub fn poll(&mut self, now: Instant) -> Option<Vec<u8>> {
        match self.state {
            JobState::Waiting { until } if now >= until => {
                self.state = JobState::Sending;
                self.sent_in_rep = 0;
                self.rep_started = Some(now);
                Some(self.data.clone())
            }
            _ => None,
        }
    }

    /// Record `n` bytes sent by the port.
    pub fn on_sent(&mut self, n: usize, now: Instant) {
        if self.state != JobState::Sending {
            return;
        }
        let n = n.min(self.data.len() - self.sent_in_rep);
        self.sent_in_rep += n;
        self.total_sent += n as u64;
        if self.sent_in_rep == self.data.len() {
            self.done += 1;
            if let Some(t) = self.rep_started.take() {
                self.send_time += now.saturating_duration_since(t);
            }
            self.state = match self.repeat {
                Repeat::Times(n) if self.done >= n => {
                    self.ended = Some(now);
                    JobState::Finished
                }
                _ => JobState::Waiting {
                    until: now + self.delay,
                },
            };
        }
    }

    /// Stop early. Data already queued is the caller's to discard.
    pub fn stop(&mut self) {
        self.stop_at(Instant::now());
    }

    pub fn stop_at(&mut self, now: Instant) {
        if self.is_running() {
            self.state = JobState::Stopped;
            self.ended = Some(now);
        }
    }

    // --- Progress -----------------------------------------------------------

    pub fn data_len(&self) -> usize {
        self.data.len()
    }

    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    /// Repetitions fully sent.
    pub fn completed(&self) -> u32 {
        self.done
    }

    /// The repetition in progress or next up, 1-based (like "Repetition 2 of 3").
    pub fn current(&self) -> u32 {
        match (self.state, self.repeat) {
            (JobState::Finished, Repeat::Times(n)) => n,
            _ => self.done + 1,
        }
    }

    /// Bytes of the current repetition sent so far.
    pub fn sent_in_repetition(&self) -> usize {
        match self.state {
            JobState::Sending => self.sent_in_rep,
            JobState::Finished => self.data.len(),
            _ => 0,
        }
    }

    pub fn total_sent(&self) -> u64 {
        self.total_sent
    }

    pub fn elapsed(&self, now: Instant) -> Duration {
        self.ended
            .unwrap_or(now)
            .saturating_duration_since(self.started)
    }

    /// Time until the next repetition starts, while waiting.
    pub fn next_in(&self, now: Instant) -> Option<Duration> {
        match self.state {
            JobState::Waiting { until } => Some(until.saturating_duration_since(now)),
            _ => None,
        }
    }

    /// Bytes per second while actually sending.
    pub fn speed(&self, now: Instant) -> Option<f64> {
        let mut t = self.send_time;
        if let (JobState::Sending, Some(s)) = (self.state, self.rep_started) {
            t += now.saturating_duration_since(s);
        }
        let secs = t.as_secs_f64();
        (secs > 0.05 && self.total_sent > 0).then(|| self.total_sent as f64 / secs)
    }

    /// Estimated time left, for a fixed number of repetitions.
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        let Repeat::Times(n) = self.repeat else {
            return None;
        };
        if !self.is_running() {
            return Some(Duration::ZERO);
        }
        let speed = self.speed(now)?;
        let total = self.data.len() as u64 * u64::from(n);
        let bytes_left = total.saturating_sub(self.total_sent) as f64;
        let delays_left = n.saturating_sub(self.done + 1);
        let wait = self.next_in(now).unwrap_or_default();
        Some(Duration::from_secs_f64(bytes_left / speed) + self.delay * delays_left + wait)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn three_repetitions_with_delay() {
        let t0 = Instant::now();
        let mut job = SendJob::new(b"ping".to_vec(), Repeat::Times(3), ms(500), t0);
        assert_eq!(job.poll(t0).as_deref(), Some(&b"ping"[..]));
        assert_eq!(job.poll(t0), None, "only once per repetition");
        job.on_sent(2, t0 + ms(1));
        assert_eq!(job.sent_in_repetition(), 2);
        job.on_sent(2, t0 + ms(2));
        assert_eq!(job.completed(), 1);
        assert_eq!(job.current(), 2);
        // Waits for the delay.
        assert_eq!(job.poll(t0 + ms(100)), None);
        assert_eq!(job.next_in(t0 + ms(102)), Some(ms(400)));
        assert!(job.poll(t0 + ms(502)).is_some());
        job.on_sent(4, t0 + ms(503));
        assert!(job.poll(t0 + ms(1003)).is_some());
        job.on_sent(4, t0 + ms(1004));
        assert_eq!(job.state(), JobState::Finished);
        assert_eq!(job.total_sent(), 12);
        assert_eq!(job.current(), 3);
    }

    #[test]
    fn forever_until_stopped() {
        let t0 = Instant::now();
        let mut job = SendJob::new(b"x".to_vec(), Repeat::Forever, Duration::ZERO, t0);
        for i in 0..100 {
            assert!(job.poll(t0).is_some(), "repetition {i}");
            job.on_sent(1, t0);
        }
        assert!(job.is_running());
        job.stop_at(t0 + ms(100));
        assert_eq!(job.state(), JobState::Stopped);
        assert_eq!(job.elapsed(t0 + ms(5000)), ms(100), "elapsed freezes");
        assert_eq!(job.poll(t0), None);
        assert_eq!(job.remaining(t0), None);
    }

    #[test]
    fn empty_data_or_zero_repetitions_finish_at_once() {
        let t0 = Instant::now();
        assert_eq!(
            SendJob::new(vec![], Repeat::Times(3), ms(0), t0).state(),
            JobState::Finished
        );
        assert_eq!(
            SendJob::new(b"a".to_vec(), Repeat::Times(0), ms(0), t0).state(),
            JobState::Finished
        );
    }

    #[test]
    fn speed_and_remaining() {
        let t0 = Instant::now();
        let mut job = SendJob::new(vec![0; 1000], Repeat::Times(2), ms(1000), t0);
        job.poll(t0);
        job.on_sent(500, t0 + ms(500));
        let speed = job.speed(t0 + ms(500)).unwrap();
        assert!((speed - 1000.0).abs() < 1.0, "{speed}");
        // 1500 bytes left at 1000 B/s, plus one 1 s delay.
        let left = job.remaining(t0 + ms(500)).unwrap();
        assert!((left.as_secs_f64() - 2.5).abs() < 0.01, "{left:?}");
    }

    #[test]
    fn extra_sent_bytes_are_ignored() {
        let t0 = Instant::now();
        let mut job = SendJob::new(b"ab".to_vec(), Repeat::Times(1), ms(0), t0);
        job.on_sent(5, t0); // not sending yet
        job.poll(t0);
        job.on_sent(5, t0);
        assert_eq!(job.total_sent(), 2);
        assert_eq!(job.state(), JobState::Finished);
    }
}
