use super::{Body, Content, Email, EmailCategory};

/// Confirms to the user that their pending cancellation was cleared and
/// their subscription is active again. Fired by the email subscriber on
/// `BillingOperation::Reactivated`.
pub struct SubscriptionReactivated;

impl Email for SubscriptionReactivated {
    fn subject(&self) -> String {
        "Your Subscription is Active Again".to_string()
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::Billing
    }

    fn campaign(&self) -> &'static str {
        "subscription_reactivated"
    }

    fn body_html(&self) -> String {
        Body::new()
            .content(
                Content::new()
                    .heading("You're back on Scanopy")
                    .paragraph("Hi there,")
                    .paragraph("Your pending cancellation has been cleared and your subscription is active again. Nothing else needs to change — billing will continue on your normal cycle.")
                    .paragraph("Glad to have you back."),
            )
            .render()
    }
}
