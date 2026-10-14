//! The digest's account of a run's warnings and credentials, built from what the run recorded:
//! its `warnings` and `credential_results`, the same two lists its historical record holds.
//!
//! Counts and names only. A credential is named by its stored name and type; nothing from its
//! settings, and nothing from its per-host results beyond how many hosts it worked on.

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::daemon::discovery::types::warnings::{DiscoveryWarning, WarningRemedy};
use crate::server::credentials::r#impl::base::Credential;
use crate::server::credentials::r#impl::mapping::CredentialQueryPayloadDiscriminants;
use crate::server::credentials::r#impl::run_results::{CredentialRunOutcome, CredentialRunResult};
use crate::server::credentials::r#impl::types::CredentialTypeDiscriminants;
use crate::server::credentials::r#impl::types::ssh_script::SshScriptOutcome;
use crate::server::digest::payload::{
    CredentialProblem, IntegrationWithoutCredential, WarningCounts,
};
use crate::server::shared::types::metadata::TypeMetadataProvider;

pub struct RunReport {
    pub warnings: WarningCounts,
    pub credentials_used: u32,
    pub credential_problems: Vec<CredentialProblem>,
    pub integrations_without_credentials: Vec<IntegrationWithoutCredential>,
}

/// Every stored credential the run names, in its results or its warnings, to look up by id.
pub fn referenced_credential_ids(
    warnings: &[DiscoveryWarning],
    results: &[CredentialRunResult],
) -> Vec<Uuid> {
    let mut seen = HashSet::new();
    results
        .iter()
        .map(|r| r.credential_id)
        .chain(warnings.iter().filter_map(DiscoveryWarning::credential_id))
        .filter(|id| seen.insert(*id))
        .collect()
}

/// Count the run's warnings by what they ask of the reader, and list the credentials they name.
///
/// Credentials come in the order the run's Credentials tab lists them
/// (`credentialResultRows`): those with results, then those the run only warned about, then one
/// row per integration for credential warnings that carry no id (sent by daemons that predate
/// it). `credentials` holds the stored rows found for the ids; one missing from it (deleted, or
/// not this organization's) keeps its row with no name.
pub fn run_report(
    warnings: &[DiscoveryWarning],
    results: &[CredentialRunResult],
    credentials: &HashMap<Uuid, Credential>,
) -> RunReport {
    let mut counts = WarningCounts::default();
    let mut integrations_without_credentials: Vec<IntegrationWithoutCredential> = Vec::new();
    for warning in warnings {
        // Grouped by integration and counted once each, below, rather than once per host.
        if let DiscoveryWarning::IntegrationWithoutCredential {
            host_id,
            integration,
        } = warning
        {
            let entry = match integrations_without_credentials
                .iter_mut()
                .find(|e| e.integration == *integration)
            {
                Some(entry) => entry,
                None => {
                    integrations_without_credentials.push(IntegrationWithoutCredential {
                        integration: *integration,
                        host_ids: Vec::new(),
                    });
                    integrations_without_credentials.last_mut().unwrap()
                }
            };
            entry.host_ids.extend(host_id);
            continue;
        }
        let code = warning.code();
        match code.remedy() {
            WarningRemedy::FixInScanopy if code.concerns_credential() => {
                counts.fix_credentials += 1
            }
            WarningRemedy::FixInScanopy => counts.fix_other += 1,
            WarningRemedy::CheckTheDevice => counts.check_device += 1,
            WarningRemedy::ClearsOnTheNextScan | WarningRemedy::NothingToDo => {
                counts.no_action += 1
            }
        }
    }

    // A credential's warnings, keyed by its id, or by integration when the warning carries none.
    let mut by_credential: Vec<(Uuid, Vec<&DiscoveryWarning>)> = Vec::new();
    let mut by_integration: Vec<(CredentialQueryPayloadDiscriminants, u32)> = Vec::new();
    for warning in warnings.iter().filter(|w| w.code().concerns_credential()) {
        match (warning.credential_id(), warning.integration()) {
            (Some(id), _) => match by_credential.iter_mut().find(|(k, _)| *k == id) {
                Some((_, ws)) => ws.push(warning),
                None => by_credential.push((id, vec![warning])),
            },
            (None, Some(integration)) => {
                match by_integration.iter_mut().find(|(k, _)| *k == integration) {
                    Some((_, n)) => *n += 1,
                    None => by_integration.push((integration, 1)),
                }
            }
            (None, None) => {}
        }
    }
    let warnings_of = |id: Uuid| {
        by_credential
            .iter()
            .find(|(k, _)| *k == id)
            .map(|(_, ws)| ws.as_slice())
            .unwrap_or_default()
    };

    let problem = |id: Uuid, outcome: Option<&CredentialRunOutcome>| {
        let ws = warnings_of(id);
        if ws.is_empty() {
            return None;
        }
        let stored = credentials.get(&id);
        let type_label = match stored {
            Some(c) => CredentialTypeDiscriminants::from(&c.base.credential_type)
                .display_name()
                .to_string(),
            None => ws
                .iter()
                .find_map(|w| w.integration())
                .map(|i| i.name().to_string())
                .unwrap_or_default(),
        };
        Some(CredentialProblem {
            name: stored.map(|c| c.base.name.clone()),
            type_label,
            hosts_reached: outcome.map_or(0, hosts_reached),
            warnings: ws.len() as u32,
        })
    };

    let reported: HashSet<Uuid> = results.iter().map(|r| r.credential_id).collect();
    let warned_only: Vec<Uuid> = by_credential
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| !reported.contains(id))
        .collect();

    let mut credential_problems: Vec<CredentialProblem> = results
        .iter()
        .filter_map(|r| problem(r.credential_id, Some(&r.outcome)))
        .chain(warned_only.iter().filter_map(|id| problem(*id, None)))
        .collect();
    credential_problems.extend(
        by_integration
            .iter()
            .map(|(integration, n)| CredentialProblem {
                name: None,
                type_label: integration.name().to_string(),
                hosts_reached: 0,
                warnings: *n,
            }),
    );

    counts.add_credentials = integrations_without_credentials.len() as u32;

    RunReport {
        warnings: counts,
        credentials_used: (results.len() + warned_only.len() + by_integration.len()) as u32,
        credential_problems,
        integrations_without_credentials,
    }
}

/// Hosts a credential worked on: collected from, script applied, or woken.
fn hosts_reached(outcome: &CredentialRunOutcome) -> u32 {
    match outcome {
        CredentialRunOutcome::Collected { hosts } => *hosts,
        CredentialRunOutcome::SshScript { runs } => runs
            .iter()
            .filter(|r| r.outcome == SshScriptOutcome::Applied)
            .count() as u32,
        CredentialRunOutcome::WakeOnLan { hosts } => hosts.iter().filter(|h| h.woke).count() as u32,
        CredentialRunOutcome::Unknown => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::discovery::types::warnings::{CredentialAttempt, SnmpWalkGroup};
    use crate::server::credentials::r#impl::base::CredentialBase;
    use crate::server::credentials::r#impl::types::ssh_script::SshScriptRun;
    use crate::server::credentials::r#impl::types::{
        CredentialIntegration, CredentialType, SecretValue,
    };
    use crate::server::digest::payload::DiscoveryDigestPayload;
    use secrecy::SecretString;

    fn snmp(name: &str, community: &str) -> Credential {
        Credential::new(CredentialBase {
            name: name.to_string(),
            credential_type: CredentialType::SnmpV2c {
                community: SecretValue::Inline {
                    value: SecretString::from(community.to_string()),
                },
            },
            ..Default::default()
        })
    }

    fn rejected(credential_id: Option<Uuid>, detail: &str) -> DiscoveryWarning {
        DiscoveryWarning::CredentialRejected(CredentialAttempt {
            address: "10.0.0.2".parse().unwrap(),
            integration: CredentialQueryPayloadDiscriminants::Snmp,
            credential_id,
            detail: Some(detail.to_string()),
        })
    }

    fn collected(credential_id: Uuid, hosts: u32) -> CredentialRunResult {
        CredentialRunResult {
            credential_id,
            outcome: CredentialRunOutcome::Collected { hosts },
        }
    }

    /// A payload as a server without the warning fields would have queued it.
    fn payload_without_warning_fields() -> DiscoveryDigestPayload {
        serde_json::from_value(serde_json::json!({
            "session_id": Uuid::nil(),
            "site_id": Uuid::nil(),
            "site_name": "HQ",
            "started_at": "2026-10-12T00:00:00Z",
            "finished_at": "2026-10-12T00:10:00Z",
            "stale_after_hours": 24,
            "subnets_scanned": [],
            "hosts_added": [],
            "hosts_stale": [],
            "hosts_changed": [],
            "vlans_added": [],
            "vlans_stale": [],
            "recipients": [],
        }))
        .unwrap()
    }

    fn by_id(credentials: &[&Credential]) -> HashMap<Uuid, Credential> {
        credentials.iter().map(|c| (c.id, (*c).clone())).collect()
    }

    #[test]
    fn warnings_are_counted_by_what_they_ask_of_the_reader() {
        let warnings = vec![
            rejected(Some(Uuid::new_v4()), "auth"),
            DiscoveryWarning::ScanTimeLimit {
                hours: 2,
                hosts_not_scanned: 10,
            },
            DiscoveryWarning::SnmpCollectedNothing {
                address: "10.0.0.3".parse().unwrap(),
            },
            DiscoveryWarning::ReverseDnsTimedOut { count: 4 },
            DiscoveryWarning::SnmpWalkUnsupported {
                address: "10.0.0.4".parse().unwrap(),
                group: SnmpWalkGroup::Lldp,
            },
        ];
        let report = run_report(&warnings, &[], &HashMap::new());
        assert_eq!(
            report.warnings,
            WarningCounts {
                fix_credentials: 1,
                fix_other: 1,
                check_device: 1,
                no_action: 2,
                add_credentials: 0,
            }
        );
        assert_eq!(report.warnings.actionable(), 3);
    }

    /// Forty SNMP hosts and one Proxmox host are two things to fix, each naming its hosts.
    #[test]
    fn integrations_without_credentials_count_once_per_integration() {
        let without = |integration| DiscoveryWarning::IntegrationWithoutCredential {
            host_id: Some(Uuid::new_v4()),
            integration,
        };
        let mut warnings: Vec<DiscoveryWarning> = (0..40)
            .map(|_| without(CredentialIntegration::Snmp))
            .collect();
        warnings.push(without(CredentialIntegration::Proxmox));

        let report = run_report(&warnings, &[], &HashMap::new());
        assert_eq!(report.warnings.add_credentials, 2);
        assert_eq!(report.warnings.actionable(), 2);
        let hosts: Vec<(CredentialIntegration, usize)> = report
            .integrations_without_credentials
            .iter()
            .map(|e| (e.integration, e.host_ids.len()))
            .collect();
        assert_eq!(
            hosts,
            vec![
                (CredentialIntegration::Snmp, 40),
                (CredentialIntegration::Proxmox, 1)
            ]
        );
    }

    #[test]
    fn only_credentials_with_warnings_are_listed_and_every_one_is_counted() {
        let working = snmp("Core switches", "c0re");
        let partial = snmp("Branch switches", "br4nch");
        let failing = snmp("Lab", "l4b");
        let warnings = vec![
            rejected(Some(partial.id), "auth"),
            rejected(Some(failing.id), "auth"),
            rejected(Some(failing.id), "auth"),
            rejected(None, "auth"),
        ];
        let results = vec![collected(working.id, 12), collected(partial.id, 3)];
        let report = run_report(&warnings, &results, &by_id(&[&working, &partial, &failing]));

        // Two with results, one only warned about, one id-less integration group.
        assert_eq!(report.credentials_used, 4);
        let listed: Vec<(Option<&str>, u32, u32)> = report
            .credential_problems
            .iter()
            .map(|p| (p.name.as_deref(), p.hosts_reached, p.warnings))
            .collect();
        assert_eq!(
            listed,
            vec![
                (Some("Branch switches"), 3, 1),
                (Some("Lab"), 0, 2),
                (None, 0, 1),
            ],
            "results first, then warned-only, then id-less; a working credential is not listed"
        );
        assert!(
            report
                .credential_problems
                .iter()
                .all(|p| !p.type_label.is_empty())
        );
    }

    #[test]
    fn a_credential_missing_from_the_lookup_keeps_its_row_without_a_name() {
        let foreign = Uuid::new_v4();
        let report = run_report(&[rejected(Some(foreign), "auth")], &[], &HashMap::new());
        assert_eq!(report.credential_problems.len(), 1);
        assert_eq!(report.credential_problems[0].name, None);
        assert!(!report.credential_problems[0].type_label.is_empty());
    }

    #[test]
    fn ssh_hosts_reached_counts_only_applied_runs() {
        let id = Uuid::new_v4();
        let run = |outcome| SshScriptRun {
            ip: "10.0.0.9".parse().unwrap(),
            outcome,
            exit_code: None,
            applied_keys: vec![],
            rejected_keys: vec![],
            detail: None,
            duration_ms: 1,
        };
        let outcome = CredentialRunOutcome::SshScript {
            runs: vec![
                run(SshScriptOutcome::Applied),
                run(SshScriptOutcome::NonZeroExit),
                run(SshScriptOutcome::Applied),
            ],
        };
        let warnings = vec![DiscoveryWarning::CredentialTimedOut(CredentialAttempt {
            address: "10.0.0.10".parse().unwrap(),
            integration: CredentialQueryPayloadDiscriminants::Ssh,
            credential_id: Some(id),
            detail: None,
        })];
        let report = run_report(
            &warnings,
            &[CredentialRunResult {
                credential_id: id,
                outcome,
            }],
            &HashMap::new(),
        );
        assert_eq!(report.credential_problems[0].hosts_reached, 2);
    }

    /// The digest goes to every user with site access by email. Whatever the run and the stored
    /// credential carry beyond a name and a type must not reach it.
    #[test]
    fn no_secret_or_diagnostic_reaches_the_payload() {
        let cred = snmp("Core switches", "s3cret-community");
        let ssh_id = Uuid::new_v4();
        let warnings = vec![rejected(Some(cred.id), "denied for s3cret-community")];
        let results = vec![CredentialRunResult {
            credential_id: ssh_id,
            outcome: CredentialRunOutcome::SshScript {
                runs: vec![SshScriptRun {
                    ip: "10.9.9.9".parse().unwrap(),
                    outcome: SshScriptOutcome::NonZeroExit,
                    exit_code: Some(1),
                    applied_keys: vec!["serial_number".to_string()],
                    rejected_keys: vec![],
                    detail: Some("/home/admin/.ssh/id_ed25519: permission denied".to_string()),
                    duration_ms: 5,
                }],
            },
        }];
        let report = run_report(&warnings, &results, &by_id(&[&cred]));
        let payload = DiscoveryDigestPayload {
            credentials_used: report.credentials_used,
            credential_problems: report.credential_problems,
            warnings: report.warnings,
            ..payload_without_warning_fields()
        };
        let json = serde_json::to_string(&payload).unwrap();
        for leaked in [
            "s3cret-community",
            "10.0.0.2",
            "10.9.9.9",
            "id_ed25519",
            "serial_number",
        ] {
            assert!(
                !json.contains(leaked),
                "{leaked} reached the digest payload"
            );
        }
        assert!(json.contains("Core switches"));
    }

    /// An event queued by a server without these fields still reads, and still sends on changes
    /// alone.
    #[test]
    fn a_payload_from_before_warnings_were_carried_still_reads() {
        let payload = payload_without_warning_fields();
        assert_eq!(payload.discovery_id, None);
        assert_eq!(payload.warnings, WarningCounts::default());
        assert!(!payload.should_send());
    }

    #[test]
    fn a_run_without_changes_sends_only_for_warnings_the_reader_can_resolve() {
        let with = |warnings: Vec<DiscoveryWarning>| DiscoveryDigestPayload {
            warnings: run_report(&warnings, &[], &HashMap::new()).warnings,
            ..payload_without_warning_fields()
        };
        assert!(
            with(vec![rejected(Some(Uuid::new_v4()), "auth")]).should_send(),
            "a rejected credential is the reader's to fix"
        );
        assert!(
            !with(vec![DiscoveryWarning::ReverseDnsTimedOut { count: 3 }]).should_send(),
            "a warning that clears on its own is not worth an email by itself"
        );
    }
}
