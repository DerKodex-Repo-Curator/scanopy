use super::{Body, Content, Email, EmailCategory};

/// Tells the user a previously failed payment has gone through and their
/// subscription is active again.
pub struct PaymentRecovered<'a> {
    pub amount: &'a str,
}

impl Email for PaymentRecovered<'_> {
    fn subject(&self) -> String {
        "Your Payment is Back On Track".to_string()
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::Billing
    }

    fn campaign(&self) -> &'static str {
        "payment_recovered"
    }

    fn body_html(&self) -> String {
        Body::new()
            .content(
                Content::new()
                    .heading("Payment Recovered")
                    .paragraph("Hi there,")
                    .paragraph(&format!(
                        "Your previously failed payment of {} has gone through. Your subscription is active again — no action needed on your end.",
                        self.amount
                    ))
                    .paragraph("Thanks for being a Scanopy customer."),
            )
            .render()
    }
}
