use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum PingEvent {
    Reply { seq: u64, ms: f64 },
    Lost { seq: u64 },
    Exited(String),
}

/// Spawns `ping` for `host` and forwards parsed events, tagged with `idx`, to `tx`.
pub fn spawn(idx: usize, host: &str, interval: f64, tx: Sender<(usize, PingEvent)>) -> Result<Child> {
    let mut child = Command::new("ping")
        .args(["-n", "-O", "-i", &interval.to_string(), host])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run `ping`")?;

    let stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");

    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Some(event) = parse_line(&line)
                && tx.send((idx, event)).is_err()
            {
                return;
            }
        }
        let mut err = String::new();
        let _ = stderr.read_to_string(&mut err);
        let msg = err.lines().last().unwrap_or("ping exited").trim().to_string();
        let _ = tx.send((idx, PingEvent::Exited(msg)));
    });

    Ok(child)
}

pub fn parse_line(line: &str) -> Option<PingEvent> {
    let seq = field(line, "icmp_seq=")?.parse().ok()?;
    if let Some(time) = field(line, "time=") {
        return Some(PingEvent::Reply { seq, ms: time.parse().ok()? });
    }
    if line.starts_with("no answer yet") || line.starts_with("From ") {
        return Some(PingEvent::Lost { seq });
    }
    None
}

/// Returns the token following `key` up to the next whitespace.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let start = line.find(key)? + key.len();
    line[start..].split_whitespace().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply() {
        assert_eq!(
            parse_line("64 bytes from 1.1.1.1: icmp_seq=3 ttl=57 time=12.4 ms"),
            Some(PingEvent::Reply { seq: 3, ms: 12.4 })
        );
        assert_eq!(
            parse_line("64 bytes from 127.0.0.1: icmp_seq=1 ttl=64 time=0.031 ms"),
            Some(PingEvent::Reply { seq: 1, ms: 0.031 })
        );
    }

    #[test]
    fn no_answer() {
        assert_eq!(
            parse_line("no answer yet for icmp_seq=5"),
            Some(PingEvent::Lost { seq: 5 })
        );
    }

    #[test]
    fn unreachable() {
        assert_eq!(
            parse_line("From 10.0.0.1 icmp_seq=7 Destination Host Unreachable"),
            Some(PingEvent::Lost { seq: 7 })
        );
    }

    #[test]
    fn ignored() {
        assert_eq!(parse_line("PING 1.1.1.1 (1.1.1.1) 56(84) bytes of data."), None);
        assert_eq!(parse_line("rtt min/avg/max/mdev = 1.0/2.0/3.0/0.5 ms"), None);
        assert_eq!(parse_line(""), None);
    }
}
