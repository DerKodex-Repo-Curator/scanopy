use super::{Body, Content, Email, EmailCategory};

/// Confirms to the user that their subscription has been paused. Fired by the
/// email subscriber on `BillingOperation::Paused`.
pub struct SubscriptionPaused<'a> {
    pub resumes_at: &'a str,
}

impl Email for SubscriptionPaused<'_> {
    fn subject(&self) -> String {
        "Your Scanopy Subscription is Paused".to_string()
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::Billing
    }

    fn campaign(&self) -> &'static str {
        "subscription_paused"
    }

    fn body_html(&self) -> String {
        Body::new()
            .content(
                Content::new()
                    .heading("Your subscription is paused")
                    .paragraph("Hi there,")
                    .paragraph(&format!(
                        "Your subscription is paused until {}, or until you click <strong>Resume now</strong> in your billing settings. We won't bill you while you're paused — when you resume, a credit for the days you paused will be applied to your next invoice.",
                        self.resumes_at
                    )),
            )
            .render()
    }
}
