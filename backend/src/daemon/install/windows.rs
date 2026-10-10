//! Windows service registration via the Service Control Manager (`sc.exe`). **New capability** —
//! Windows previously had no background-service story.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::{ServiceSpec, run};

/// `net session` succeeds only for administrators — a dependency-free elevation probe.
pub fn is_elevated() -> bool {
    Command::new("net")
        .args(["session"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn elevation_hint() -> String {
    format!(
        "This command must be run as Administrator. Re-open your terminal with \
         \"Run as administrator\" and re-run:\n  {}",
        super::current_invocation()
    )
}

pub fn default_bin_dir() -> PathBuf {
    let program_files =
        std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".to_string());
    PathBuf::from(program_files).join("Scanopy")
}

pub fn register_service(spec: &ServiceSpec) -> Result<()> {
    // The service reads secrets from config.json. It runs as LocalSystem (a different profile than
    // the installing user), so --config-dir/--log-file point it at the system paths the installer
    // wrote — %APPDATA% would otherwise resolve to LocalSystem's profile, not the user's.
    // `--service` tells the launched binary the SCM started it, so it runs under the service
    // control dispatcher (reports RUNNING/STOPPED) instead of as a plain console process — without
    // it, `sc start` times out with error 1053.
    let bin_path_value = format!(
        "\"{}\" --service --config-dir \"{}\" --log-file \"{}\"",
        spec.bin_path.display(),
        spec.config_dir.display(),
        spec.log_file.display(),
    );

    // Idempotent: remove any prior registration first. Silence the best-effort stop/delete —
    // on a fresh install they print "[SC] OpenService FAILED 1060" (service does not exist),
    // which is expected here and only confuses users.
    let _ = Command::new("sc")
        .args(["stop", &spec.service_id])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = Command::new("sc")
        .args(["delete", &spec.service_id])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    // `sc create` requires a space after each `key=`, so the value is a separate argument.
    run(
        "sc",
        &[
            "create",
            &spec.service_id,
            "binPath=",
            &bin_path_value,
            "start=",
            "auto",
            "DisplayName=",
            &spec.display_name,
        ],
    )?;
    run("sc", &["start", &spec.service_id])?;
    Ok(())
}

/// How long [`restart_service`] waits for the service to report STOPPED.
const STOP_TIMEOUT: Duration = Duration::from_secs(30);
const STOP_POLL: Duration = Duration::from_millis(500);

/// Stop the service, wait until the SCM reports it stopped, then start it. `sc stop` returns
/// before the service has stopped, and `sc start` on a service still stopping fails, so the wait
/// is what makes this a restart.
pub fn restart_service(spec: &ServiceSpec) -> Result<()> {
    // Fails harmlessly when the service is already stopped; a missing service fails `sc start`.
    let _ = Command::new("sc")
        .args(["stop", &spec.service_id])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let deadline = Instant::now() + STOP_TIMEOUT;
    loop {
        let query = Command::new("sc")
            .args(["query", &spec.service_id])
            .output()
            .context("Failed to execute `sc query`")?;
        if String::from_utf8_lossy(&query.stdout).contains("STOPPED") || !query.status.success() {
            break;
        }
        if Instant::now() >= deadline {
            anyhow::bail!(
                "Service '{}' did not stop within {} seconds",
                spec.service_id,
                STOP_TIMEOUT.as_secs()
            );
        }
        std::thread::sleep(STOP_POLL);
    }

    run("sc", &["start", &spec.service_id])
}

/// Returns whether the service existed before removal. Stop/delete are best-effort (idempotent).
pub fn deregister_service(spec: &ServiceSpec) -> Result<bool> {
    let existed = Command::new("sc")
        .args(["query", &spec.service_id])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let _ = Command::new("sc")
        .args(["stop", &spec.service_id])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = Command::new("sc")
        .args(["delete", &spec.service_id])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    Ok(existed)
}
