//! Native CI probe only, compiled separately by rustc and included only in the
//! explicitly requested test image. No Runtime HTTP operation can select this binary.
use std::{
    fs,
    io::{self, Write},
    net::{SocketAddr, TcpStream},
    os::unix::{fs::PermissionsExt, process::ExitStatusExt},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

fn inspect(host_sentinel: &str) {
    let status = fs::read_to_string("/proc/self/status").unwrap();
    let field = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .unwrap()
            .trim()
    };
    assert_eq!(field("Uid:"), "65532\t65532\t65532\t65532");
    assert_eq!(field("Gid:"), "65532\t65532\t65532\t65532");
    assert_eq!(field("CapEff:"), "0000000000000000");
    assert_eq!(field("NoNewPrivs:"), "1");
    assert_eq!(field("Seccomp:"), "2");
    assert!(fs::write("/rootfs-native-escape", b"forbidden").is_err());
    assert!(fs::write("/input/native-escape", b"forbidden").is_err());
    assert!(fs::read(host_sentinel).is_err());
    assert!(fs::metadata("/var/run/docker.sock").is_err());
    assert!(std::env::var_os("DATABASE_URL").is_none());
    assert!(std::env::var_os("OPENAI_API_KEY").is_none());
    assert!(std::env::var_os("QUAZONAI_TEST_HOST_SECRET").is_none());
    let address: SocketAddr = "198.51.100.1:9".parse().unwrap();
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_err());
    assert_eq!(
        fs::read("/input/fixture.txt").unwrap(),
        b"native read-only input"
    );
    assert_eq!(
        fs::read_to_string("/sys/fs/cgroup/memory.max")
            .unwrap()
            .trim(),
        "67108864"
    );
    assert_eq!(
        fs::read_to_string("/sys/fs/cgroup/pids.max")
            .unwrap()
            .trim(),
        "64"
    );
    let cpu = fs::read_to_string("/sys/fs/cgroup/cpu.max").unwrap();
    let parts: Vec<u64> = cpu
        .split_whitespace()
        .map(|part| part.parse().unwrap())
        .collect();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0], parts[1]);
    fs::copy("/usr/local/bin/isolation-probe", "/tmp/native-exec-probe").unwrap();
    fs::set_permissions("/tmp/native-exec-probe", fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Command::new("/tmp/native-exec-probe")
        .arg("sleep")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_err());
    fs::write(
        "/output/verified",
        b"native namespace and resource controls verified",
    )
    .unwrap();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MemoryEvents {
    max: u64,
    oom: u64,
    oom_kill: u64,
}

impl MemoryEvents {
    fn parse(text: &str) -> Result<Self, &'static str> {
        let mut counters = [None; 3];
        for line in text.lines() {
            let mut fields = line.split_whitespace();
            let name = fields.next().ok_or("missing counter name")?;
            let value = fields
                .next()
                .ok_or("missing counter value")?
                .parse::<u64>()
                .map_err(|_| "invalid counter value")?;
            if fields.next().is_some() {
                return Err("extra counter fields");
            }
            let index = match name {
                "max" => 0,
                "oom" => 1,
                "oom_kill" => 2,
                _ => continue,
            };
            if counters[index].replace(value).is_some() {
                return Err("duplicate counter");
            }
        }
        Ok(Self {
            max: counters[0].ok_or("missing max counter")?,
            oom: counters[1].ok_or("missing oom counter")?,
            oom_kill: counters[2].ok_or("missing oom_kill counter")?,
        })
    }

    fn proves_limit_kill(self, before: Self, signal: Option<i32>) -> bool {
        // SIGKILL alone also covers cancellation; oom_kill alone can include a
        // host OOM. Require this cgroup to hit its own limit and invoke OOM too.
        signal == Some(9)
            && self.max > before.max
            && self.oom > before.oom
            && self.oom_kill > before.oom_kill
    }
}

fn wait_for_marker(path: &Path, deadline: Instant) -> io::Result<()> {
    loop {
        match fs::metadata(path) {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "probe handshake"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_child(child: &mut Child, deadline: Instant) -> io::Result<ExitStatus> {
    loop {
        let error = match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Ok(None) => io::Error::new(io::ErrorKind::TimedOut, "probe pressure child"),
            Err(error) => error,
        };
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
}

fn memory_limit() {
    let deadline = Instant::now() + Duration::from_secs(15);
    assert_eq!(
        fs::read_to_string("/sys/fs/cgroup/memory.max")
            .unwrap()
            .trim(),
        "67108864"
    );
    assert_eq!(
        fs::read_to_string("/sys/fs/cgroup/memory.swap.max")
            .unwrap()
            .trim(),
        "0"
    );
    // Start pressure only after the host receives Docker's start response.
    wait_for_marker(Path::new("/input/memory-start"), deadline).unwrap();
    let before_text = fs::read_to_string("/sys/fs/cgroup/memory.events.local").unwrap();
    let before = MemoryEvents::parse(&before_text).unwrap();
    let mut child = Command::new("/usr/local/bin/isolation-probe")
        .arg("memory-pressure")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let status = wait_for_child(&mut child, deadline).unwrap();
    let after_text = fs::read_to_string("/sys/fs/cgroup/memory.events.local").unwrap();
    let after = MemoryEvents::parse(&after_text).unwrap();
    fs::write(
        "/output/memory-evidence.pending",
        format!(
            "signal={:?}; before={before:?}; after={after:?}",
            status.signal()
        ),
    )
    .unwrap();
    fs::rename("/output/memory-evidence.pending", "/output/memory-evidence").unwrap();
    assert!(after.proves_limit_kill(before, status.signal()));
    fs::write(
        "/output/memory-verified",
        b"native cgroup memory limit killed the pressure child",
    )
    .unwrap();
    // Keep the cgroup populated until Docker acknowledges its separate OOM
    // event. Deleting an already-exited container cannot recover a lost event.
    wait_for_marker(Path::new("/input/memory-observed"), deadline).unwrap();
    std::process::exit(1);
}

fn memory_pressure() {
    let mut chunks = Vec::new();
    for _ in 0..128 {
        let mut bytes = vec![0u8; 4 * 1024 * 1024];
        for index in (0..bytes.len()).step_by(4096) {
            bytes[index] = 1;
        }
        chunks.push(bytes);
        std::hint::black_box(&chunks);
    }
    // Reaching this point means the native memory limit did not contain the process.
    std::process::exit(99);
}

fn pids_limit() {
    let mut children = Vec::new();
    let mut bounded = false;
    for _ in 0..128 {
        match Command::new("/usr/local/bin/isolation-probe")
            .arg("sleep")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => children.push(child),
            Err(_) => {
                bounded = true;
                break;
            }
        }
    }
    let total = children.len();
    for mut child in children {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert!(bounded && total > 0 && total < 64);
    fs::write("/output/pids-verified", total.to_string()).unwrap();
}

fn output_limit() {
    let mut output = fs::File::create("/output/bounded-file").unwrap();
    for _ in 0..256 {
        if output.write_all(&[0x5a; 65536]).is_err() {
            std::process::exit(42);
        }
    }
    std::process::exit(99);
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        Some("inspect") => inspect(&arguments.next().expect("test-owned host sentinel path")),
        Some("memory") => memory_limit(),
        Some("memory-pressure") => memory_pressure(),
        Some("pids") => pids_limit(),
        Some("output") => output_limit(),
        Some("sleep") => std::thread::sleep(Duration::from_secs(120)),
        _ => std::process::exit(2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_events_require_complete_unambiguous_native_counters() {
        assert_eq!(
            MemoryEvents::parse("oom_kill 2\nlow 0\nmax 7\noom 3\n").unwrap(),
            MemoryEvents {
                max: 7,
                oom: 3,
                oom_kill: 2
            }
        );
        for invalid in [
            "",
            "max 1\noom 1",
            "max 1\noom 1\noom_kill -1",
            "max 1\noom 1\noom_kill 1\nmax 2",
            "max 1 2\noom 1\noom_kill 1",
        ] {
            assert!(MemoryEvents::parse(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn only_fresh_local_limit_oom_and_sigkill_prove_memory_enforcement() {
        let before = MemoryEvents {
            max: 7,
            oom: 3,
            oom_kill: 2,
        };
        let after = MemoryEvents {
            max: 8,
            oom: 4,
            oom_kill: 3,
        };
        assert!(after.proves_limit_kill(before, Some(9)));
        assert!(!after.proves_limit_kill(before, None));
        assert!(!after.proves_limit_kill(before, Some(15)));
        assert!(!before.proves_limit_kill(before, Some(9)));
        assert!(!MemoryEvents {
            max: before.max,
            ..after
        }
        .proves_limit_kill(before, Some(9)));
        assert!(!MemoryEvents {
            oom: before.oom,
            ..after
        }
        .proves_limit_kill(before, Some(9)));
        assert!(!MemoryEvents {
            oom_kill: before.oom_kill,
            ..after
        }
        .proves_limit_kill(before, Some(9)));
        assert!(!before.proves_limit_kill(after, Some(9)));
    }

    #[test]
    fn absent_handshake_is_bounded() {
        let absent =
            std::env::temp_dir().join(format!("native-probe-absent-{}", std::process::id()));
        assert_eq!(
            wait_for_marker(&absent, Instant::now()).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
    }

    #[test]
    fn child_fixture() {
        match std::env::var("QUAZONAI_TEST_PROBE_CHILD").as_deref() {
            Ok("exit-137") => std::process::exit(137),
            Ok("sleep") => std::thread::sleep(Duration::from_secs(30)),
            _ => {}
        }
    }

    fn child_fixture_process(mode: &str) -> Child {
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::child_fixture"])
            .env("QUAZONAI_TEST_PROBE_CHILD", mode)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    #[test]
    fn ordinary_exit_137_cannot_become_an_oom_kill() {
        let mut child = child_fixture_process("exit-137");
        let status = wait_for_child(&mut child, Instant::now() + Duration::from_secs(5)).unwrap();
        assert_eq!(status.code(), Some(137));
        assert_eq!(status.signal(), None);
        let before = MemoryEvents {
            max: 0,
            oom: 0,
            oom_kill: 0,
        };
        let after = MemoryEvents {
            max: 1,
            oom: 1,
            oom_kill: 1,
        };
        assert!(!after.proves_limit_kill(before, status.signal()));
    }

    #[test]
    fn timed_out_child_is_killed_and_reaped_without_oom_evidence() {
        let mut child = child_fixture_process("sleep");
        assert_eq!(
            wait_for_child(&mut child, Instant::now())
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        let status = child.try_wait().unwrap().expect("child must be reaped");
        assert_eq!(status.signal(), Some(9));
        let unchanged = MemoryEvents {
            max: 0,
            oom: 0,
            oom_kill: 0,
        };
        assert!(!unchanged.proves_limit_kill(unchanged, status.signal()));
    }

    #[test]
    fn existing_handshake_is_observed_without_an_arbitrary_delay() {
        let marker =
            std::env::temp_dir().join(format!("native-probe-present-{}", std::process::id()));
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker)
            .unwrap();
        let result = wait_for_marker(&marker, Instant::now());
        drop(file);
        fs::remove_file(marker).unwrap();
        result.unwrap();
    }
}
