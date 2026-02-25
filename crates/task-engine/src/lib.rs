use std::fmt::{Display, Formatter};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailurePolicy {
    StopOnFail,
    ContinueOnFail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Manual,
    IntervalMs(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskAction {
    /// 发送十六进制字节序列。
    SendHex(Vec<u8>),
    /// 断言最近一次接收数据中包含目标片段。
    AssertContains(Vec<u8>),
    /// 阻塞等待指定时间。
    SleepMs(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSpec {
    /// 任务唯一标识。
    pub task_id: String,
    /// 任务名称。
    pub name: String,
    /// 触发策略。
    pub trigger: Trigger,
    /// 失败处理策略。
    pub failure_policy: FailurePolicy,
    /// 最大重复次数。
    pub repeat: u32,
    /// 动作列表。
    pub actions: Vec<TaskAction>,
}

impl Default for TaskSpec {
    fn default() -> Self {
        Self {
            task_id: "task-default".to_string(),
            name: "default task".to_string(),
            trigger: Trigger::Manual,
            failure_policy: FailurePolicy::StopOnFail,
            repeat: 1,
            actions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskReport {
    /// 任务标识。
    pub task_id: String,
    /// 是否成功。
    pub success: bool,
    /// 执行动作总数。
    pub executed_actions: usize,
    /// 失败动作索引。
    pub failed_at: Option<usize>,
    /// 文本日志。
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskError {
    InvalidScriptLine(String),
    SendFailed(String),
    AssertionFailed(String),
}

impl Display for TaskError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskError::InvalidScriptLine(line) => write!(f, "invalid script line: {line}"),
            TaskError::SendFailed(msg) => write!(f, "send failed: {msg}"),
            TaskError::AssertionFailed(msg) => write!(f, "assertion failed: {msg}"),
        }
    }
}

impl std::error::Error for TaskError {}

pub type TaskResult<T> = Result<T, TaskError>;

/// 任务执行依赖，由上层注入具体收发实现。
pub trait TaskIo {
    fn send(&mut self, data: &[u8]) -> Result<(), String>;
    fn latest_receive(&self) -> &[u8];
}

pub struct TaskEngine;

impl TaskEngine {
    pub fn run(io: &mut dyn TaskIo, spec: &TaskSpec) -> TaskReport {
        let repeat = spec.repeat.max(1);
        let mut report = TaskReport {
            task_id: spec.task_id.clone(),
            success: true,
            executed_actions: 0,
            failed_at: None,
            logs: Vec::new(),
        };

        let start = Instant::now();

        for loop_idx in 0..repeat {
            report.logs.push(format!("loop {loop_idx} start"));

            for (i, action) in spec.actions.iter().enumerate() {
                let res = execute_action(io, action);
                report.executed_actions += 1;

                match res {
                    Ok(msg) => report.logs.push(msg),
                    Err(err) => {
                        report.success = false;
                        report.failed_at = Some(i);
                        report.logs.push(err.to_string());

                        if spec.failure_policy == FailurePolicy::StopOnFail {
                            report.logs.push("task stopped by policy".to_string());
                            report
                                .logs
                                .push(format!("elapsed={}ms", start.elapsed().as_millis()));
                            return report;
                        }
                    }
                }
            }

            if let Trigger::IntervalMs(ms) = spec.trigger {
                thread::sleep(Duration::from_millis(ms));
            }
        }

        report
            .logs
            .push(format!("elapsed={}ms", start.elapsed().as_millis()));
        report
    }
}

fn execute_action(io: &mut dyn TaskIo, action: &TaskAction) -> TaskResult<String> {
    match action {
        TaskAction::SendHex(data) => {
            io.send(data).map_err(TaskError::SendFailed)?;
            Ok(format!("send {} bytes", data.len()))
        }
        TaskAction::AssertContains(needle) => {
            let haystack = io.latest_receive();
            if contains_subslice(haystack, needle) {
                Ok(format!("assert pass: contains {} bytes", needle.len()))
            } else {
                Err(TaskError::AssertionFailed(format!(
                    "needle={:02X?} not found",
                    needle
                )))
            }
        }
        TaskAction::SleepMs(ms) => {
            thread::sleep(Duration::from_millis(*ms));
            Ok(format!("sleep {}ms", ms))
        }
    }
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// 解析简单脚本为动作序列。
///
/// 支持语法：
/// - `SEND 01 0A FF`
/// - `ASSERT CONTAINS 0A 0B`
/// - `SLEEP 100`
pub fn parse_script(script: &str) -> TaskResult<Vec<TaskAction>> {
    let mut actions = Vec::new();

    for line in script.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        match tokens[0].to_ascii_uppercase().as_str() {
            "SEND" => {
                let bytes = parse_hex_tokens(&tokens[1..])?;
                actions.push(TaskAction::SendHex(bytes));
            }
            "ASSERT" => {
                if tokens.len() < 3 || !tokens[1].eq_ignore_ascii_case("CONTAINS") {
                    return Err(TaskError::InvalidScriptLine(line.to_string()));
                }
                let bytes = parse_hex_tokens(&tokens[2..])?;
                actions.push(TaskAction::AssertContains(bytes));
            }
            "SLEEP" => {
                if tokens.len() != 2 {
                    return Err(TaskError::InvalidScriptLine(line.to_string()));
                }
                let ms = tokens[1]
                    .parse::<u64>()
                    .map_err(|_| TaskError::InvalidScriptLine(line.to_string()))?;
                actions.push(TaskAction::SleepMs(ms));
            }
            _ => return Err(TaskError::InvalidScriptLine(line.to_string())),
        }
    }

    Ok(actions)
}

fn parse_hex_tokens(tokens: &[&str]) -> TaskResult<Vec<u8>> {
    if tokens.is_empty() {
        return Err(TaskError::InvalidScriptLine(
            "missing hex bytes".to_string(),
        ));
    }

    let mut out = Vec::new();
    for token in tokens {
        let value = u8::from_str_radix(token, 16)
            .map_err(|_| TaskError::InvalidScriptLine(format!("invalid hex token: {token}")))?;
        out.push(value);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MockIo {
        /// 已发送数据队列。
        sent: Vec<Vec<u8>>,
        /// 最近接收缓存。
        latest_rx: Vec<u8>,
    }

    impl TaskIo for MockIo {
        fn send(&mut self, data: &[u8]) -> Result<(), String> {
            self.sent.push(data.to_vec());
            Ok(())
        }

        fn latest_receive(&self) -> &[u8] {
            &self.latest_rx
        }
    }

    #[test]
    fn parse_script_should_work() {
        let script = "\n# comment\nSEND 01 02\nASSERT CONTAINS 02\nSLEEP 1\n";
        let actions = parse_script(script).unwrap();
        assert_eq!(actions.len(), 3);
    }

    #[test]
    fn run_task_should_pass() {
        let mut io = MockIo {
            sent: Vec::new(),
            latest_rx: vec![0x10, 0x20, 0x30],
        };

        let spec = TaskSpec {
            task_id: "t1".to_string(),
            name: "demo".to_string(),
            trigger: Trigger::Manual,
            failure_policy: FailurePolicy::StopOnFail,
            repeat: 1,
            actions: vec![
                TaskAction::SendHex(vec![0xAA]),
                TaskAction::AssertContains(vec![0x20]),
            ],
        };

        let report = TaskEngine::run(&mut io, &spec);
        assert!(report.success);
        assert_eq!(io.sent, vec![vec![0xAA]]);
    }

    #[test]
    fn run_task_should_stop_on_fail() {
        let mut io = MockIo {
            sent: Vec::new(),
            latest_rx: vec![0x01],
        };

        let spec = TaskSpec {
            task_id: "t2".to_string(),
            name: "demo".to_string(),
            trigger: Trigger::Manual,
            failure_policy: FailurePolicy::StopOnFail,
            repeat: 1,
            actions: vec![
                TaskAction::AssertContains(vec![0xFF]),
                TaskAction::SendHex(vec![0x10]),
            ],
        };

        let report = TaskEngine::run(&mut io, &spec);
        assert!(!report.success);
        assert!(io.sent.is_empty());
    }
}
