//! FreeBSD (and future OpenBSD) service registration via rc.d. Writes an `rc.subr` script to
//! `/usr/local/etc/rc.d/`, enables it in `/etc/rc.conf` via `sysrc`, and starts it. This is the
//! fourth arm of the shared `install` engine.

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

fn script_path(spec: &ServiceSpec) -> PathBuf {
    PathBuf::from("/usr/local/etc/rc.d").join(&spec.service_id)
}

/// `rc.conf` variable name — rc.subr requires `[A-Za-z0-9_]`, so hyphens become underscores.
fn rcvar(spec: &ServiceSpec) -> String {
    spec.service_id.replace('-', "_")
}

fn script_contents(spec: &ServiceSpec) -> String {
    let name = rcvar(spec);
    format!(
        "#!/bin/sh\n\
         #\n\
         # PROVIDE: {name}\n\
         # REQUIRE: NETWORKING SYSLOG\n\
         # KEYWORD: shutdown\n\
         #\n\
         # rc.conf knobs:\n\
         #   {name}_enable (bool):  Set to YES to enable {desc}.\n\
         \n\
         . /etc/rc.subr\n\
         \n\
         name=\"{name}\"\n\
         rcvar=\"{name}_enable\"\n\
         \n\
         load_rc_config \"$name\"\n\
         : ${{{name}_enable:=\"NO\"}}\n\
         \n\
         pidfile=\"/var/run/{svc}.pid\"\n\
         command=\"/usr/sbin/daemon\"\n\
         command_args=\"-P ${{pidfile}} -r -t \\\"{desc}\\\" {bin} --config-dir {config_dir} --log-file {log_file}\"\n\
         \n\
         run_rc_command \"$1\"\n",
        name = name,
        svc = spec.service_id,
        desc = spec.display_name,
        bin = spec.bin_path.display(),
        config_dir = spec.config_dir.display(),
        log_file = spec.log_file.display(),
    )
}

pub fn register_service(spec: &ServiceSpec) -> Result<()> {
    let path = script_path(spec);
    // /usr/local/etc/rc.d may not exist on a fresh FreeBSD (no port has created it yet).
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    std::fs::write(&path, script_contents(spec))
        .with_context(|| format!("Failed to write rc.d script {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .with_context(|| format!("Failed to make {} executable", path.display()))?;
    }

    // Enable in /etc/rc.conf and start.
    run("sysrc", &[&format!("{}_enable=YES", rcvar(spec))])?;
    run("service", &[&spec.service_id, "start"])?;
    Ok(())
}

pub fn restart_service(spec: &ServiceSpec) -> Result<()> {
    run("service", &[&spec.service_id, "restart"])
}

/// Returns whether an rc.d script was present before removal. Stop/disable are best-effort.
pub fn deregister_service(spec: &ServiceSpec) -> Result<bool> {
    let path = script_path(spec);
    let existed = path.exists();

    let _ = Command::new("service")
        .args([&spec.service_id, "stop"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = Command::new("sysrc")
        .args(["-x", &format!("{}_enable", rcvar(spec))])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    if existed {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove rc.d script {}", path.display()))?;
    }

    Ok(existed)
}
