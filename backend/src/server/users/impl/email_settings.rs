use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
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
    /// Send the release notes when this server upgrades.
    #[serde(default = "default_true")]
    pub product_updates: bool,
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
            product_updates: true,
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
            EmailCategory::ProductUpdates => self.product_updates,
        }
    }

    /// Switch one pausable category on or off. Returns whether the setting
    /// changed, so a caller can skip a write that would store the same value,
    /// or `None` for a required category, which has no setting.
    pub fn set(&mut self, category: EmailCategory, enabled: bool) -> Option<bool> {
        let flag = self.flag_mut(category)?;
        let changed = *flag != enabled;
        *flag = enabled;
        Some(changed)
    }

    /// Switch every pausable category off, leaving only required emails.
    /// Returns whether any setting changed.
    pub fn pause_all(&mut self) -> bool {
        EmailCategory::iter()
            .filter(|category| category.pausable())
            .fold(false, |changed, category| {
                self.set(category, false).unwrap_or(false) || changed
            })
    }

    fn flag_mut(&mut self, category: EmailCategory) -> Option<&mut bool> {
        match category {
            EmailCategory::Auth
            | EmailCategory::Account
            | EmailCategory::Billing
            | EmailCategory::DaemonSetup
            | EmailCategory::Invitation => None,
            EmailCategory::DiscoveryDigest => Some(&mut self.discovery_digest),
            EmailCategory::ProductOnboarding => Some(&mut self.product_onboarding),
            EmailCategory::DaemonAlerts => Some(&mut self.daemon_alerts),
            EmailCategory::TrialAndUsage => Some(&mut self.trial_and_usage),
            EmailCategory::ProductUpdates => Some(&mut self.product_updates),
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

    /// The unsubscribe link names one category and turns off that category
    /// alone; a required category has nothing to turn off.
    #[test]
    fn set_turns_off_exactly_the_named_category() {
        for category in EmailCategory::iter() {
            let mut settings = EmailSettings::default();
            if !category.pausable() {
                assert_eq!(settings.set(category, false), None, "{}", category.id());
                assert_eq!(settings, EmailSettings::default());
                continue;
            }
            assert_eq!(settings.set(category, false), Some(true));
            assert_eq!(
                settings.set(category, false),
                Some(false),
                "a repeat is not a change"
            );
            for other in EmailCategory::iter() {
                assert_eq!(
                    settings.allows(other),
                    other != category,
                    "turning off {} changed {}",
                    category.id(),
                    other.id()
                );
            }
        }
    }

    #[test]
    fn pause_all_leaves_only_required_emails() {
        let mut settings = EmailSettings::default();
        assert!(settings.pause_all());
        for category in EmailCategory::iter() {
            assert_eq!(settings.allows(category), !category.pausable());
        }
        assert!(!settings.pause_all(), "already paused: nothing to store");
    }
}
