use super::{Body, Content, Email, EmailCategory};

/// Confirms to the user that a payment method was added to their account.
pub struct PaymentMethodAdded;

impl Email for PaymentMethodAdded {
    fn subject(&self) -> String {
        "Payment Method Added".to_string()
    }

    fn category(&self) -> EmailCategory {
        EmailCategory::Billing
    }

    fn campaign(&self) -> &'static str {
        "payment_method_added"
    }

    fn body_html(&self) -> String {
        Body::new()
            .content(
                Content::new()
                    .heading("Payment Method Added")
                    .paragraph("Hi there,")
                    .paragraph("A payment method has been added to your Scanopy account."),
            )
            .render()
    }
}
