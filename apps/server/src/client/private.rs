//! Private CLI files use native owner permissions on Unix and Windows.
use super::{Failure, Result};
use std::{fs, path::Path};

pub(super) fn directory(path: &Path) -> Result<()> {
    if !path.try_exists().map_err(|_| Failure::Configuration)? {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path).map_err(|_| Failure::Configuration)?;
        #[cfg(windows)]
        restrict(path, true)?;
    }
    if !fs::symlink_metadata(path)
        .map_err(|_| Failure::Configuration)?
        .is_dir()
    {
        return Err(Failure::Credential);
    }
    check(path)
}

pub(super) fn check(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path)
            .map_err(|_| Failure::Configuration)?
            .permissions()
            .mode()
            & 0o077
            != 0
        {
            return Err(Failure::Credential);
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        windows(path, false, false)
    }
}

#[cfg(any(test, windows))]
pub(super) fn restrict(path: &Path, directory: bool) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
        )
        .map_err(|_| Failure::Configuration)
    }
    #[cfg(windows)]
    {
        windows(path, true, directory)
    }
}

#[cfg(windows)]
fn windows(path: &Path, restrict: bool, directory: bool) -> Result<()> {
    use std::process::{Command, Stdio};
    // Only the file path and mode cross the process boundary; credentials never do.
    // A protected DACL inherits owner-only access into new profile files.
    let script = r#"
$ErrorActionPreference = 'Stop'
try {
    $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
    $path = $env:QUAZONAI_PRIVATE_PATH
    if ($env:QUAZONAI_PRIVATE_RESTRICT -eq 'true') {
        if ($env:QUAZONAI_PRIVATE_DIRECTORY -eq 'true') {
            $acl = New-Object System.Security.AccessControl.DirectorySecurity
            $flags = 'OICI'
        } else {
            $acl = New-Object System.Security.AccessControl.FileSecurity
            $flags = ''
        }
        $acl.SetSecurityDescriptorSddlForm("O:$($sid.Value)D:P(A;$flags;FA;;;$($sid.Value))")
        Set-Acl -LiteralPath $path -AclObject $acl
    }
    $acl = Get-Acl -LiteralPath $path
    if ($acl.GetOwner([System.Security.Principal.SecurityIdentifier]).Value -ne $sid.Value) { exit 1 }
    $ownerAllowed = $false
    foreach ($rule in $acl.GetAccessRules($true, $true, [System.Security.Principal.SecurityIdentifier])) {
        if ($rule.AccessControlType -eq 'Allow') {
            if ($rule.IdentityReference.Value -ne $sid.Value) { exit 1 }
            $ownerAllowed = $true
        }
    }
    if (-not $ownerAllowed) { exit 1 }
    exit 0
} catch { exit 1 }
"#;
    let system_root = std::env::var_os("SystemRoot").ok_or(Failure::Configuration)?;
    let executable = Path::new(&system_root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let status = Command::new(executable)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .env("QUAZONAI_PRIVATE_PATH", path)
        .env("QUAZONAI_PRIVATE_RESTRICT", restrict.to_string())
        .env("QUAZONAI_PRIVATE_DIRECTORY", directory.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| Failure::Configuration)?;
    if status.success() {
        Ok(())
    } else {
        Err(Failure::Credential)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn windows_rejects_a_credential_readable_by_another_identity() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("private-credential");
        fs::write(&path, "disposable fixture").unwrap();
        restrict(&path, false).unwrap();
        check(&path).unwrap();
        let system_root = std::env::var_os("SystemRoot").unwrap();
        let output =
            std::process::Command::new(Path::new(&system_root).join("System32/icacls.exe"))
                .arg(&path)
                .args(["/grant", "*S-1-1-0:(R)"])
                .output()
                .unwrap();
        assert!(output.status.success());
        assert!(matches!(check(&path), Err(Failure::Credential)));
    }
}
