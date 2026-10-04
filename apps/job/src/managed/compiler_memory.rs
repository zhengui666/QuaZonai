//! Read only this job's private cgroup. No host lookup, watcher, or timing change.
use std::{
    fs::File,
    io::{Read, Seek},
    process::ExitStatus,
};

#[derive(Debug)]
pub struct CompilerMemoryLimit;

impl std::fmt::Display for CompilerMemoryLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NATIVE_COMPILER_MEMORY_LIMIT")
    }
}
impl std::error::Error for CompilerMemoryLimit {}

pub(super) struct CompilerMemory {
    events: File,
    before: [u64; 3],
}

fn read_text(reader: &mut impl Read) -> Option<String> {
    // Kernel pseudo-files have size zero; bound the read instead of metadata.
    let mut text = String::new();
    reader.take(4097).read_to_string(&mut text).ok()?;
    (text.len() <= 4096).then_some(text)
}

fn counters(text: &str) -> Option<[u64; 3]> {
    let mut values = [None; 3];
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let key = fields.next()?;
        let value = fields.next()?;
        if fields.next().is_some() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let value = value.parse::<u64>().ok()?;
        if let Some(index) = ["max", "oom", "oom_kill"]
            .iter()
            .position(|name| *name == key)
        {
            if values[index].replace(value).is_some() {
                return None;
            }
        }
    }
    Some([values[0]?, values[1]?, values[2]?])
}

fn local_oom(before: [u64; 3], after: [u64; 3]) -> bool {
    // oom_kill includes global OOM kills. Fresh local max and oom events also
    // establish that this cgroup reached its own limit, rather than merely
    // suffering an unrelated SIGKILL or a host-wide memory-pressure victim.
    // https://docs.kernel.org/admin-guide/cgroup-v2.html#memory-interface-files
    before
        .into_iter()
        .zip(after)
        .all(|(before, after)| after > before)
}

impl CompilerMemory {
    pub(super) fn capture() -> Option<Self> {
        // Runtime selects a private cgroup-v2 namespace. Never reinterpret a
        // host/shared namespace or search another process's cgroup path.
        if read_text(&mut File::open("/proc/self/cgroup").ok()?)?.trim() != "0::/" {
            return None;
        }
        let mut events = File::open("/sys/fs/cgroup/memory.events.local").ok()?;
        let before = counters(&read_text(&mut events)?)?;
        Some(Self { events, before })
    }

    pub(super) fn check(
        status: ExitStatus,
        mut memory: Option<Self>,
        message: &'static str,
    ) -> anyhow::Result<()> {
        if status.success() {
            return Ok(());
        }
        let observed = memory.as_mut().and_then(|memory| {
            // Retain the original kernel file description across the child;
            // missing/invalid evidence cannot turn a generic exit into OOM.
            memory.events.rewind().ok()?;
            let after = counters(&read_text(&mut memory.events)?)?;
            Some(local_oom(memory.before, after))
        });
        if observed == Some(true) {
            return Err(CompilerMemoryLimit.into());
        }
        anyhow::bail!(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_events_require_complete_unique_unsigned_counters() {
        assert_eq!(
            counters("low 0\noom_kill 3\nmax 7\nfuture_counter 9\noom 4\n"),
            Some([7, 4, 3])
        );
        for text in [
            "",
            "max 1\noom 1\n",
            "max 1\noom 1\noom_kill 1\noom_kill 2\n",
            "max 1\noom 1\noom_kill -1\n",
            "max 1\noom 1\noom_kill +1\n",
            "max 1\noom 1\noom_kill invalid\n",
            "max 1\noom 1\noom_kill 18446744073709551616\n",
            "max 1\noom 1\noom_kill 1 extra\n",
        ] {
            assert_eq!(counters(text), None);
        }
    }

    #[test]
    fn stale_pressure_only_or_global_kill_counters_are_not_local_limit_evidence() {
        let before = [7, 4, 3];
        assert!(local_oom(before, [8, 5, 4]));
        for after in [
            before,
            [8, 4, 3],
            [7, 5, 3],
            [8, 5, 3],
            [7, 4, 4],
            [6, 5, 4],
            [8, 3, 4],
            [8, 5, 2],
        ] {
            assert!(!local_oom(before, after));
        }
    }

    #[test]
    fn unavailable_or_oversized_kernel_reads_are_not_evidence() {
        struct Unavailable;
        impl Read for Unavailable {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("test-owned unavailable reader"))
            }
        }
        assert!(read_text(&mut Unavailable).is_none());
        assert!(read_text(&mut &vec![b'0'; 4097][..]).is_none());
        assert!(read_text(&mut &[0xff][..]).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn failed_child_requires_fresh_original_file_evidence() {
        use std::os::unix::process::ExitStatusExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("memory.events.local");
        for (after, expected) in [
            ("max 8\noom 5\noom_kill 4\n", true),
            ("max 7\noom 4\noom_kill 3\n", false),
            ("max 8\noom 5\noom_kill 3\n", false),
            ("max 7\noom 4\noom_kill 4\n", false),
            ("max 8\noom 5\noom_kill invalid\n", false),
            ("", false),
        ] {
            std::fs::write(&path, "max 7\noom 4\noom_kill 3\n").unwrap();
            let memory = CompilerMemory {
                events: File::open(&path).unwrap(),
                before: [7, 4, 3],
            };
            std::fs::write(&path, after).unwrap();
            let error = CompilerMemory::check(
                ExitStatus::from_raw(1 << 8),
                Some(memory),
                "NATIVE_COMPILATION_FAILED",
            )
            .unwrap_err();
            assert_eq!(error.is::<CompilerMemoryLimit>(), expected);
        }
        for status in [1 << 8, 137 << 8, 9] {
            let error = CompilerMemory::check(
                ExitStatus::from_raw(status),
                None,
                "NATIVE_COMPILATION_FAILED",
            )
            .unwrap_err();
            assert!(!error.is::<CompilerMemoryLimit>());
        }
        assert!(CompilerMemory::check(ExitStatus::from_raw(0), None, "unused").is_ok());
    }
}
