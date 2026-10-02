//! Best-effort host observation of one test-owned container's original cgroup.
//! This never changes or classifies the job and is not an acceptance oracle.
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub max: u64,
    pub oom: u64,
    pub oom_kill: u64,
    pub memory_peak: Option<u64>,
    pub memory_limit: Option<u64>,
    pub swap_limit: Option<u64>,
    pub pids_max_events: Option<u64>,
    pub pids_limit: Option<u64>,
}

#[derive(Debug, Default)]
pub struct Observation {
    pub first: Option<Sample>,
    pub last: Option<Sample>,
    pub samples: u64,
    pub deadline_elapsed: bool,
    pub stopped_reason: &'static str,
    pub last_unavailable_reason: Option<&'static str>,
}

fn number(text: &str) -> Option<u64> {
    let text = text.trim();
    (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

fn counters<const N: usize>(text: &str, names: [&str; N]) -> Option<[u64; N]> {
    let mut counters = [None; N];
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let name = fields.next()?;
        let value = number(fields.next()?)?;
        if fields.next().is_some() {
            return None;
        }
        let Some(index) = names.iter().position(|expected| *expected == name) else {
            continue;
        };
        if counters[index].replace(value).is_some() {
            return None;
        }
    }
    let mut values = [0; N];
    for (value, counter) in values.iter_mut().zip(counters) {
        *value = counter?;
    }
    Some(values)
}

fn read(path: &Path) -> Option<String> {
    // Only fixed files below the host's kernel-owned cgroup hierarchy are read.
    // Kernel pseudo-files report length zero, so bound the actual read instead.
    let mut text = String::new();
    File::open(path)
        .ok()?
        .take(4097)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() <= 4096).then_some(text)
}

fn sample(directory: &Path) -> Result<Sample, &'static str> {
    let [max, oom, oom_kill] = counters(
        &read(&directory.join("memory.events.local")).ok_or("memory_events_unavailable")?,
        ["max", "oom", "oom_kill"],
    )
    .ok_or("memory_events_invalid")?;
    let value = |name| read(&directory.join(name)).and_then(|text| number(&text));
    Ok(Sample {
        max,
        oom,
        oom_kill,
        // A disappearing optional file must not discard already-read counters.
        memory_peak: value("memory.peak"),
        memory_limit: value("memory.max"),
        swap_limit: value("memory.swap.max"),
        pids_max_events: read(&directory.join("pids.events"))
            .and_then(|text| counters(&text, ["max"]))
            .map(|values| values[0]),
        pids_limit: value("pids.max"),
    })
}

fn directory(id: &str, membership: &str) -> Option<PathBuf> {
    if id.len() != 64
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    // Require the live Docker PID to name this exact container's cgroup v2
    // membership. Custom parents/v1 remain unavailable, without a host search.
    let path = membership.strip_prefix("0::")?.trim_end_matches('\n');
    if path != format!("/system.slice/docker-{id}.scope") && path != format!("/docker/{id}") {
        return None;
    }
    Some(PathBuf::from("/sys/fs/cgroup").join(path.strip_prefix('/')?))
}

fn process_start(stat: &str, pid: i64) -> Option<u64> {
    if stat.split_once(' ')?.0 != pid.to_string() {
        return None;
    }
    // comm is parenthesized and may itself contain spaces or parentheses.
    let fields = stat.rsplit_once(") ")?.1;
    number(fields.split_whitespace().nth(19)?)
}

#[derive(Clone)]
struct Identity {
    pid: i64,
    started: u64,
    membership: String,
}

impl Identity {
    fn current(&self) -> bool {
        read(&PathBuf::from(format!("/proc/{}/stat", self.pid)))
            .and_then(|stat| process_start(&stat, self.pid))
            == Some(self.started)
            && read(&PathBuf::from(format!("/proc/{}/cgroup", self.pid))).as_ref()
                == Some(&self.membership)
    }
}

pub struct Observer {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Observation>>,
}

impl Observer {
    pub fn start(id: &str, pid: i64, deadline: Instant) -> Result<Self, &'static str> {
        if pid <= 0 {
            return Err("no_live_pid");
        }
        let membership = read(&PathBuf::from(format!("/proc/{pid}/cgroup")))
            .ok_or("pid_membership_unavailable")?;
        let directory = directory(id, &membership).ok_or("cgroup_identity_unavailable")?;
        let started = read(&PathBuf::from(format!("/proc/{pid}/stat")))
            .and_then(|stat| process_start(&stat, pid))
            .ok_or("pid_start_unavailable")?;
        let identity = Identity {
            pid,
            started,
            membership,
        };
        if !identity.current() {
            return Err("pid_identity_changed");
        }
        Self::watch(directory, identity, deadline)
    }

    fn watch(
        directory: PathBuf,
        identity: Identity,
        deadline: Instant,
    ) -> Result<Self, &'static str> {
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("owned-compiler-memory".into())
            .spawn(move || {
                let mut observed = Observation::default();
                while !cancelled.load(Ordering::Relaxed) && Instant::now() < deadline {
                    if !identity.current() {
                        observed.stopped_reason = "pid_identity_lost";
                        break;
                    }
                    let current = sample(&directory);
                    if !identity.current() {
                        observed.stopped_reason = "pid_identity_lost";
                        break;
                    }
                    match current {
                        Ok(current) => {
                            observed.first.get_or_insert(current);
                            observed.last = Some(current);
                            observed.samples += 1;
                        }
                        Err(reason) => observed.last_unavailable_reason = Some(reason),
                    }
                    thread::sleep(Duration::from_millis(2));
                }
                observed.deadline_elapsed = Instant::now() >= deadline;
                if observed.stopped_reason.is_empty() {
                    observed.stopped_reason = if observed.deadline_elapsed {
                        "deadline_elapsed"
                    } else {
                        "stopped"
                    };
                }
                observed
            })
            .map_err(|_| "observer_start_failed")?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }

    pub fn finish(mut self) -> Option<Observation> {
        self.stop.store(true, Ordering::Relaxed);
        self.worker.take()?.join().ok()
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exact_container_ids_select_fixed_cgroup_paths() {
        let id = "a".repeat(64);
        assert_eq!(
            directory(&id, &format!("0::/system.slice/docker-{id}.scope\n")),
            Some(PathBuf::from(format!(
                "/sys/fs/cgroup/system.slice/docker-{id}.scope"
            )))
        );
        assert_eq!(
            directory(&id, &format!("0::/docker/{id}\n")),
            Some(PathBuf::from(format!("/sys/fs/cgroup/docker/{id}")))
        );
        for invalid in [
            "",
            "../memory.events.local",
            &"a".repeat(63),
            &"A".repeat(64),
            &"g".repeat(64),
        ] {
            assert!(directory(invalid, &format!("0::/docker/{invalid}")).is_none());
        }
        for invalid in [
            format!("0::/docker/{}", "b".repeat(64)),
            format!("0::/docker/{id}/../{id}"),
            format!("0::/custom/docker/{id}"),
            format!("7:memory:/docker/{id}"),
            format!("0::/docker/{id}\n0::/docker/{id}"),
            "0::/".to_owned(),
        ] {
            assert!(directory(&id, &invalid).is_none());
        }
    }

    #[test]
    fn only_complete_unambiguous_unsigned_counters_are_retained() {
        assert_eq!(
            counters(
                "low 0\nmax 38\noom 1\noom_kill 1\n",
                ["max", "oom", "oom_kill"]
            ),
            Some([38, 1, 1])
        );
        for invalid in [
            "",
            "max 0\noom 0",
            "max +1\noom 1\noom_kill 1",
            "max 1\noom -1\noom_kill 1",
            "max 1\noom 1\noom_kill 1\nmax 2",
            "max 1 2\noom 1\noom_kill 1",
            "max 18446744073709551616\noom 1\noom_kill 1",
        ] {
            assert_eq!(counters(invalid, ["max", "oom", "oom_kill"]), None);
        }
        assert_eq!(counters("max 7\n", ["max"]), Some([7]));
        assert_eq!(counters("max 7\nmax 8\n", ["max"]), None);
        assert_eq!(number("max"), None);
        assert_eq!(number("0\n"), Some(0));
    }

    #[test]
    fn bounded_reads_preserve_missing_and_invalid_evidence_as_unavailable() {
        struct Temporary(PathBuf);
        impl Drop for Temporary {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let path =
            std::env::temp_dir().join(format!("qz-compiler-observer-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        let owned = Temporary(path);
        assert_eq!(sample(&owned.0), Err("memory_events_unavailable"));
        let events = owned.0.join("memory.events.local");
        std::fs::write(&events, "max 38\noom 1\noom_kill 1\n").unwrap();
        let observed = sample(&owned.0).unwrap();
        assert_eq!([observed.max, observed.oom, observed.oom_kill], [38, 1, 1]);
        assert_eq!(observed.memory_peak, None);
        assert_eq!(observed.pids_max_events, None);
        std::fs::write(owned.0.join("pids.events"), "max 7\n").unwrap();
        std::fs::write(owned.0.join("memory.max"), "67108864\n").unwrap();
        assert_eq!(sample(&owned.0).unwrap().pids_max_events, Some(7));
        assert_eq!(sample(&owned.0).unwrap().memory_limit, Some(67108864));
        std::fs::write(owned.0.join("pids.events"), "max private\n").unwrap();
        assert_eq!(sample(&owned.0).unwrap().pids_max_events, None);
        std::fs::write(&events, "max 1\noom 1\noom_kill 1\nmax 2").unwrap();
        assert_eq!(sample(&owned.0), Err("memory_events_invalid"));
        std::fs::write(&events, vec![b'0'; 4097]).unwrap();
        assert_eq!(sample(&owned.0), Err("memory_events_unavailable"));
        // Exercise the real sampler's bound and RAII cancellation using owned
        // local files and this process's identity, without a container or load.
        let pid = i64::from(std::process::id());
        let identity = Identity {
            pid,
            started: process_start(
                &read(&PathBuf::from(format!("/proc/{pid}/stat"))).unwrap(),
                pid,
            )
            .unwrap(),
            membership: read(&PathBuf::from(format!("/proc/{pid}/cgroup"))).unwrap(),
        };
        let observed = Observer::watch(owned.0.clone(), identity.clone(), Instant::now())
            .unwrap()
            .finish()
            .unwrap();
        assert!(observed.deadline_elapsed);
        assert_eq!(observed.stopped_reason, "deadline_elapsed");
        assert_eq!(observed.first, None);
        let began = Instant::now();
        drop(
            Observer::watch(
                owned.0.clone(),
                identity,
                Instant::now() + Duration::from_secs(30),
            )
            .unwrap(),
        );
        assert!(began.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn process_identity_uses_pid_and_start_time_after_parenthesized_comm() {
        let fields = std::iter::once("S")
            .chain(std::iter::repeat_n("0", 18))
            .chain(["12345", "999"])
            .collect::<Vec<_>>()
            .join(" ");
        let stat = format!("42 (native (job) name) {fields}");
        assert_eq!(process_start(&stat, 42), Some(12345));
        assert_eq!(process_start(&stat, 43), None);
        assert_eq!(process_start("42 (job) S 0", 42), None);
        assert_eq!(process_start(&stat.replace("12345", "private"), 42), None);
        let id = "a".repeat(64);
        assert!(matches!(
            Observer::start(&id, 0, Instant::now()),
            Err("no_live_pid")
        ));
        // An unrelated live host process cannot authorize cgroup observation.
        assert!(Observer::start(&id, i64::from(std::process::id()), Instant::now()).is_err());
    }
}
