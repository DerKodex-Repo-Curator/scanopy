use std::num::NonZeroU32;
use std::time::Duration;

use anyhow::Error;
use async_trait::async_trait;
use email_address::EmailAddress;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};

use super::{messages::Email, transport::EmailTransport};

/// Attempts after the first for a send the relay refused with a transient
/// (4xx) reply. SES answers 454 when the per-second rate is exceeded.
const TRANSIENT_RETRIES: u32 = 3;
/// Wait before the first retry; doubled for each one after it.
const FIRST_BACKOFF: Duration = Duration::from_secs(1);

/// Whether `error` is a transient SMTP reply: one the relay may accept later.
pub fn is_transient(error: &Error) -> bool {
    error
        .downcast_ref::<lettre::transport::smtp::Error>()
        .is_some_and(|e| e.is_transient())
}

/// A per-second quota of `per_second`, at least one.
pub fn per_second(per_second: u32) -> Quota {
    Quota::per_second(NonZeroU32::new(per_second.max(1)).expect("at least one"))
}

/// Wraps the configured transport so every email this process sends waits
/// for a permit, and a transient refusal is retried with backoff.
///
/// The limit is per process: containers running side by side each get the
/// full rate, which is why the default sits below the relay's own limit.
pub struct RateLimitedTransport {
    inner: Box<dyn EmailTransport>,
    limiter: DefaultDirectRateLimiter,
    first_backoff: Duration,
    transient: fn(&Error) -> bool,
}

impl RateLimitedTransport {
    pub fn new(inner: Box<dyn EmailTransport>, quota: Quota) -> Self {
        Self {
            inner,
            limiter: RateLimiter::direct(quota),
            first_backoff: FIRST_BACKOFF,
            transient: is_transient,
        }
    }
}

#[async_trait]
impl EmailTransport for RateLimitedTransport {
    async fn send(
        &self,
        to: EmailAddress,
        email: &dyn Email,
        base_url: &str,
        self_hosted: bool,
        unsubscribe_url: Option<&str>,
    ) -> Result<(), Error> {
        let mut backoff = self.first_backoff;
        let mut attempt = 0;
        loop {
            self.limiter.until_ready().await;
            match self
                .inner
                .send(to.clone(), email, base_url, self_hosted, unsubscribe_url)
                .await
            {
                Err(e) if attempt < TRANSIENT_RETRIES && (self.transient)(&e) => {
                    attempt += 1;
                    tracing::warn!(
                        attempt,
                        ?backoff,
                        "Relay refused an email for now; retrying"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff *= 2;
                }
                result => return result,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::email::messages::EmailCategory;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Instant;

    struct Probe;

    impl Email for Probe {
        fn subject(&self) -> String {
            "probe".into()
        }
        fn body_html(&self) -> String {
            String::new()
        }
        fn category(&self) -> EmailCategory {
            EmailCategory::Auth
        }
        fn campaign(&self) -> &'static str {
            "probe"
        }
    }

    /// Counts sends, refusing the first `failures` of them.
    struct Counting {
        sends: Arc<AtomicU32>,
        failures: u32,
    }

    #[derive(Debug, thiserror::Error)]
    #[error("try later")]
    struct TryLater;

    #[async_trait]
    impl EmailTransport for Counting {
        async fn send(
            &self,
            _: EmailAddress,
            _: &dyn Email,
            _: &str,
            _: bool,
            _: Option<&str>,
        ) -> Result<(), Error> {
            let n = self.sends.fetch_add(1, Ordering::SeqCst);
            if n < self.failures {
                Err(TryLater.into())
            } else {
                Ok(())
            }
        }
    }

    fn transport(quota: Quota, failures: u32) -> (RateLimitedTransport, Arc<AtomicU32>) {
        let sends = Arc::new(AtomicU32::new(0));
        let mut transport = RateLimitedTransport::new(
            Box::new(Counting {
                sends: sends.clone(),
                failures,
            }),
            quota,
        );
        transport.first_backoff = Duration::from_millis(1);
        transport.transient = |e| e.is::<TryLater>();
        (transport, sends)
    }

    async fn send(transport: &RateLimitedTransport) -> Result<(), Error> {
        transport
            .send(
                EmailAddress::new_unchecked("a@example.com"),
                &Probe,
                "https://example.com",
                false,
                None,
            )
            .await
    }

    /// Past the initial burst, sends are spaced to the quota: 20 per second
    /// allows 20 at once and one every 50ms after that.
    #[tokio::test]
    async fn sends_beyond_the_burst_wait_for_the_quota() {
        let (transport, sends) = transport(per_second(20), 0);
        let started = Instant::now();
        for _ in 0..30 {
            send(&transport).await.unwrap();
        }
        assert_eq!(sends.load(Ordering::SeqCst), 30);
        assert!(
            started.elapsed() >= Duration::from_millis(450),
            "30 sends at 20/s finished in {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn a_transient_refusal_is_retried_until_it_succeeds() {
        let (transport, sends) = transport(per_second(1000), 2);
        send(&transport).await.unwrap();
        assert_eq!(sends.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retries_stop_after_the_limit() {
        let (transport, sends) = transport(per_second(1000), u32::MAX);
        assert!(send(&transport).await.is_err());
        assert_eq!(sends.load(Ordering::SeqCst), TRANSIENT_RETRIES + 1);
    }
}
