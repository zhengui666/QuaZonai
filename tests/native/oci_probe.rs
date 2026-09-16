//! Native CI probe only, compiled separately by rustc and included only in the
//! explicitly requested test image. No Runtime HTTP operation can select this binary.
use std::{
    fs,
    io::Write,
    net::{SocketAddr, TcpStream},
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
    time::Duration,
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

fn memory_limit() {
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
        Some("pids") => pids_limit(),
        Some("output") => output_limit(),
        Some("sleep") => std::thread::sleep(Duration::from_secs(120)),
        _ => std::process::exit(2),
    }
}
