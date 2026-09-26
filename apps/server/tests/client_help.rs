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
