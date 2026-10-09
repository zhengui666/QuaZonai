use std::process::{Command, Stdio};

#[test]
fn every_native_help_works_without_credentials_or_database() {
    let mut pending: Vec<Vec<String>> = vec![vec![]];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(path) = pending.pop() {
        assert!(visited.insert(path.clone()), "duplicate command: {path:?}");
        let output = Command::new(env!("CARGO_BIN_EXE_server"))
            .env_clear()
            .args(&path)
            .arg("--help")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success(), "help failed: {path:?}");
        assert!(output.stderr.is_empty(), "unexpected stderr: {path:?}");
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("Usage: quazonai"), "missing usage: {path:?}");
        let mut commands = false;
        for line in help.lines() {
            if line == "Commands:" {
                commands = true;
            } else if commands && line.is_empty() {
                break;
            } else if commands {
                let name = line.split_whitespace().next().unwrap();
                if name != "help" {
                    let mut child = path.clone();
                    child.push(name.to_owned());
                    pending.push(child);
                }
            }
        }
    }
    assert!(visited.contains(&vec!["client".into(), "project".into(), "list".into()]));
    assert!(visited.contains(&vec!["client".into(), "brief".into(), "freeze".into()]));
    for group in ["input-set", "policy"] {
        assert!(visited.contains(&vec!["client".into(), group.into(), "create".into()]));
    }
    for command in ["fields", "field", "source", "mappings"] {
        assert!(visited.contains(&vec!["client".into(), "migrate".into(), command.into()]));
    }
    for group in ["weights", "messages"] {
        assert!(visited.contains(&vec![
            "client".into(),
            "forward".into(),
            group.into(),
            "submit".into()
        ]));
    }
    assert!(visited.contains(&vec!["migrate".into()]));
    assert!(visited.contains(&vec!["recover-access".into()]));
    assert!(visited.contains(&vec!["mcp".into()]));
}

#[test]
fn preparation_and_archive_help_preserve_native_argument_positions() {
    for (args, positionals) in [
        (vec!["input-set", "create"], ""),
        (vec!["policy", "create"], ""),
        (vec!["migrate", "fields"], "<ID> <RECORD>"),
        (vec!["migrate", "field"], "<ID> <RECORD> <NAME>"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_server"))
            .env_clear()
            .arg("client")
            .args(&args)
            .arg("--help")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success(), "help failed: {args:?}");
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        let usage = help
            .lines()
            .find(|line| line.starts_with("Usage:"))
            .unwrap();
        assert!(usage.starts_with(&format!("Usage: quazonai client {}", args.join(" "))));
        assert!(!help.contains("--project-id"));
        if positionals.is_empty() {
            assert!(!usage.contains('<'), "unexpected positional: {usage}");
            assert!(help.contains("--idempotency-key <IDEMPOTENCY_KEY>"));
        } else {
            assert!(usage.contains(positionals), "{usage}");
            if args[1] == "field" {
                assert!(help.contains("--offset <OFFSET>"));
                assert!(help.contains("[default: 0]"));
            }
        }
    }
}
