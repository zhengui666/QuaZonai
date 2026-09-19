//! Pinned native exec presentation shared by the account-waived integration tests.
pub fn exec_part(text: &str) -> (Option<u64>, &str) {
    // Pinned ExecCommandToolOutput::response_text owns this presentation format.
    // Inspect only the header: JSON output is not process-state evidence.
    let (header, body) = text
        .split_once("\nOutput:\n")
        .expect("pinned native exec Output header required");
    let running: Vec<_> = header
        .lines()
        .filter_map(|line| line.strip_prefix("Process running with session ID "))
        .collect();
    let exits: Vec<_> = header
        .lines()
        .filter_map(|line| line.strip_prefix("Process exited with code "))
        .collect();
    match (running.as_slice(), exits.as_slice()) {
        ([session], []) => (Some(session.parse().expect("native exec session ID")), body),
        ([], ["0"]) => (None, body),
        _ => panic!("native exec must be pending or have exactly one successful exit"),
    }
}
