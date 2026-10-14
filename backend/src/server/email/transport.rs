use anyhow::Error;
use async_trait::async_trait;
use email_address::EmailAddress;
use uuid::Uuid;

use super::messages::Email;
use crate::server::shared::types::metadata::HasId;

/// The header SES reads message tags from. SES strips it before delivery.
pub const SES_MESSAGE_TAGS: &str = "X-SES-MESSAGE-TAGS";

/// The `X-SES-MESSAGE-TAGS` value for one send: the email's category and
/// campaign, the recipient's user id when they have an account, then the
/// email's own tags. SES publishes them with each event, so the event archive
/// can say who was sent what.
pub fn ses_message_tags(email: &dyn Email, user_id: Option<Uuid>) -> String {
    let mut tags = vec![
        ("category", email.category().id().to_string()),
        ("campaign", email.campaign().to_string()),
    ];
    if let Some(user_id) = user_id {
        tags.push(("user_id", user_id.to_string()));
    }
    tags.extend(email.ses_message_tags());
    tags.iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Delivers a rendered email to a recipient.
///
/// Implementors own *how* a message is sent (SMTP, or logging to disk) and ask
/// the [`Email`] for exactly the renderings they need — HTML, plaintext, the
/// subject, the category tag. They never inspect which concrete email it is,
/// so a new email never requires a transport change.
#[async_trait]
pub trait EmailTransport: Send + Sync {
    /// Render `email` against `base_url` and deliver it to `to`. `self_hosted`
    /// gates the footer's sender-identification block (see
    /// [`Email::render_html`](super::messages::Email::render_html)).
    /// `unsubscribe_url` is the recipient's one-click unsubscribe link (RFC 8058),
    /// present only for a pausable email to a known user.
    /// `ses_tags` is the `X-SES-MESSAGE-TAGS` value, present only on Scanopy
    /// Cloud (see [`ses_message_tags`]).
    async fn send(
        &self,
        to: EmailAddress,
        email: &dyn Email,
        base_url: &str,
        self_hosted: bool,
        unsubscribe_url: Option<&str>,
        ses_tags: Option<&str>,
    ) -> Result<(), Error>;
}
