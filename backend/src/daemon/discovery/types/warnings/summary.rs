//! How one run's warnings break down by what they ask of the reader.
//!
//! Anything that reports on a run without listing its warnings (the homepage, the discovery
//! digest) counts from here, so "needs attention" means the same thing on every surface.

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use utoipa::ToSchema;

use super::{DiscoveryWarning, WarningRemedy};

/// The number of a run's warnings on one [`WarningRemedy`] rung.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RemedyCount {
    /// The rung these warnings sit on.
    pub remedy: WarningRemedy,
    /// Warnings on that rung.
    pub count: u32,
}

/// One run's warnings, counted by remedy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RunWarningSummary {
    /// Counts per remedy in [`WarningRemedy`] declaration order. Rungs with no warnings are left
    /// out.
    pub by_remedy: Vec<RemedyCount>,
    /// Warnings whose remedy asks the reader to act ([`WarningRemedy::is_actionable`]).
    pub actionable: u32,
    /// Every warning the run recorded.
    pub total: u32,
}

impl From<&[DiscoveryWarning]> for RunWarningSummary {
    fn from(warnings: &[DiscoveryWarning]) -> Self {
        let by_remedy: Vec<RemedyCount> = WarningRemedy::iter()
            .map(|remedy| RemedyCount {
                remedy,
                count: warnings
                    .iter()
                    .filter(|w| w.code().remedy() == remedy)
                    .count() as u32,
            })
            .filter(|c| c.count > 0)
            .collect();
        let actionable = by_remedy
            .iter()
            .filter(|c| c.remedy.is_actionable())
            .map(|c| c.count)
            .sum();

        Self {
            by_remedy,
            actionable,
            total: warnings.len() as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::*;
    use crate::daemon::discovery::types::warnings::SnmpWalkGroup;
    use crate::server::credentials::r#impl::mapping::CredentialQueryPayloadDiscriminants;

    fn addr(last: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, last))
    }

    #[test]
    fn counts_each_rung_and_only_the_actionable_ones_as_work() {
        let warnings = vec![
            DiscoveryWarning::CredentialTargetNotResponding {
                address: addr(1),
                integration: CredentialQueryPayloadDiscriminants::Snmp,
                credential_id: None,
            },
            DiscoveryWarning::CredentialTargetNotResponding {
                address: addr(2),
                integration: CredentialQueryPayloadDiscriminants::Snmp,
                credential_id: None,
            },
            DiscoveryWarning::SnmpWalkPartialDiscarded {
                address: addr(3),
                group: SnmpWalkGroup::ArpTable,
            },
            DiscoveryWarning::SnmpWalkUnsupported {
                address: addr(4),
                group: SnmpWalkGroup::BridgePortNumbering,
            },
        ];

        let summary = RunWarningSummary::from(warnings.as_slice());

        assert_eq!(summary.total, 4);
        assert_eq!(summary.actionable, 2);
        assert_eq!(
            summary.by_remedy,
            vec![
                RemedyCount {
                    remedy: WarningRemedy::FixInScanopy,
                    count: 2
                },
                RemedyCount {
                    remedy: WarningRemedy::ClearsOnTheNextScan,
                    count: 1
                },
                RemedyCount {
                    remedy: WarningRemedy::NothingToDo,
                    count: 1
                },
            ]
        );
    }

    #[test]
    fn a_run_without_warnings_has_an_empty_summary() {
        assert_eq!(
            RunWarningSummary::from(&[][..]),
            RunWarningSummary::default()
        );
    }
}
