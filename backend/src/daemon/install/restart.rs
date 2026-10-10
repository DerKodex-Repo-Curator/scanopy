//! `scanopy-daemon restart`: restart one installed daemon's service, so it runs whatever binary is
//! now on disk.

use super::*;

pub(super) fn run_restart(args: RestartArgs) -> Result<()> {
    require_elevation("restart")?;

    let installed = installed_daemons();
    let target = restart_target(&installed, args.instance.as_deref())?;
    let spec = system_service_spec(&target.slot);
    platform::restart_service(&spec)
        .with_context(|| format!("Failed to restart service '{}'", spec.service_id))?;
    println!(
        "Restarted service '{}' ({}).",
        spec.service_id,
        target.label()
    );

    // On Windows the upgrade command renames the running binary aside before moving the new one
    // in. Clear that copy once nothing runs it; another daemon still on the old image keeps it.
    let _ = remove_file_when_released(&spec.bin_path.with_extension("old"));
    Ok(())
}

/// Which install a `restart` acts on. Unlike `install`, a selector that matches nothing is an
/// error: there is no service to restart, so the operator is shown what is installed instead.
pub(crate) fn restart_target<'a>(
    installed: &'a [Installed],
    selector: Option<&str>,
) -> Result<&'a Installed> {
    if let Some(selector) = selector {
        return installed
            .iter()
            .find(|i| i.matches(selector))
            .with_context(|| {
                format!(
                    "No daemon matching '{selector}' is installed on this host.{}",
                    describe_installed(installed)
                )
            });
    }

    match installed {
        [only] => Ok(only),
        [] => anyhow::bail!("Nothing to restart.{}", describe_installed(installed)),
        _ => anyhow::bail!(
            "This host has {} Scanopy daemons installed — say which to restart with \
             `--instance <name>`.{}",
            installed.len(),
            describe_installed(installed)
        ),
    }
}
