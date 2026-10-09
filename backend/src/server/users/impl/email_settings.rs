use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::server::email::messages::EmailCategory;

/// Per-user toggles for the pausable email categories. Each field is named
/// after one [`EmailCategory`] whose `pausable()` is true; required
/// categories are never gated here.
///
/// Stored as a JSONB blob, so new categories are added as new fields rather
/// than via migration. New fields carry `#[serde(default = "default_true")]`
/// so a category is opted in by default if its key is absent from the stored
/// JSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, ToSchema)]
pub struct EmailSettings {
    /// Send a periodic summary of what discovery found.
    pub discovery_digest: bool,
    /// Send getting-started guidance.
    #[serde(default = "default_true")]
    pub product_onboarding: bool,
    /// Send an alert when a daemon stops reporting.
    #[serde(default = "default_true")]
    pub daemon_alerts: bool,
    /// Send trial reminders and plan-usage warnings.
    #[serde(default = "default_true")]
    pub trial_and_usage: bool,
}

fn default_true() -> bool {
    true
}

impl Default for EmailSettings {
    fn default() -> Self {
        Self {
            discovery_digest: true,
            product_onboarding: true,
            daemon_alerts: true,
            trial_and_usage: true,
        }
    }
}

impl EmailSettings {
    /// Whether an email in `category` may be delivered to this user.
    /// Required categories always send; pausable ones project onto the
    /// matching flag. The exhaustive match means a new [`EmailCategory`]
    /// will not compile until it is placed here.
    pub fn allows(&self, category: EmailCategory) -> bool {
        match category {
            EmailCategory::Auth
            | EmailCategory::Account
            | EmailCategory::Billing
            | EmailCategory::DaemonSetup
            | EmailCategory::Invitation => true,
            EmailCategory::DiscoveryDigest => self.discovery_digest,
            EmailCategory::ProductOnboarding => self.product_onboarding,
            EmailCategory::DaemonAlerts => self.daemon_alerts,
            EmailCategory::TrialAndUsage => self.trial_and_usage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::shared::types::metadata::HasId;
    use std::collections::BTreeSet;
    use strum::IntoEnumIterator;

    fn settings_keys(settings: &EmailSettings) -> BTreeSet<String> {
        serde_json::to_value(settings)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    /// Settings → Email looks a toggle's metadata up by its field name, so
    /// the pausable category ids and the stored keys have to be the same set.
    #[test]
    fn pausable_category_ids_match_settings_fields() {
        let ids: BTreeSet<String> = EmailCategory::iter()
            .filter(|c| c.pausable())
            .map(|c| c.id().to_string())
            .collect();
        assert_eq!(settings_keys(&EmailSettings::default()), ids);
    }

    /// Switching one toggle off stops exactly its own category, and no
    /// toggle stops a required one.
    #[test]
    fn each_flag_gates_only_its_own_category() {
        for field in settings_keys(&EmailSettings::default()) {
            let mut value = serde_json::to_value(EmailSettings::default()).unwrap();
            value[&field] = serde_json::Value::Bool(false);
            let settings: EmailSettings = serde_json::from_value(value).unwrap();
            for category in EmailCategory::iter() {
                assert_eq!(
                    settings.allows(category),
                    category.id() != field,
                    "{field} off, {} allowed?",
                    category.id()
                );
            }
        }
    }
}
