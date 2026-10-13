//! End-to-end tests for the `is-ai-agent` binary.
//!
//! Each test runs the binary with a cleared environment so results do not
//! depend on the harness running `cargo test`.

use std::process::{Command, Output};

fn run(env: &[(&str, &str)], args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_is-ai-agent"));
    cmd.env_clear().args(args);
    // Windows needs SYSTEMROOT to start processes.
    if let Ok(root) = std::env::var("SYSTEMROOT") {
        cmd.env("SYSTEMROOT", root);
    }
    cmd.envs(env.iter().copied());
    cmd.output().expect("failed to run is-ai-agent")
}

fn stdout(out: &Output) -> &str {
    std::str::from_utf8(&out.stdout).unwrap()
}

// `/opt/.devin` would make the "no agent" cases detect Devin.
fn devin_marker_present() -> bool {
    std::path::Path::new("/opt/.devin").exists()
}

#[test]
fn silent_and_exit_0_when_agent_detected() {
    let out = run(&[("CLAUDECODE", "1")], &[]);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
    assert!(out.stderr.is_empty());
}

#[test]
fn silent_and_exit_1_when_no_agent() {
    if devin_marker_present() {
        return;
    }
    let out = run(&[], &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(out.stderr.is_empty());
}

#[test]
fn output_id_and_name() {
    let out = run(&[("CLAUDECODE", "1")], &["--output", "id"]);
    assert_eq!(stdout(&out), "claude-code\n");
    let out = run(&[("CLAUDECODE", "1")], &["-o", "name"]);
    assert_eq!(stdout(&out), "Claude Code\n");
    let out = run(&[("CLAUDECODE", "1")], &["--output=id"]);
    assert_eq!(stdout(&out), "claude-code\n");
    let out = run(&[("CLAUDECODE", "1")], &["-oid"]);
    assert_eq!(stdout(&out), "claude-code\n");
}

#[test]
fn output_id_is_empty_and_exit_1_when_no_agent() {
    if devin_marker_present() {
        return;
    }
    let out = run(&[], &["-o", "id"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
}

#[test]
fn output_text() {
    let out = run(&[("CLAUDECODE", "1")], &["-o", "text"]);
    assert_eq!(
        stdout(&out),
        "Claude Code (claude-code) detected via env CLAUDECODE=1\n"
    );
    if !devin_marker_present() {
        let out = run(&[], &["-o", "text"]);
        assert_eq!(stdout(&out), "no AI agent detected\n");
    }
}

#[test]
fn output_json_detected() {
    let out = run(
        &[
            ("CLAUDECODE", "1"),
            (
                "TRACEPARENT",
                "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            ),
        ],
        &["-o", "json"],
    );
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stdout(&out),
        concat!(
            r#"{"detected":true,"id":"claude-code","name":"Claude Code","#,
            r#""signal":{"type":"env","name":"CLAUDECODE","value":"1"},"#,
            r#""session_id":null,"trace_id":"4bf92f3577b34da6a3ce929d0e0e4736","#,
            r#""traceparent":"00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"}"#,
            "\n"
        )
    );
}

#[test]
fn output_json_escapes_values() {
    let out = run(&[("AI_AGENT", "we\"ird\\name")], &["-o", "json"]);
    assert!(
        stdout(&out).contains(r#""value":"we\"ird\\name""#),
        "{}",
        stdout(&out)
    );
}

#[test]
fn output_json_not_detected() {
    if devin_marker_present() {
        return;
    }
    let out = run(&[], &["-o", "json"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stdout(&out),
        "{\"detected\":false,\"id\":null,\"name\":null,\"signal\":null,\"session_id\":null,\"trace_id\":null,\"traceparent\":null}\n"
    );
}

#[test]
fn bare_output_defaults_to_id() {
    for args in [&["--output"][..], &["-o"]] {
        let out = run(&[("CLAUDECODE", "1")], args);
        assert_eq!(out.status.code(), Some(0), "{args:?}");
        assert_eq!(stdout(&out), "claude-code\n", "{args:?}");
    }
    // A following flag is not consumed as the format.
    let out = run(&[("CLAUDECODE", "1")], &["-o", "--output", "name"]);
    assert_eq!(stdout(&out), "Claude Code\n");
}

#[test]
fn usage_errors_exit_2() {
    for args in [&["--bogus"][..], &["-o", "yaml"], &["--output="]] {
        let out = run(&[("CLAUDECODE", "1")], args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
    }
}

#[test]
fn help_and_version_exit_0() {
    let out = run(&[], &["--help"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).contains("Usage: is-ai-agent"));
    let out = run(&[], &["-V"]);
    assert_eq!(
        stdout(&out),
        format!("is-ai-agent {}\n", env!("CARGO_PKG_VERSION"))
    );
}
