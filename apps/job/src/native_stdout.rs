//! CLI-only process-global stdout redirection for upstream strategies using println!.
//! Never use this guard from a concurrent library call; job executes one command per process.
use anyhow::Result;
use std::io::Write;

#[cfg(unix)]
pub fn with_stderr_logs<T>(run: impl FnOnce() -> Result<T>) -> Result<T> {
    struct Restore(Option<std::os::fd::OwnedFd>);
    impl Restore {
        fn finish(&mut self) -> Result<()> {
            std::io::stdout().flush()?;
            if let Some(fd) = self.0.as_ref() {
                rustix::stdio::dup2_stdout(fd)?;
            }
            self.0 = None;
            Ok(())
        }
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            // Also restore during unwinding; no strategy output may precede the JSON result.
            let _ = self.finish();
        }
    }
    std::io::stdout().flush()?;
    let mut restore = Restore(Some(rustix::io::fcntl_dupfd_cloexec(std::io::stdout(), 3)?));
    rustix::stdio::dup2_stdout(std::io::stderr())?;
    let result = run();
    restore.finish()?;
    result
}

#[cfg(not(unix))]
pub fn with_stderr_logs<T>(_run: impl FnOnce() -> Result<T>) -> Result<T> {
    anyhow::bail!("NATIVE_STRATEGY_CLI_REQUIRES_UNIX_STDIO")
}
