//! `is-ai-agent` command-line tool.
//!
//! Exits `0` when an AI agent is detected, `1` when none is, and `2` on a
//! usage error, so it composes directly with shell conditionals:
//!
//! ```sh
//! if is-ai-agent; then echo "running under an agent"; fi
//! ```
//!
//! Silent by default. `--output <FORMAT>` prints details to stdout without
//! changing the exit status.

use std::io::{self, Write};
use std::process::ExitCode;

use is_ai_agent::{Agent, Signal, detect};

const DETECTED: u8 = 0;
const NOT_DETECTED: u8 = 1;
const USAGE_ERROR: u8 = 2;

const HELP: &str = "\
Detect whether this process is being run by an AI coding agent.

Usage: is-ai-agent [OPTIONS]

Exit status:
  0  an AI agent was detected
  1  no AI agent was detected
  2  usage error

Options:
  -o, --output [<FORMAT>]  Print details to stdout (exit status is unchanged):
                             id    agent slug, e.g. `claude-code` (default; nothing if none)
                             name  display name, e.g. `Claude Code` (nothing if none)
                             text  one-line human-readable summary
                             json  full detection result as a JSON object
  -h, --help               Print help
  -V, --version            Print version

Examples:
  if is-ai-agent; then echo \"agent\"; fi
  agent=$(is-ai-agent --output) && echo \"running under $agent\"
  is-ai-agent -o json | jq .
";

#[derive(Clone, Copy)]
enum Format {
    Id,
    Name,
    Text,
    Json,
}

impl Format {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "id" => Some(Self::Id),
            "name" => Some(Self::Name),
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

enum Command {
    Detect(Option<Format>),
    Help,
    Version,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut format = None;
    let mut args = args.into_iter().peekable();
    while let Some(arg) = args.next() {
        let value = match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            // A bare `-o`/`--output` defaults to `id`; the following argument
            // is only taken as the format when it isn't another flag.
            "-o" | "--output" => args
                .next_if(|next| !next.starts_with('-'))
                .unwrap_or_else(|| "id".to_string()),
            _ => {
                if let Some(v) = arg.strip_prefix("--output=") {
                    v.to_string()
                } else if let Some(v) = arg.strip_prefix("-o").filter(|v| !v.is_empty()) {
                    v.trim_start_matches('=').to_string()
                } else {
                    return Err(format!("unexpected argument '{arg}'"));
                }
            }
        };
        format = Some(Format::parse(&value).ok_or_else(|| {
            format!("invalid output format '{value}' (expected id, name, text or json)")
        })?);
    }
    Ok(Command::Detect(format))
}

fn main() -> ExitCode {
    let command = match parse_args(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("is-ai-agent: {message}\n\nFor more information, try '--help'.");
            return ExitCode::from(USAGE_ERROR);
        }
    };

    let format = match command {
        Command::Help => return emit(HELP.to_string(), DETECTED),
        Command::Version => {
            return emit(
                format!("is-ai-agent {}\n", env!("CARGO_PKG_VERSION")),
                DETECTED,
            );
        }
        Command::Detect(format) => format,
    };

    let agent = detect();
    let status = if agent.is_some() {
        DETECTED
    } else {
        NOT_DETECTED
    };
    match format {
        None => ExitCode::from(status),
        Some(format) => emit(render(format, agent.as_ref()), status),
    }
}

/// Write `output` to stdout, ignoring errors such as a closed pipe so that a
/// consumer like `head` can never turn detection into a panic.
fn emit(output: String, status: u8) -> ExitCode {
    let mut stdout = io::stdout().lock();
    let _ = stdout.write_all(output.as_bytes());
    let _ = stdout.flush();
    ExitCode::from(status)
}

fn render(format: Format, agent: Option<&Agent>) -> String {
    match (format, agent) {
        (Format::Id, Some(agent)) => format!("{}\n", agent.id.as_str()),
        (Format::Name, Some(agent)) => format!("{}\n", agent.name),
        (Format::Id | Format::Name, None) => String::new(),
        (Format::Text, Some(agent)) => format!(
            "{} ({}) detected via {}\n",
            agent.name,
            agent.id.as_str(),
            describe_signal(&agent.signal)
        ),
        (Format::Text, None) => "no AI agent detected\n".to_string(),
        (Format::Json, agent) => format!("{}\n", to_json(agent)),
    }
}

fn describe_signal(signal: &Signal) -> String {
    match signal {
        Signal::EnvVar { name, value } => format!("env {name}={value}"),
        Signal::File { path } => format!("file {path}"),
        _ => "unknown signal".to_string(),
    }
}

/// A stable schema: every key is always present, `null` when unavailable.
fn to_json(agent: Option<&Agent>) -> String {
    let Some(agent) = agent else {
        return r#"{"detected":false,"id":null,"name":null,"signal":null,"session_id":null,"trace_id":null,"traceparent":null}"#.to_string();
    };
    let signal = match &agent.signal {
        Signal::EnvVar { name, value } => format!(
            r#"{{"type":"env","name":{},"value":{}}}"#,
            json_str(name),
            json_str(value)
        ),
        Signal::File { path } => format!(r#"{{"type":"file","path":{}}}"#, json_str(path)),
        _ => r#"{"type":"unknown"}"#.to_string(),
    };
    format!(
        r#"{{"detected":true,"id":{},"name":{},"signal":{},"session_id":{},"trace_id":{},"traceparent":{}}}"#,
        json_str(agent.id.as_str()),
        json_str(agent.name),
        signal,
        json_opt(agent.session_id.as_deref()),
        json_opt(agent.trace_id()),
        json_opt(agent.traceparent.as_deref()),
    )
}

fn json_opt(value: Option<&str>) -> String {
    value.map_or_else(|| "null".to_string(), json_str)
}

fn json_str(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
