use std::collections::VecDeque;

use crate::ping::PingEvent;

const MAX_SAMPLES: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sample {
    Rtt { seq: u64, ms: f64 },
    Lost { seq: u64 },
}

impl Sample {
    pub fn seq(&self) -> u64 {
        match *self {
            Sample::Rtt { seq, .. } | Sample::Lost { seq } => seq,
        }
    }
}

pub struct Host {
    pub addr: String,
    pub samples: VecDeque<Sample>,
    pub received: u64,
    pub lost: u64,
    pub last: Option<f64>,
    pub min: f64,
    pub max: f64,
    sum: f64,
    /// Set when the ping process has exited (e.g. bad hostname).
    pub status: Option<String>,
}

impl Host {
    fn new(addr: String) -> Self {
        Self {
            addr,
            samples: VecDeque::new(),
            received: 0,
            lost: 0,
            last: None,
            min: f64::INFINITY,
            max: 0.0,
            sum: 0.0,
            status: None,
        }
    }

    pub fn avg(&self) -> Option<f64> {
        (self.received > 0).then(|| self.sum / self.received as f64)
    }

    pub fn loss_pct(&self) -> f64 {
        let total = self.received + self.lost;
        if total == 0 { 0.0 } else { self.lost as f64 * 100.0 / total as f64 }
    }

    fn apply(&mut self, event: PingEvent) {
        match event {
            PingEvent::Reply { seq, ms } => {
                // A late reply may arrive after the seq was already reported lost.
                match self.samples.iter_mut().rev().find(|s| s.seq() == seq) {
                    Some(s @ Sample::Lost { .. }) => {
                        *s = Sample::Rtt { seq, ms };
                        self.lost -= 1;
                    }
                    Some(Sample::Rtt { .. }) => return, // duplicate reply
                    None => self.push(Sample::Rtt { seq, ms }),
                }
                self.received += 1;
                self.last = Some(ms);
                self.min = self.min.min(ms);
                self.max = self.max.max(ms);
                self.sum += ms;
            }
            PingEvent::Lost { seq } => {
                if self.samples.iter().rev().any(|s| s.seq() == seq) {
                    return;
                }
                self.push(Sample::Lost { seq });
                self.lost += 1;
            }
            PingEvent::Exited(msg) => self.status = Some(msg),
        }
    }

    fn push(&mut self, sample: Sample) {
        if self.samples.len() == MAX_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }
}

pub struct App {
    pub hosts: Vec<Host>,
}

impl App {
    pub fn new(addrs: Vec<String>) -> Self {
        Self { hosts: addrs.into_iter().map(Host::new).collect() }
    }

    pub fn apply(&mut self, idx: usize, event: PingEvent) {
        self.hosts[idx].apply(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_reply_replaces_loss() {
        let mut h = Host::new("x".into());
        h.apply(PingEvent::Lost { seq: 1 });
        assert_eq!(h.lost, 1);
        h.apply(PingEvent::Reply { seq: 1, ms: 5.0 });
        assert_eq!(h.lost, 0);
        assert_eq!(h.received, 1);
        assert_eq!(h.samples, [Sample::Rtt { seq: 1, ms: 5.0 }]);
    }
}
