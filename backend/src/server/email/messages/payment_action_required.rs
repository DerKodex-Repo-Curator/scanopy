use super::{Body, Content, Email, EmailCategory};

/// Asks the user to complete additional authentication (3D Secure) for a
/// payment, linking to the provider-hosted authorization page.
pub struct PaymentActionRequired<'a> {
    pub cta_href: &'a str,
}

impl Email for PaymentActionRequired<'_> {
    fn subject(&self) -> String {
        "Payment Requires Authentication".to_string()
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::Billing
    }

    fn campaign(&self) -> &'static str {
        "payment_action_required"
    }

    fn body_html(&self) -> String {
        Body::new()
            .content(
                Content::new()
                    .heading("Payment Requires Authentication")
                    .paragraph("Hi there,")
                    .paragraph("Your recent payment for Scanopy requires additional authentication (3D Secure). Please complete the verification to continue your subscription."),
            )
            .cta(self.cta_href, "Authorize Payment")
            .render()
    }
}
