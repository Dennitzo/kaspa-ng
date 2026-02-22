use crate::imports::*;
use serde::Serialize;
use std::collections::VecDeque;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub ts: String,
    pub level: String,
    pub message: String,
}

#[derive(Clone)]
pub struct LogStore {
    capacity: usize,
    lines: Arc<Mutex<VecDeque<LogLine>>>,
}

impl LogStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            lines: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
        }
    }

    pub fn push(&self, level: &str, message: &str) {
        let message = strip_ansi_codes(message);
        let line = LogLine {
            ts: chrono::Utc::now().to_rfc3339(),
            level: level.to_string(),
            message,
        };
        let mut guard = self.lines.lock().unwrap();
        if guard.len() >= self.capacity {
            guard.pop_front();
        }
        guard.push_back(line);
    }

    pub fn snapshot(&self, limit: usize) -> Vec<LogLine> {
        let guard = self.lines.lock().unwrap();
        let total = guard.len();
        let start = total.saturating_sub(limit);
        guard.iter().skip(start).cloned().collect()
    }
}

#[derive(Clone)]
pub struct LogStores {
    pub postgres: Arc<LogStore>,
    pub indexer: Arc<LogStore>,
    pub rest: Arc<LogStore>,
    pub socket: Arc<LogStore>,
}

fn strip_ansi_codes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if let Some('[') = chars.peek().copied() {
                let _ = chars.next();
                while let Some(inner) = chars.next() {
                    if inner.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }
            continue;
        }
        out.push(ch);
    }
    out
}
