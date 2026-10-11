//! Linux service registration via systemd. Generates a unit with a fully-formed `ExecStart`
//! (no hand-edit), then `daemon-reload` + `enable --now`.

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

fn unit_path(spec: &ServiceSpec) -> PathBuf {
    PathBuf::from("/etc/systemd/system").join(format!("{}.service", spec.service_id))
}

fn unit_contents(spec: &ServiceSpec) -> String {
    format!(
        "[Unit]\n\
         Description={desc}\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={bin} --config-dir {config_dir} --log-file {log_file}\n\
         Restart=always\n\
         RestartSec=10\n\
         User=root\n\
         StandardOutput=journal\n\
         StandardError=journal\n\
         SyslogIdentifier={id}\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        desc = spec.display_name,
        bin = spec.bin_path.display(),
        config_dir = spec.config_dir.display(),
        log_file = spec.log_file.display(),
        id = spec.service_id,
    )
}

pub fn register_service(spec: &ServiceSpec) -> Result<()> {
    let path = unit_path(spec);
    std::fs::write(&path, unit_contents(spec))
        .with_context(|| format!("Failed to write unit file {}", path.display()))?;

    let unit = format!("{}.service", spec.service_id);
    run("systemctl", &["daemon-reload"])?;
    run("systemctl", &["enable", "--now", &unit])?;
    Ok(())
}

pub fn restart_service(spec: &ServiceSpec) -> Result<()> {
    run(
        "systemctl",
        &["restart", &format!("{}.service", spec.service_id)],
    )
}

/// Returns whether a unit was present before removal. Best-effort stop/disable; missing service
/// is not an error (idempotent).
pub fn deregister_service(spec: &ServiceSpec) -> Result<bool> {
    let unit = format!("{}.service", spec.service_id);
    let path = unit_path(spec);
    let existed = path.exists();

    let _ = Command::new("systemctl")
        .args(["disable", "--now", &unit])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    if existed {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove unit file {}", path.display()))?;
        let _ = Command::new("systemctl").args(["daemon-reload"]).status();
    }

    Ok(existed)
}
