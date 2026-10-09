use anyhow::Error;
use async_trait::async_trait;
use email_address::EmailAddress;

use super::messages::Email;

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
    async fn send(
        &self,
        to: EmailAddress,
        email: &dyn Email,
        base_url: &str,
        self_hosted: bool,
        unsubscribe_url: Option<&str>,
    ) -> Result<(), Error>;
}
