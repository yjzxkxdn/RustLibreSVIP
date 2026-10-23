//! Session handles and project summaries — port of `libresvip.cli.mcp.session`.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use libresvip_core::model::base::{Project, Track};
use serde_json::{json, Value};

pub const DEFAULT_CAPACITY: usize = 8;

/// `ProjectSession` — an LRU of loaded projects keyed by handle.
pub struct ProjectSession {
    entries: Vec<(String, Project)>,
    capacity: usize,
}

impl Default for ProjectSession {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

impl ProjectSession {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::new(),
            capacity,
        }
    }

    /// `put`: stores a project and returns a fresh handle.
    pub fn put(&mut self, project: Project) -> String {
        let handle = new_handle();
        self.entries.push((handle.clone(), project));
        while self.entries.len() > self.capacity {
            self.entries.remove(0);
        }
        handle
    }

    /// Immutable access that also refreshes the LRU position (`get`).
    pub fn get(&mut self, handle: &str) -> Option<&Project> {
        let index = self.touch(handle)?;
        Some(&self.entries[index].1)
    }

    /// Removes a project, refreshing its LRU position first (used by the
    /// middleware tools, which consume and re-insert the project).
    pub fn take(&mut self, handle: &str) -> Option<Project> {
        let index = self.touch(handle)?;
        Some(self.entries.remove(index).1)
    }

    /// `replace`: stores a project under an existing handle.
    pub fn replace(&mut self, handle: &str, project: Project) {
        if let Some(index) = self.touch(handle) {
            self.entries[index].1 = project;
        } else {
            self.entries.push((handle.to_owned(), project));
        }
    }

    /// `drop`.
    pub fn drop_handle(&mut self, handle: &str) -> bool {
        match self.entries.iter().position(|(key, _)| key == handle) {
            Some(index) => {
                self.entries.remove(index);
                true
            }
            None => false,
        }
    }

    fn touch(&mut self, handle: &str) -> Option<usize> {
        let index = self.entries.iter().position(|(key, _)| key == handle)?;
        if index + 1 != self.entries.len() {
            let entry = self.entries.remove(index);
            self.entries.push(entry);
            return Some(self.entries.len() - 1);
        }
        Some(index)
    }
}

/// `uuid4().hex` equivalent: 32 lowercase hex characters, unique per process.
fn new_handle() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let mut handle = String::with_capacity(32);
    for salt in 0..2u64 {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(counter);
        hasher.write_u64(salt);
        hasher.write(&nanos.to_le_bytes());
        hasher.write_usize(std::process::id() as usize);
        handle.push_str(&format!("{:016x}", hasher.finish()));
    }
    handle
}

/// `summarize` — the `ProjectSummary` TypedDict rendered in declaration order.
pub fn summarize(handle: &str, project: &Project, warnings: &[String]) -> Value {
    let mut tracks = Vec::new();
    let mut note_count: i64 = 0;
    let mut singing_track_count: i64 = 0;
    for track in &project.track_list {
        match track {
            Track::Singing(singing) => {
                singing_track_count += 1;
                note_count += singing.note_list.len() as i64;
                tracks.push(Value::String(format!(
                    "{} ({} notes)",
                    title_or_untitled(&singing.title),
                    singing.note_list.len()
                )));
            }
            Track::Instrumental(instrumental) => {
                tracks.push(Value::String(format!(
                    "{} (instrumental)",
                    title_or_untitled(&instrumental.title)
                )));
            }
        }
    }
    let first_bpm = project
        .song_tempo_list
        .first()
        .map(|tempo| Value::from(tempo.bpm))
        .unwrap_or(Value::Null);
    json!({
        "handle": handle,
        "track_count": project.track_list.len(),
        "singing_track_count": singing_track_count,
        "note_count": note_count,
        "tempo_count": project.song_tempo_list.len(),
        "time_signature_count": project.time_signature_list.len(),
        "first_bpm": first_bpm,
        "tracks": tracks,
        "warnings": warnings,
    })
}

fn title_or_untitled(title: &str) -> String {
    if title.is_empty() {
        "(untitled)".to_owned()
    } else {
        title.to_owned()
    }
}
