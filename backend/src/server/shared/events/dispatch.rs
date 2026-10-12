//! Event dispatch: how a published event reaches its subscribers.
//!
//! Each subscriber has its own queue. `publish` queues the event for every subscriber whose
//! filter matches and returns; a worker per subscriber handles events in publish order, retrying
//! failures (`handle_with_retry`) and counting them in `scanopy_event_subscriber_errors_total`.
//! Debounced subscribers batch instead. A `DispatchTracker` shared by the bus counts queued work,
//! so tests can `flush` and the server can drain on shutdown.

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, RwLock, broadcast, mpsc, watch};

use crate::server::shared::events::traits::{Event, Operation, Subscriber, SubscriberFilter};

/// A subscriber failure that a re-run would make worse: side effects for part of the work have
/// already been applied, and running `handle` again would repeat them. `failed` is how many
/// events (or sends) failed, so the error counter reflects the size of the loss.
#[derive(Debug, thiserror::Error)]
#[error("{failed} failed: {source}")]
pub struct NonRetryable {
    pub failed: usize,
    #[source]
    pub source: anyhow::Error,
}

impl NonRetryable {
    /// `None` when `failures` is empty; otherwise the count and the first error.
    pub fn from_failures(failures: Vec<anyhow::Error>) -> Option<Self> {
        let failed = failures.len();
        failures
            .into_iter()
            .next()
            .map(|source| Self { failed, source })
    }
}

/// First wait between attempts of a failing subscriber (doubling after). Short, because a
/// subscriber's queue waits behind its retries and a shutdown drain has a few seconds.
const SUBSCRIBER_RETRY_MIN_DELAY: Duration = Duration::from_millis(200);
/// Retries after the first attempt.
const SUBSCRIBER_RETRY_TIMES: usize = 2;

/// How a delivery that hit an error ended. The `outcome` label of
/// `scanopy_event_subscriber_errors_total`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum SubscriberErrorOutcome {
    /// A later attempt succeeded.
    Recovered,
    /// Every attempt failed.
    Exhausted,
    /// Not retried: the subscriber said a re-run is unsafe, or the error can't clear in time.
    NotRetried,
}

/// Errors a retry can't fix within the budget: a [`NonRetryable`] report, a validation failure,
/// or an org lock that already waited its full timeout.
fn not_retryable(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        c.is::<NonRetryable>()
            || c.is::<crate::server::shared::types::api::ValidationError>()
            || matches!(
                c.downcast_ref::<crate::server::shared::storage::lock::LockError>(),
                Some(crate::server::shared::storage::lock::LockError::Timeout(..))
            )
    })
}

/// Run `subscriber.handle`, retrying this subscriber alone (never the others on the bus, whose
/// side effects already ran) while the error is retryable and attempts remain. Records one
/// `scanopy_event_subscriber_errors_total` increment per delivery that hit an error.
async fn handle_with_retry<Op: Operation>(
    subscriber: &dyn Subscriber<Op>,
    name: &'static str,
    events: Vec<Event<Op>>,
) {
    use crate::server::metrics::subscriber::record_subscriber_error;
    use backon::{ExponentialBuilder, Retryable};

    let mut retries = 0usize;
    let result = (|| subscriber.handle(events.clone()))
        .retry(
            ExponentialBuilder::default()
                .with_min_delay(SUBSCRIBER_RETRY_MIN_DELAY)
                .with_max_times(SUBSCRIBER_RETRY_TIMES),
        )
        .when(|e| !not_retryable(e))
        .notify(|e, delay| {
            retries += 1;
            tracing::debug!(
                subscriber = name,
                retry = retries,
                retry_in_ms = delay.as_millis() as u64,
                error = format!("{e:#}"),
                "Typed subscriber failed; retrying"
            );
        })
        .await;
    let attempts = retries + 1;

    match result {
        Ok(()) if retries > 0 => {
            record_subscriber_error(name, SubscriberErrorOutcome::Recovered, 1);
            tracing::warn!(
                subscriber = name,
                attempts,
                "Typed subscriber recovered after retry"
            );
        }
        Ok(()) => {}
        Err(e) if not_retryable(&e) => {
            let failed = e
                .chain()
                .find_map(|c| c.downcast_ref::<NonRetryable>())
                .map_or(1, |n| n.failed);
            record_subscriber_error(name, SubscriberErrorOutcome::NotRetried, failed as u64);
            tracing::error!(
                subscriber = name,
                error = format!("{e:#}"),
                "Typed subscriber failed; not retried"
            );
        }
        Err(e) => {
            record_subscriber_error(name, SubscriberErrorOutcome::Exhausted, 1);
            tracing::error!(
                subscriber = name,
                attempts,
                error = format!("{e:#}"),
                "Typed subscriber failed after retries"
            );
        }
    }
}

// ===========================================================================
// Typed channel — per-operation-type pub/sub plumbing
// ===========================================================================

/// Counts events queued for, or being handled by, any subscriber of the bus, so a caller can wait
/// for dispatch to finish: tests before asserting a subscriber's effect, and the server before it
/// exits.
pub struct DispatchTracker {
    in_flight: AtomicUsize,
    idle: Notify,
    /// Bumped to make debounced subscribers drain now rather than at their next tick.
    drain_now: watch::Sender<u64>,
}

impl Default for DispatchTracker {
    fn default() -> Self {
        Self {
            in_flight: AtomicUsize::new(0),
            idle: Notify::new(),
            drain_now: watch::Sender::new(0),
        }
    }
}

impl DispatchTracker {
    fn queued(&self, count: usize) {
        self.in_flight.fetch_add(count, Ordering::SeqCst);
    }

    fn done(&self, count: usize) {
        if count > 0 && self.in_flight.fetch_sub(count, Ordering::SeqCst) == count {
            self.idle.notify_waiters();
        }
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }

    /// Wait until no event is queued or being handled. Debounced subscribers drain at once
    /// instead of at their next tick. Events a subscriber publishes while handling one are
    /// counted before that one finishes, so this also waits for them.
    pub async fn flush(&self) {
        self.drain_now.send_modify(|generation| *generation += 1);
        loop {
            let idle = self.idle.notified();
            tokio::pin!(idle);
            idle.as_mut().enable();
            if self.in_flight() == 0 {
                return;
            }
            idle.await;
        }
    }
}

struct TypedSubscriberState<Op: Operation> {
    subscriber: Arc<dyn Subscriber<Op>>,
    delivery: Delivery<Op>,
}

/// How events reach a subscriber. Both keep publish order.
enum Delivery<Op: Operation> {
    /// One event at a time, through a queue its own worker task reads.
    Queued(mpsc::UnboundedSender<Event<Op>>),
    /// Batched, drained every debounce window.
    Debounced(Arc<RwLock<Vec<Event<Op>>>>),
}

impl<Op: Operation> TypedSubscriberState<Op> {
    fn new(
        subscriber: Arc<dyn Subscriber<Op>>,
        name: &'static str,
        tracker: Arc<DispatchTracker>,
    ) -> Self {
        let debounce_ms = subscriber.debounce_window_ms();

        let delivery = if debounce_ms > 0 {
            let pending = Arc::new(RwLock::new(Vec::<Event<Op>>::new()));
            let pending_clone = pending.clone();
            let subscriber_clone = subscriber.clone();
            let window = Duration::from_millis(debounce_ms);
            let mut drain_now = tracker.drain_now.subscribe();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(window);
                loop {
                    tokio::select! {
                        _ = interval.tick() => {}
                        changed = drain_now.changed() => {
                            if changed.is_err() {
                                return;
                            }
                        }
                    }
                    let drained: Vec<Event<Op>> = {
                        let mut p = pending_clone.write().await;
                        if p.is_empty() {
                            continue;
                        }
                        p.drain(..).collect()
                    };
                    let count = drained.len();
                    deliver(subscriber_clone.as_ref(), name, drained).await;
                    tracker.done(count);
                }
            });
            Delivery::Debounced(pending)
        } else {
            let (sender, mut receiver) = mpsc::unbounded_channel::<Event<Op>>();
            let subscriber_clone = subscriber.clone();
            tokio::spawn(async move {
                while let Some(event) = receiver.recv().await {
                    deliver(subscriber_clone.as_ref(), name, vec![event]).await;
                    tracker.done(1);
                }
            });
            Delivery::Queued(sender)
        };

        Self {
            subscriber,
            delivery,
        }
    }

    async fn add_event(&self, event: Event<Op>) {
        match &self.delivery {
            // The worker only stops when this sender is dropped, so the send can't fail.
            Delivery::Queued(sender) => {
                let _ = sender.send(event);
            }
            Delivery::Debounced(pending) => pending.write().await.push(event),
        }
    }
}

/// [`handle_with_retry`], with a panic caught and reported like any other failure so it stops
/// neither the worker nor the events queued behind it.
async fn deliver<Op: Operation>(
    subscriber: &dyn Subscriber<Op>,
    name: &'static str,
    events: Vec<Event<Op>>,
) {
    use futures::FutureExt;

    let outcome = std::panic::AssertUnwindSafe(handle_with_retry(subscriber, name, events))
        .catch_unwind()
        .await;
    if let Err(panic) = outcome {
        let message = panic
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        crate::server::metrics::subscriber::record_subscriber_error(
            name,
            SubscriberErrorOutcome::Exhausted,
            1,
        );
        tracing::error!(
            subscriber = name,
            panic = message,
            "Typed subscriber panicked"
        );
    }
}

/// One broadcast channel + subscriber list per operation type. Owned by
/// `EventBus`. `publish` queues the event for every subscriber whose
/// `filter().matches` returns true and returns without waiting for them.
pub struct TypedChannel<Op: Operation> {
    sender: broadcast::Sender<Event<Op>>,
    subscribers: Arc<RwLock<Vec<TypedSubscriberState<Op>>>>,
    tracker: Arc<DispatchTracker>,
}

impl<Op: Operation> Default for TypedChannel<Op> {
    fn default() -> Self {
        Self::with_tracker(Arc::default())
    }
}

impl<Op: Operation> TypedChannel<Op> {
    pub fn new() -> Self {
        Self::default()
    }

    /// A channel whose dispatch counts toward `tracker`, shared by every channel of one bus.
    pub fn with_tracker(tracker: Arc<DispatchTracker>) -> Self {
        let (sender, _) = broadcast::channel(1000);
        Self {
            sender,
            subscribers: Arc::new(RwLock::new(Vec::new())),
            tracker,
        }
    }

    pub async fn register(&self, subscriber: Arc<dyn Subscriber<Op>>, name: &'static str) {
        let debounce_ms = subscriber.debounce_window_ms();
        let state = TypedSubscriberState::new(subscriber, name, self.tracker.clone());
        let mut subs = self.subscribers.write().await;
        subs.push(state);
        tracing::debug!(
            subscriber = name,
            debounce_ms = debounce_ms,
            "Registered typed subscriber",
        );
    }

    /// Queue `event` for each matching subscriber and return. Each subscriber receives events in
    /// the order they were published; subscribers run independently of each other.
    pub async fn publish(&self, event: Event<Op>) -> anyhow::Result<()> {
        let _ = self.sender.send(event.clone());
        let subs = self.subscribers.read().await;
        for state in subs.iter() {
            if state.subscriber.filter().matches(&event) {
                self.tracker.queued(1);
                state.add_event(event.clone()).await;
            }
        }
        Ok(())
    }

    /// Wait until every event published on this channel's bus has been handled.
    pub async fn flush(&self) {
        self.tracker.flush().await
    }

    pub fn subscribe_channel(&self) -> broadcast::Receiver<Event<Op>> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::auth::middleware::auth::AuthenticatedEntity;
    use crate::server::shared::events::traits::{EventFilter, OrgScope};
    use crate::server::shared::events::types::OnboardingOperation;
    use uuid::Uuid;

    use crate::server::shared::storage::lock::{LockError, LockKey};

    /// Fails its first `failures` calls with the error `make_error` builds, then succeeds.
    struct ScriptedSubscriber {
        failures: usize,
        make_error: fn() -> anyhow::Error,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Subscriber<OnboardingOperation> for ScriptedSubscriber {
        fn filter(&self) -> EventFilter<OnboardingOperation> {
            EventFilter::all()
        }
        async fn handle(&self, _: Vec<Event<OnboardingOperation>>) -> anyhow::Result<()> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call < self.failures {
                return Err((self.make_error)());
            }
            Ok(())
        }
    }

    /// Publish one event to a channel holding only `subscriber`; return its call count and the
    /// rendered `scanopy_event_subscriber_errors_total` lines.
    async fn deliver(failures: usize, make_error: fn() -> anyhow::Error) -> (usize, String) {
        let recorder = crate::server::metrics::prometheus_builder().build_recorder();
        let handle = recorder.handle();
        let _guard = metrics::set_default_local_recorder(&recorder);

        let calls = Arc::new(AtomicUsize::new(0));
        let channel = TypedChannel::<OnboardingOperation>::new();
        channel
            .register(
                Arc::new(ScriptedSubscriber {
                    failures,
                    make_error,
                    calls: calls.clone(),
                }),
                "scripted_subscriber:onboarding_operation",
            )
            .await;
        channel
            .publish(Event::new(
                OrgScope {
                    organization_id: Uuid::new_v4(),
                },
                OnboardingOperation::OnboardingModalCompleted,
                AuthenticatedEntity::System,
            ))
            .await
            .expect("publish carries on past a failing subscriber");
        channel.flush().await;

        let rendered = handle
            .render()
            .lines()
            .filter(|l| l.starts_with("scanopy_event_subscriber_errors_total{"))
            .collect::<Vec<_>>()
            .join("\n");
        (calls.load(Ordering::SeqCst), rendered)
    }

    fn series(outcome: &str, value: u64) -> String {
        format!(
            r#"scanopy_event_subscriber_errors_total{{subscriber="scripted_subscriber",event_type="onboarding_operation",outcome="{outcome}"}} {value}"#
        )
    }

    fn transient() -> anyhow::Error {
        anyhow::anyhow!("connection reset")
    }

    #[tokio::test(flavor = "current_thread")]
    async fn transient_failure_is_retried_and_counted_as_recovered() {
        let (calls, rendered) = deliver(1, transient).await;
        assert_eq!(calls, 2);
        assert_eq!(rendered, series("recovered", 1));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn persistent_failure_stops_after_three_attempts() {
        let (calls, rendered) = deliver(usize::MAX, transient).await;
        assert_eq!(calls, 3);
        assert_eq!(rendered, series("exhausted", 1));
    }

    /// The lock already waited its full timeout; retrying would hold the request for another 30s.
    #[tokio::test(flavor = "current_thread")]
    async fn lock_timeout_is_not_retried() {
        let (calls, rendered) = deliver(usize::MAX, || {
            anyhow::Error::from(LockError::Timeout(
                Duration::from_secs(30),
                LockKey::Organization(Uuid::nil()),
            ))
            .context("org mirror write")
        })
        .await;
        assert_eq!(calls, 1);
        assert_eq!(rendered, series("not_retried", 1));
    }

    /// A subscriber that already applied part of its work reports the failures without a retry,
    /// and the counter carries how many failed.
    #[tokio::test(flavor = "current_thread")]
    async fn non_retryable_report_counts_each_failure() {
        let (calls, rendered) = deliver(usize::MAX, || {
            NonRetryable::from_failures(vec![transient(), transient(), transient()])
                .expect("three failures")
                .into()
        })
        .await;
        assert_eq!(calls, 1);
        assert_eq!(rendered, series("not_retried", 3));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn successful_delivery_records_nothing() {
        let (calls, rendered) = deliver(0, transient).await;
        assert_eq!(calls, 1);
        assert!(rendered.is_empty(), "{rendered}");
    }

    fn onboarding_event(op: OnboardingOperation) -> Event<OnboardingOperation> {
        Event::new(
            OrgScope {
                organization_id: Uuid::new_v4(),
            },
            op,
            AuthenticatedEntity::System,
        )
    }

    /// Blocks in `handle` until released, then records the event.
    struct GatedSubscriber {
        gate: Arc<Notify>,
        handled: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Subscriber<OnboardingOperation> for GatedSubscriber {
        fn filter(&self) -> EventFilter<OnboardingOperation> {
            EventFilter::all()
        }
        async fn handle(&self, _: Vec<Event<OnboardingOperation>>) -> anyhow::Result<()> {
            self.gate.notified().await;
            self.handled.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn publish_returns_before_a_slow_subscriber_finishes() {
        let gate = Arc::new(Notify::new());
        let handled = Arc::new(AtomicUsize::new(0));
        let channel = TypedChannel::<OnboardingOperation>::new();
        channel
            .register(
                Arc::new(GatedSubscriber {
                    gate: gate.clone(),
                    handled: handled.clone(),
                }),
                "gated_subscriber:onboarding_operation",
            )
            .await;

        channel
            .publish(onboarding_event(
                OnboardingOperation::OnboardingModalCompleted,
            ))
            .await
            .unwrap();
        assert_eq!(handled.load(Ordering::SeqCst), 0);

        gate.notify_one();
        channel.flush().await;
        assert_eq!(handled.load(Ordering::SeqCst), 1);
    }

    /// Records the order events arrive in, yielding mid-handle so a reordering would show.
    struct RecordingSubscriber {
        seen: Arc<std::sync::Mutex<Vec<Uuid>>>,
    }

    #[async_trait::async_trait]
    impl Subscriber<OnboardingOperation> for RecordingSubscriber {
        fn filter(&self) -> EventFilter<OnboardingOperation> {
            EventFilter::all()
        }
        async fn handle(&self, events: Vec<Event<OnboardingOperation>>) -> anyhow::Result<()> {
            tokio::task::yield_now().await;
            self.seen
                .lock()
                .unwrap()
                .extend(events.iter().map(|e| e.id));
            Ok(())
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_subscriber_receives_events_in_publish_order() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let channel = TypedChannel::<OnboardingOperation>::new();
        channel
            .register(
                Arc::new(RecordingSubscriber { seen: seen.clone() }),
                "recording_subscriber:onboarding_operation",
            )
            .await;

        let mut published = Vec::new();
        for _ in 0..100 {
            let event = onboarding_event(OnboardingOperation::OnboardingModalCompleted);
            published.push(event.id);
            channel.publish(event).await.unwrap();
        }
        channel.flush().await;

        assert_eq!(*seen.lock().unwrap(), published);
    }

    /// Panics on its first call, then counts.
    struct PanickingSubscriber {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Subscriber<OnboardingOperation> for PanickingSubscriber {
        fn filter(&self) -> EventFilter<OnboardingOperation> {
            EventFilter::all()
        }
        async fn handle(&self, _: Vec<Event<OnboardingOperation>>) -> anyhow::Result<()> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                panic!("subscriber bug");
            }
            Ok(())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_panicking_subscriber_is_counted_and_keeps_handling() {
        let recorder = crate::server::metrics::prometheus_builder().build_recorder();
        let handle = recorder.handle();
        let _guard = metrics::set_default_local_recorder(&recorder);

        let calls = Arc::new(AtomicUsize::new(0));
        let channel = TypedChannel::<OnboardingOperation>::new();
        channel
            .register(
                Arc::new(PanickingSubscriber {
                    calls: calls.clone(),
                }),
                "panicking_subscriber:onboarding_operation",
            )
            .await;
        for _ in 0..2 {
            channel
                .publish(onboarding_event(
                    OnboardingOperation::OnboardingModalCompleted,
                ))
                .await
                .unwrap();
        }
        channel.flush().await;

        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(handle.render().lines().any(|l| l.starts_with(
            r#"scanopy_event_subscriber_errors_total{subscriber="panicking_subscriber",event_type="onboarding_operation",outcome="exhausted"} 1"#
        )));
    }

    /// Debounced batches drain on a flush instead of waiting out the window, so a shutdown
    /// doesn't drop them.
    #[tokio::test]
    async fn flush_drains_a_debounced_batch() {
        struct Debounced(Arc<AtomicUsize>);

        #[async_trait::async_trait]
        impl Subscriber<OnboardingOperation> for Debounced {
            fn filter(&self) -> EventFilter<OnboardingOperation> {
                EventFilter::all()
            }
            async fn handle(&self, events: Vec<Event<OnboardingOperation>>) -> anyhow::Result<()> {
                self.0.fetch_add(events.len(), Ordering::SeqCst);
                Ok(())
            }
            fn debounce_window_ms(&self) -> u64 {
                3_600_000
            }
        }

        let handled = Arc::new(AtomicUsize::new(0));
        let channel = TypedChannel::<OnboardingOperation>::new();
        channel
            .register(
                Arc::new(Debounced(handled.clone())),
                "debounced:onboarding_operation",
            )
            .await;
        // The interval's first tick fires at once; let it pass on an empty batch.
        tokio::task::yield_now().await;
        for _ in 0..3 {
            channel
                .publish(onboarding_event(
                    OnboardingOperation::OnboardingModalCompleted,
                ))
                .await
                .unwrap();
        }
        tokio::time::timeout(Duration::from_secs(5), channel.flush())
            .await
            .expect("flush drains the batch without waiting an hour");
        assert_eq!(handled.load(Ordering::SeqCst), 3);
    }
}
