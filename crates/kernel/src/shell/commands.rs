#![allow(dead_code)]

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    SystemStatus,
    MemoryStatus,
    SchedulerStatus,
    SecurityStatus,
    FileList { path: String },
    FileRead { path: String },
    FileWrite { path: String, content: String },
    FileCreate { path: String },
    ProcessList,
    ProcessInfo { pid: u16 },
    ExplainScheduler,
    ExplainMediator,
    ExplainDecision { context: String },
    AskAI { query: String },
    Unknown { raw: String },
}

pub fn parse(input: &str) -> Intent {
    let trimmed = input.trim().to_lowercase();

    match trimmed.as_str() {
        "status" | "stat" => Intent::SystemStatus,
        "mem" | "memory" => Intent::MemoryStatus,
        "sched" | "scheduler" => Intent::SchedulerStatus,
        "sec" | "security" => Intent::SecurityStatus,
        "ps" | "processes" => Intent::ProcessList,
        "explain scheduler" | "why scheduler" => Intent::ExplainScheduler,
        "explain mediator" | "why blocked" => Intent::ExplainMediator,
        _ => parse_natural_language(&trimmed),
    }
}

fn parse_natural_language(input: &str) -> Intent {
    if input.starts_with("ls ") || input.starts_with("list ") {
        let path = extract_path(input).unwrap_or_else(|| "/".into());
        return Intent::FileList { path };
    }
    if input.starts_with("cat ") || input.starts_with("read ") {
        if let Some(path) = extract_path(input) {
            return Intent::FileRead { path };
        }
    }
    if input.starts_with("mkdir ") || input.starts_with("create ") {
        if let Some(path) = extract_path(input) {
            return Intent::FileCreate { path };
        }
    }
    if input.contains("write") && input.contains(" to ") {
        return parse_write(input);
    }

    if input.contains("process") || input.contains(" ps ") || input.contains("running") {
        return Intent::ProcessList;
    }

    if input.contains("memory") || input.contains("ram") || input.contains("free") {
        return Intent::MemoryStatus;
    }
    if input.contains("scheduler") || input.contains("cpu") || input.contains("priority") {
        return Intent::SchedulerStatus;
    }
    if input.contains("security")
        || input.contains("threat")
        || input.contains("blocked")
        || input.contains("attack")
    {
        return Intent::SecurityStatus;
    }

    if input.contains("explain")
        || input.contains("why")
        || input.contains("how did")
        || input.contains("decision")
    {
        return Intent::ExplainDecision {
            context: input.into(),
        };
    }

    Intent::AskAI {
        query: input.into(),
    }
}

fn extract_path(input: &str) -> Option<String> {
    let parts: Vec<&str> = input.splitn(2, ' ').collect();
    if parts.len() == 2 && !parts[1].is_empty() {
        Some(parts[1].trim().into())
    } else {
        None
    }
}

fn parse_write(input: &str) -> Intent {
    if let Some(to_idx) = input.find(" to ") {
        let content = input[..to_idx]
            .trim_start_matches("write")
            .trim()
            .to_string();
        let path = input[to_idx + 4..].trim().to_string();
        return Intent::FileWrite { path, content };
    }
    Intent::Unknown { raw: input.into() }
}
