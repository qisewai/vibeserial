use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionProfile {
    /// 会话标识。
    pub session_id: String,
    /// 串口端点。
    pub endpoint: String,
    /// 波特率。
    pub baud_rate: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSnapshot {
    /// 项目标识。
    pub project_id: String,
    /// 项目名称。
    pub name: String,
    /// 最后更新时间戳（毫秒）。
    pub updated_at_ms: u128,
    /// 会话配置列表。
    pub sessions: Vec<SessionProfile>,
    /// 任务标识列表。
    pub task_ids: Vec<String>,
    /// 日志文件索引。
    pub log_files: Vec<String>,
}

impl ProjectSnapshot {
    pub fn new(project_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            project_id: project_id.into(),
            name: name.into(),
            updated_at_ms: now_ms(),
            sessions: Vec::new(),
            task_ids: Vec::new(),
            log_files: Vec::new(),
        }
    }

    pub fn touch(&mut self) {
        self.updated_at_ms = now_ms();
    }
}

#[derive(Debug)]
pub enum StoreError {
    Io(String),
    Parse(String),
}

impl Display for StoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(msg) => write!(f, "io error: {msg}"),
            StoreError::Parse(msg) => write!(f, "parse error: {msg}"),
        }
    }
}

impl std::error::Error for StoreError {}

pub type StoreResult<T> = Result<T, StoreError>;

pub struct ProjectStore {
    /// 项目数据根目录。
    root_dir: PathBuf,
}

impl ProjectStore {
    pub fn new(root_dir: impl AsRef<Path>) -> Self {
        Self {
            root_dir: root_dir.as_ref().to_path_buf(),
        }
    }

    pub fn save(&self, snapshot: &ProjectSnapshot) -> StoreResult<PathBuf> {
        fs::create_dir_all(&self.root_dir).map_err(|e| StoreError::Io(e.to_string()))?;
        let path = self.root_dir.join(format!("{}.vsp", snapshot.project_id));
        let content = encode_snapshot(snapshot);
        fs::write(&path, content).map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(path)
    }

    pub fn load(&self, project_id: &str) -> StoreResult<ProjectSnapshot> {
        let path = self.root_dir.join(format!("{project_id}.vsp"));
        let content = fs::read_to_string(path).map_err(|e| StoreError::Io(e.to_string()))?;
        decode_snapshot(&content)
    }

    pub fn list_project_ids(&self) -> StoreResult<Vec<String>> {
        if !self.root_dir.exists() {
            return Ok(Vec::new());
        }

        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.root_dir).map_err(|e| StoreError::Io(e.to_string()))? {
            let entry = entry.map_err(|e| StoreError::Io(e.to_string()))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("vsp") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    ids.push(stem.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }
}

fn encode_snapshot(snapshot: &ProjectSnapshot) -> String {
    let mut lines = vec![
        format!("PROJECT_ID={}", escape(&snapshot.project_id)),
        format!("NAME={}", escape(&snapshot.name)),
        format!("UPDATED_AT_MS={}", snapshot.updated_at_ms),
        format!("SESSIONS={}", snapshot.sessions.len()),
    ];

    for s in &snapshot.sessions {
        lines.push(format!(
            "SESSION|{}|{}|{}",
            escape(&s.session_id),
            escape(&s.endpoint),
            s.baud_rate
        ));
    }

    lines.push(format!("TASKS={}", snapshot.task_ids.len()));
    for task_id in &snapshot.task_ids {
        lines.push(format!("TASK|{}", escape(task_id)));
    }

    lines.push(format!("LOGS={}", snapshot.log_files.len()));
    for log in &snapshot.log_files {
        lines.push(format!("LOG|{}", escape(log)));
    }

    lines.join("\n")
}

fn decode_snapshot(content: &str) -> StoreResult<ProjectSnapshot> {
    let mut project_id = None;
    let mut name = None;
    let mut updated_at_ms = None;
    let mut sessions = Vec::new();
    let mut task_ids = Vec::new();
    let mut log_files = Vec::new();

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("PROJECT_ID=") {
            project_id = Some(unescape(rest));
            continue;
        }
        if let Some(rest) = line.strip_prefix("NAME=") {
            name = Some(unescape(rest));
            continue;
        }
        if let Some(rest) = line.strip_prefix("UPDATED_AT_MS=") {
            let ts = rest
                .parse::<u128>()
                .map_err(|_| StoreError::Parse("invalid UPDATED_AT_MS".to_string()))?;
            updated_at_ms = Some(ts);
            continue;
        }
        if let Some(rest) = line.strip_prefix("SESSION|") {
            let parts: Vec<&str> = rest.split('|').collect();
            if parts.len() != 3 {
                return Err(StoreError::Parse("invalid SESSION line".to_string()));
            }
            let baud_rate = parts[2]
                .parse::<u32>()
                .map_err(|_| StoreError::Parse("invalid session baud_rate".to_string()))?;
            sessions.push(SessionProfile {
                session_id: unescape(parts[0]),
                endpoint: unescape(parts[1]),
                baud_rate,
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix("TASK|") {
            task_ids.push(unescape(rest));
            continue;
        }
        if let Some(rest) = line.strip_prefix("LOG|") {
            log_files.push(unescape(rest));
            continue;
        }
    }

    Ok(ProjectSnapshot {
        project_id: project_id
            .ok_or_else(|| StoreError::Parse("missing PROJECT_ID".to_string()))?,
        name: name.ok_or_else(|| StoreError::Parse("missing NAME".to_string()))?,
        updated_at_ms: updated_at_ms
            .ok_or_else(|| StoreError::Parse("missing UPDATED_AT_MS".to_string()))?,
        sessions,
        task_ids,
        log_files,
    })
}

fn escape(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('|', "\\p")
        .replace('\n', "\\n")
}

fn unescape(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('p') => out.push('|'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }

    out
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root() -> PathBuf {
        let mut path = std::env::temp_dir();
        let ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        path.push(format!("vibeserial-test-{}-{}", std::process::id(), ns));
        path
    }

    #[test]
    fn save_and_load_should_work() {
        let root = test_root();
        let store = ProjectStore::new(&root);

        let mut snapshot = ProjectSnapshot::new("p1", "demo");
        snapshot.sessions.push(SessionProfile {
            session_id: "s1".to_string(),
            endpoint: "/dev/ttyUSB0".to_string(),
            baud_rate: 115_200,
        });
        snapshot.task_ids.push("task-a".to_string());
        snapshot.log_files.push("logs/2026-02-24.log".to_string());

        store.save(&snapshot).unwrap();
        let loaded = store.load("p1").unwrap();

        assert_eq!(loaded.project_id, "p1");
        assert_eq!(loaded.sessions.len(), 1);
        assert_eq!(loaded.task_ids, vec!["task-a"]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn list_project_ids_should_sort() {
        let root = test_root();
        let store = ProjectStore::new(&root);

        let a = ProjectSnapshot::new("a", "A");
        let b = ProjectSnapshot::new("b", "B");
        store.save(&b).unwrap();
        store.save(&a).unwrap();

        let ids = store.list_project_ids().unwrap();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);

        let _ = fs::remove_dir_all(root);
    }
}
