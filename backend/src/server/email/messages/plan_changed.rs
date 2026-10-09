use super::{Body, Content, Email, EmailCategory, links};
use crate::server::license::types::LicenseKeyType;

/// Sent when an organization's plan changes: confirms the new plan and that the
/// change is effective immediately.
pub struct PlanChanged<'a> {
    pub plan_name: &'a str,
    /// The type of key the organization held when it moved off a self-hosted
    /// plan, which ends its license. `None` when it was not on one.
    pub ended_license: Option<LicenseKeyType>,
}

impl Email for PlanChanged<'_> {
    fn subject(&self) -> String {
        "Your Plan Has Changed".to_string()
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::Billing
    }

    fn campaign(&self) -> &'static str {
        "plan_changed"
    }

    fn body_html(&self) -> String {
        let mut content = Content::new()
            .heading("Plan Updated")
            .paragraph("Hi there,")
            .paragraph(&format!(
                "Your Scanopy plan has been changed to {}. The change takes effect immediately.",
                self.plan_name
            ));

        // An online key is refused at its next check-in. An air-gapped key
        // never checks in, so it runs to the expiry it was minted with.
        match self.ended_license {
            Some(LicenseKeyType::Online) => {
                content = content.paragraph(
                    "Your self-hosted license ended with this change. Your own server stops accepting its license key at the next check-in, within a few hours, and goes read-only.",
                );
            }
            Some(LicenseKeyType::Offline) => {
                content = content.paragraph(
                    "Your self-hosted license ended with this change. Your air-gapped key keeps working until the expiry date it carries, and then your server goes read-only.",
                );
            }
            None => {}
        }

        Body::new()
            .content(content)
            .cta(links::APP_HOME, "Open Scanopy")
            .render()
    }
}
