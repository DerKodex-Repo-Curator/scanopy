//! macOS service registration via launchd. Writes a LaunchDaemon plist to
//! `/Library/LaunchDaemons/` and bootstraps it into the system domain. **New capability** —
//! macOS previously had no background-service story.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use super::{ServiceSpec, run};

/// `geteuid() == 0`.
pub fn is_elevated() -> bool {
    // SAFETY: `geteuid` takes no arguments and cannot fail.
    unsafe { libc::geteuid() == 0 }
}

pub fn elevation_hint() -> String {
    format!(
        "This command must be run as root.\n  Re-run with: sudo {}",
        super::current_invocation()
    )
}

pub fn default_bin_dir() -> PathBuf {
    PathBuf::from("/usr/local/bin")
}

fn label(spec: &ServiceSpec) -> String {
    super::launchd_label(&spec.slot)
}

fn plist_path(spec: &ServiceSpec) -> PathBuf {
    PathBuf::from("/Library/LaunchDaemons").join(format!("{}.plist", label(spec)))
}

fn plist_contents(spec: &ServiceSpec, log_file: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin}</string>
        <string>--config-dir</string>
        <string>{config_dir}</string>
        <string>--log-file</string>
        <string>{daemon_log}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#,
        label = label(spec),
        bin = spec.bin_path.display(),
        config_dir = spec.config_dir.display(),
        daemon_log = spec.log_file.display(),
        log = log_file,
    )
}

pub fn register_service(spec: &ServiceSpec) -> Result<()> {
    let log_dir = PathBuf::from("/var/log/scanopy");
    std::fs::create_dir_all(&log_dir)
        .with_context(|| format!("Failed to create log directory {}", log_dir.display()))?;
    let log_file = log_dir.join(format!("{}.out.log", spec.service_id));

    let path = plist_path(spec);
    std::fs::write(&path, plist_contents(spec, &log_file.to_string_lossy()))
        .with_context(|| format!("Failed to write plist {}", path.display()))?;

    // Idempotent (re)load: bootout first (ignore if not loaded), then bootstrap + enable.
    // Silence bootout: when nothing is loaded it prints "Boot-out failed: 5: Input/output error",
    // which is expected here and only confuses users during a fresh install.
    let label = label(spec);
    let service_target = format!("system/{label}");
    let _ = Command::new("launchctl")
        .args(["bootout", "system", &path.to_string_lossy()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    run(
        "launchctl",
        &["bootstrap", "system", &path.to_string_lossy()],
    )?;
    let _ = Command::new("launchctl")
        .args(["enable", &service_target])
        .status();

    Ok(())
}

/// Kill and relaunch the loaded job, so it starts from the binary now on disk.
pub fn restart_service(spec: &ServiceSpec) -> Result<()> {
    run(
        "launchctl",
        &["kickstart", "-k", &format!("system/{}", label(spec))],
    )
}

/// Returns whether a plist was present before removal. `bootout` is best-effort (idempotent).
pub fn deregister_service(spec: &ServiceSpec) -> Result<bool> {
    let path = plist_path(spec);
    let existed = path.exists();

    let _ = Command::new("launchctl")
        .args(["bootout", "system", &path.to_string_lossy()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    if existed {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove plist {}", path.display()))?;
    }

    Ok(existed)
}
