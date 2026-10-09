use super::{Body, Content, Email, EmailCategory};

/// Confirms to the user that their paused subscription has resumed. Fired by
/// the email subscriber on `BillingOperation::Resumed`.
pub struct SubscriptionResumed;

impl Email for SubscriptionResumed {
    fn subject(&self) -> String {
        "Your Scanopy Subscription is Active Again".to_string()
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::Billing
    }

    fn campaign(&self) -> &'static str {
        "subscription_resumed"
    }

    fn body_html(&self) -> String {
        Body::new()
            .content(
                Content::new()
                    .heading("Welcome back")
                    .paragraph("Hi there,")
                    .paragraph("Your Scanopy subscription is active again and the app is unlocked. A credit for the days you paused has been applied to your next invoice."),
            )
            .render()
    }
}
