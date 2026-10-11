//! `scanopy-daemon uninstall`: stop and deregister a slot's service, then remove its config, logs
//! and (under `--purge`, once nothing else is installed) the shared binary.

use super::*;

/// Print a uniform per-item disposition line for uninstall, e.g.
/// `Config at /etc/…/config.json: Deleted` or
/// `Binary at /usr/local/bin/scanopy-daemon: Kept (re-run with --purge to delete)`.
fn report_disposition(label: &str, deleted: bool) {
    if deleted {
        println!("{label}: Deleted");
    } else {
        println!("{label}: Kept (re-run with --purge to delete)");
    }
}

pub(super) async fn run_uninstall(args: UninstallArgs) -> Result<()> {
    require_elevation("uninstall")?;

    let installed = installed_daemons();
    let targets = uninstall_targets(&installed, &args)?;

    // `removed_anything`: something was actually deregistered/deleted.
    // `found_anything`: an artifact was present (even if kept). These drive the
    // final summary so a plain uninstall that only finds a kept binary/log
    // doesn't claim "no install found".
    // A selector naming nothing discoverable still gets its paths swept (a half-removed install,
    // service registered but config already gone, is only reachable that way). But if that turns
    // up nothing either, the operator named a daemon that simply isn't here — say so, and say what
    // is, rather than letting the generic summary below suggest `--purge` at them.
    let unmatched_selector = args
        .name
        .as_deref()
        .filter(|s| !installed.iter().any(|i| i.matches(s)));

    let mut removed_anything = false;
    let mut found_anything = false;
    for slot in &targets {
        let (removed, found) = remove_slot(slot, args.purge)?;
        removed_anything |= removed;
        found_anything |= found;
    }

    if let Some(selector) = unmatched_selector
        && !found_anything
    {
        println!(
            "No daemon matching '{selector}' is installed on this host.{}",
            describe_installed(&installed)
        );
        return Ok(());
    }

    // The binary is shared by every daemon on the host, so it can only go once nothing is left to
    // run it. Kept by default in any case; deleted only under --purge.
    let bin_path = platform::default_bin_dir().join(BINARY_FILE_NAME);
    if bin_path.exists() {
        found_anything = true;
        let remaining = installed_daemons();
        if args.purge && remaining.is_empty() {
            let mut self_retired = None;
            match remove_file_when_released(&bin_path) {
                Ok(()) => {
                    removed_anything = true;
                    report_disposition(&format!("Binary at {}", bin_path.display()), true);
                }
                // Windows cannot delete the image a process is executing — and on Windows that
                // process is usually this uninstaller, run from the very path being purged. It
                // does allow the file to be *renamed*, so retire it instead of failing an
                // uninstall that has already removed the service and config. `place_binary` and
                // the sweep below clear a retired binary once nothing holds it.
                Err(_) => {
                    let retired = retired_binary_path(&bin_path);
                    std::fs::rename(&bin_path, &retired).with_context(|| {
                        format!("Failed to delete binary {}", bin_path.display())
                    })?;
                    removed_anything = true;
                    println!(
                        "Binary at {}: Removed (retired to {}, which this running uninstaller \
                         cannot delete; the next install clears it)",
                        bin_path.display(),
                        retired.display()
                    );
                    self_retired = Some(retired);
                }
            }
            // Binaries retired by earlier installs and upgrades go too, once nothing runs them.
            for retired in remove_retired_binaries(&bin_path) {
                if Some(&retired) != self_retired.as_ref() {
                    println!(
                        "Retired binary at {}: still in use — safe to delete once that process \
                         exits.",
                        retired.display()
                    );
                }
            }
        } else if args.purge {
            println!(
                "Binary at {}: Kept ({} other daemon(s) still installed)",
                bin_path.display(),
                remaining.len()
            );
        } else {
            report_disposition(&format!("Binary at {}", bin_path.display()), false);
        }
    }

    if removed_anything {
        println!("Scanopy daemon uninstalled.");
    } else if found_anything {
        println!("Nothing removed — re-run with --purge to delete the kept file(s).");
    } else {
        println!("Nothing to remove — no Scanopy daemon install found.");
    }
    Ok(())
}

/// Which slots an `uninstall` invocation should remove.
///
/// A selector that matches nothing discoverable still resolves to itself, so a half-removed
/// install (service registered, config already deleted) stays removable by name. With no selector
/// and nothing installed, the default slot is returned so the command reports on the standard
/// paths rather than claiming there was nothing to look at.
pub(super) fn uninstall_targets(
    installed: &[Installed],
    args: &UninstallArgs,
) -> Result<Vec<String>> {
    if args.all {
        return Ok(installed.iter().map(|i| i.slot.clone()).collect());
    }

    if let Some(selector) = &args.name {
        return Ok(vec![
            installed
                .iter()
                .find(|i| i.matches(selector))
                .map(|i| i.slot.clone())
                .unwrap_or_else(|| selector.clone()),
        ]);
    }

    match installed.len() {
        0 => Ok(vec![DEFAULT_NAME.to_string()]),
        1 => Ok(vec![installed[0].slot.clone()]),
        _ => Ok(vec![choose_uninstall_target(installed)?]),
    }
}

/// Ask which daemon to remove when several are installed. Never guesses: with no terminal to ask
/// on, it lists them and stops.
fn choose_uninstall_target(installed: &[Installed]) -> Result<String> {
    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "This host has {} Scanopy daemons installed — say which to remove with \
             `--name <name>`, or remove them all with `--all`.{}",
            installed.len(),
            describe_installed(installed)
        );
    }

    println!("This host has several Scanopy daemons installed:\n");
    for (index, entry) in installed.iter().enumerate() {
        println!(
            "  [{}] {} (service {})",
            index + 1,
            entry.label(),
            entry.service_id()
        );
    }
    print!("\nWhich do you want to remove? [1-{}] ", installed.len());
    std::io::stdout().flush().ok();

    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .context("Failed to read your choice")?;
    let answer = answer.trim();
    let choice: usize = answer
        .parse()
        .ok()
        .filter(|c| (1..=installed.len()).contains(c))
        .with_context(|| {
            format!(
                "'{answer}' is not one of the choices (1-{})",
                installed.len()
            )
        })?;
    Ok(installed[choice - 1].slot.clone())
}

/// Remove one slot's service, config and logs. Returns `(removed_anything, found_anything)`.
fn remove_slot(slot: &str, purge: bool) -> Result<(bool, bool)> {
    let mut removed_anything = false;
    let mut found_anything = false;

    // 1. Stop + deregister the service (tolerant of an already-absent service).
    //    Always removed in both modes.
    let spec = system_service_spec(slot);
    let config_dir = spec.config_dir.clone();
    if platform::deregister_service(&spec).context("Failed to remove the system service")? {
        removed_anything = true;
        found_anything = true;
        println!("Service '{}': Removed", spec.service_id);
    } else {
        println!("Service '{}': Not found", spec.service_id);
    }

    // 2. Remove config.json from both the system location (service installs) and the per-user
    //    profile location (--no-service / manual installs). Always removed in both modes.
    let system_cfg = config_dir.join("config.json");
    let profile_cfg = AppConfig::get_config_path_for_name(Some(slot), None)?.1;
    for config_path in [system_cfg, profile_cfg] {
        if config_path.exists() {
            std::fs::remove_file(&config_path)
                .with_context(|| format!("Failed to delete config {}", config_path.display()))?;
            removed_anything = true;
            found_anything = true;
            report_disposition(&format!("Config at {}", config_path.display()), true);
            // Take the slot directory with it once it holds nothing else, so removing a daemon
            // doesn't leave its slot behind as an empty husk. `remove_dir` only succeeds on an
            // empty directory, which is exactly the condition wanted — anything the operator put
            // there keeps the directory.
            if let Some(slot_dir) = config_path.parent() {
                let _ = std::fs::remove_dir(slot_dir);
            }
        }
    }

    // 3. Log files. The service logs to a known, installer-baked path
    //    (`--log-file`, always `default_system_log_path` == `spec.log_file`);
    //    macOS also has the launchd stdout/stderr capture. Kept by default
    //    (useful for a post-mortem), deleted only under --purge.
    let log_files: Vec<std::path::PathBuf> = [
        spec.log_file.clone(),
        // macOS also captures the launchd stdout log; cfg'd on the element so the
        // list is single-entry on other platforms without an unused `mut`/`vec!`.
        #[cfg(target_os = "macos")]
        std::path::PathBuf::from("/var/log/scanopy").join(format!("{}.out.log", spec.service_id)),
    ]
    .into_iter()
    .collect();
    for log_path in log_files.iter().filter(|p| p.exists()) {
        found_anything = true;
        if purge {
            std::fs::remove_file(log_path)
                .with_context(|| format!("Failed to delete log {}", log_path.display()))?;
            removed_anything = true;
        }
        report_disposition(&format!("Log at {}", log_path.display()), purge);
    }

    Ok((removed_anything, found_anything))
}
