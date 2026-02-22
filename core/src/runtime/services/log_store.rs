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
    pub k_indexer: Arc<LogStore>,
    pub rest: Arc<LogStore>,
    pub socket: Arc<LogStore>,
}

fn strip_ansi_codes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    // CSI: ESC [ ... final-byte(0x40..0x7E)
                    while let Some(inner) = chars.next() {
                        let b = inner as u32;
                        if (0x40..=0x7e).contains(&b) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    // OSC: ESC ] ... BEL or ESC \
                    while let Some(inner) = chars.next() {
                        if inner == '\u{7}' {
                            break;
                        }
                        if inner == '\u{1b}' && matches!(chars.peek().copied(), Some('\\')) {
                            let _ = chars.next();
                            break;
                        }
                    }
                }
                Some('P') | Some('X') | Some('^') | Some('_') => {
                    // DCS/SOS/PM/APC: ESC <type> ... ESC \
                    while let Some(inner) = chars.next() {
                        if inner == '\u{1b}' && matches!(chars.peek().copied(), Some('\\')) {
                            let _ = chars.next();
                            break;
                        }
                    }
                }
                Some(_) | None => {}
            }
            continue;
        }

        // Keep printable chars and common whitespace only.
        if ch.is_control() && !matches!(ch, '\n' | '\r' | '\t') {
            continue;
        }
        out.push(ch);
    }
    out
}
