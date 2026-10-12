//! Generic typed event types.
//!
//! `Event<Op>` is parameterized over an `Operation` impl. Each operation type
//! carries per-domain `Scope` (identity dimensions: org_id / site_id / etc.),
//! `Flags` (cross-cutting emission hints like `suppress_logs`), and `Filter`
//! (the shape of selection predicates a subscriber declares).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{borrow::Cow, collections::HashMap, fmt::Debug, hash::Hash, net::IpAddr};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use strum::IntoDiscriminant;
use tokio::sync::{Notify, RwLock, broadcast, mpsc, watch};
use uuid::Uuid;

use crate::daemon::discovery::types::base::DiscoveryTerminalReason;
use crate::server::{
    auth::middleware::auth::{ActorProperties, AuthenticatedEntity},
    daemons::r#impl::version::own_version,
    discovery::r#impl::types::DiscoveryType,
    shared::{
        entities::{Entity, EntityDiscriminants},
        events::types::{EntityOperation, EntityOperationDiscriminants, EventLogLevel, LabelColor},
    },
};

// ===========================================================================
// Operation trait
// ===========================================================================

/// An operation enum for a typed event. Implementors are sum types with
/// `#[derive(EnumDiscriminants)]`. The discriminant enum is what filters key
/// on.
///
/// Each operation type carries:
/// - `Scope`: identity dimensions (org_id / site_id / entity / etc.)
/// - `Flags`: cross-cutting emission hints (`suppress_logs`, etc.)
/// - `Filter`: the filter shape a `Subscriber<Self>` declares
///
/// All discriminant trait-bound repetition is consolidated here via an
/// associated-type bound so call sites don't need to repeat it.
pub trait Operation:
    IntoDiscriminant<
        Discriminant: Eq
                          + Hash
                          + Clone
                          + Debug
                          + Send
                          + Sync
                          + Serialize
                          + DeserializeOwned
                          + AsRef<str>
                          + 'static,
    > + Serialize
    + DeserializeOwned
    + Clone
    + Debug
    + Send
    + Sync
    + Sized
    + 'static
{
    type Scope: EventScope + Clone + Debug + Send + Sync + Serialize + DeserializeOwned + 'static;
    type Flags: Default + Clone + Debug + Send + Sync + Serialize + DeserializeOwned + 'static;
    type Filter: SubscriberFilter<Self>;

    fn log_level(&self) -> EventLogLevel;

    /// The event's snake_case name, e.g. `"subnet_created"`: the one name every consumer outside
    /// the bus (log line, PostHog) uses. Receives the `Scope` because some operations (notably
    /// entity events) carry the meaningful noun there rather than on the operation itself.
    fn event_name(&self, scope: &Self::Scope) -> Cow<'static, str>;

    /// Label prefixed to the event's log line. Defaults to [`Operation::event_name`].
    fn log_label(&self, scope: &Self::Scope) -> String {
        self.event_name(scope).into_owned()
    }

    /// Whom the event is about, for consumers that attribute events to a person.
    fn attribution(&self, _scope: &Self::Scope) -> Attribution {
        Attribution::Actor
    }

    /// The event-specific payload a consumer outside the server receives, under `metadata`.
    /// Defaults to the operation itself. Operations whose identity lives on the scope override
    /// it to pick the scope fields that are safe to send: a scope can carry the whole entity, a
    /// client address or a device's diagnostic.
    fn metadata<'a>(&'a self, _scope: &'a Self::Scope) -> impl Serialize + Send + 'a {
        self
    }

    /// Color for the label in the log line. Defaults to `Neutral`; operation
    /// types with create/update/delete semantics override this (see
    /// `EntityOperation`).
    fn log_color(&self) -> LabelColor {
        LabelColor::Neutral
    }
}

// ===========================================================================
// Scope types
// ===========================================================================

/// The organization an event belongs to, as its scope names it. A site-scoped event names its
/// site; the consumer looks the site's organization up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeOrganization {
    Org(Uuid),
    Site(Uuid),
}

/// Implemented by every operation's `Scope`.
pub trait EventScope {
    /// `None` only where the scope has no organization at all, e.g. a failed login.
    fn organization(&self) -> Option<ScopeOrganization>;
}

/// Whom an event is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attribution {
    /// The authenticated user, else the event's organization.
    Actor,
    /// A named user, e.g. an email's recipient.
    User(Uuid),
    /// The organization, whoever acted, e.g. an anonymous share view.
    Organization,
}

/// Identity scope for org-only events: `BillingOperation`, `OnboardingOperation`,
/// `AnalyticsOperation`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OrgScope {
    pub organization_id: Uuid,
}

/// Identity scope for site-only events: `DiscoveryPhase`. Discovery sessions
/// are site-keyed; org is derivable via the site.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SiteScope {
    pub site_id: Uuid,
}

/// Identity scope for auth events. Both `user_id` and `organization_id` are
/// optional because failed-login events have neither.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct AuthScope {
    pub user_id: Option<Uuid>,
    pub organization_id: Option<Uuid>,
    pub ip_address: IpAddr,
    pub user_agent: Option<String>,
}

/// Identity scope for entity events. Entities are either org-scoped (User,
/// Invite, ApiKey, Organization) or site-scoped (Host, Subnet, Service,
/// Daemon, Tag, etc.) — never both.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EntityScope {
    Org {
        organization_id: Uuid,
        entity_id: Uuid,
        entity_type: Entity,
    },
    Site {
        site_id: Uuid,
        entity_id: Uuid,
        entity_type: Entity,
    },
}

impl EntityScope {
    /// Build a scope from the entity-event source fields: prefers
    /// `organization_id` when present (org-scoped entity), otherwise uses
    /// `site_id`. At least one must be `Some` — otherwise this returns
    /// `None`.
    pub fn from_ids(
        entity_id: Uuid,
        entity_type: Entity,
        site_id: Option<Uuid>,
        organization_id: Option<Uuid>,
    ) -> Option<Self> {
        if let Some(organization_id) = organization_id {
            Some(EntityScope::Org {
                organization_id,
                entity_id,
                entity_type,
            })
        } else {
            site_id.map(|site_id| EntityScope::Site {
                site_id,
                entity_id,
                entity_type,
            })
        }
    }

    pub fn entity_id(&self) -> Uuid {
        match self {
            EntityScope::Org { entity_id, .. } | EntityScope::Site { entity_id, .. } => *entity_id,
        }
    }

    pub fn entity_type(&self) -> &Entity {
        match self {
            EntityScope::Org { entity_type, .. } | EntityScope::Site { entity_type, .. } => {
                entity_type
            }
        }
    }

    pub fn entity_discriminant(&self) -> EntityDiscriminants {
        self.entity_type().discriminant()
    }
}

impl EventScope for OrgScope {
    fn organization(&self) -> Option<ScopeOrganization> {
        Some(ScopeOrganization::Org(self.organization_id))
    }
}

impl EventScope for SiteScope {
    fn organization(&self) -> Option<ScopeOrganization> {
        Some(ScopeOrganization::Site(self.site_id))
    }
}

impl EventScope for AuthScope {
    fn organization(&self) -> Option<ScopeOrganization> {
        self.organization_id.map(ScopeOrganization::Org)
    }
}

impl EventScope for EntityScope {
    fn organization(&self) -> Option<ScopeOrganization> {
        Some(match self {
            EntityScope::Org {
                organization_id, ..
            } => ScopeOrganization::Org(*organization_id),
            EntityScope::Site { site_id, .. } => ScopeOrganization::Site(*site_id),
        })
    }
}

impl EventScope for DiscoveryScope {
    fn organization(&self) -> Option<ScopeOrganization> {
        Some(ScopeOrganization::Site(self.site_id))
    }
}

/// Identity scope for discovery session events. Carries the session/daemon/
/// discovery-type identifiers, plus an `error_reason` populated for `Failed`
/// / `Cancelled` phases. `reason` says why a terminal phase was reached, so
/// subscribers that only see the event (metrics, analytics) can tell a reaped stall from a
/// user cancel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DiscoveryScope {
    pub site_id: Uuid,
    pub session_id: Uuid,
    pub daemon_id: Uuid,
    pub discovery_type: DiscoveryType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<DiscoveryTerminalReason>,
}

// ===========================================================================
// Flags types
// ===========================================================================

/// Cross-cutting hint flags for entity events. `trigger_stale` and `clear_stale`
/// control topology-rebuild gating; `suppress_logs` keeps noisy emissions out
/// of logs and analytics.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EntityEventFlags {
    pub suppress_logs: bool,
    pub trigger_stale: bool,
    pub clear_stale: bool,
}

/// Cross-cutting hints for non-entity events. Currently only `suppress_logs`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EventFlags {
    pub suppress_logs: bool,
}

// ===========================================================================
// Generic typed event
// ===========================================================================

/// Generic typed event parameterized over an `Operation`. The `Operation`
/// trait already requires Serialize/DeserializeOwned on `Self`, `Scope`, and
/// `Flags`, so the serde-derive's auto-generated bounds are sufficient.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "Op: Operation")]
pub struct Event<Op: Operation> {
    pub id: Uuid,
    pub scope: Op::Scope,
    pub operation: Op,
    pub flags: Op::Flags,
    pub timestamp: DateTime<Utc>,
    pub authentication: AuthenticatedEntity,
}

impl<Op: Operation> Event<Op> {
    pub fn new(scope: Op::Scope, operation: Op, authentication: AuthenticatedEntity) -> Self {
        Self {
            id: Uuid::new_v4(),
            scope,
            operation,
            flags: Op::Flags::default(),
            timestamp: Utc::now(),
            authentication,
        }
    }

    pub fn with_flags(mut self, flags: Op::Flags) -> Self {
        self.flags = flags;
        self
    }

    pub fn discriminant(&self) -> <Op as IntoDiscriminant>::Discriminant {
        self.operation.discriminant()
    }

    pub fn log_label(&self) -> String {
        self.operation.log_label(&self.scope)
    }

    pub fn name(&self) -> Cow<'static, str> {
        self.operation.event_name(&self.scope)
    }

    pub fn attribution(&self) -> Attribution {
        self.operation.attribution(&self.scope)
    }

    /// The event as a consumer outside the server receives it: who acted, this server's version,
    /// and the operation's [`metadata`](Operation::metadata). `site_organization_id` is the
    /// caller's lookup of a [`ScopeOrganization::Site`]; the organization falls back to the
    /// authenticated entity's when the scope yields none.
    pub fn properties(
        &self,
        site_organization_id: Option<Uuid>,
    ) -> EventProperties<impl Serialize + Send + '_> {
        let mut actor = ActorProperties::from(&self.authentication);
        let scope_organization_id = match self.scope.organization() {
            Some(ScopeOrganization::Org(id)) => Some(id),
            Some(ScopeOrganization::Site(_)) => site_organization_id,
            None => None,
        };
        if let Some(id) = scope_organization_id {
            actor.organization_id = Some(id);
        }
        EventProperties {
            actor,
            server_version: own_version(),
            metadata: self.operation.metadata(&self.scope),
        }
    }
}

/// See [`Event::properties`]. `metadata` holds every event-specific field, so the top level
/// is the same for every event.
#[derive(Debug, Serialize)]
pub struct EventProperties<M> {
    #[serde(flatten)]
    pub actor: ActorProperties,
    pub server_version: semver::Version,
    pub metadata: M,
}

impl<M> EventProperties<M> {
    /// Extend the metadata, e.g. with a count a consumer aggregated over several events.
    pub fn map_metadata<N>(self, f: impl FnOnce(M) -> N) -> EventProperties<N> {
        EventProperties {
            actor: self.actor,
            server_version: self.server_version,
            metadata: f(self.metadata),
        }
    }
}

/// Render an event as JSON. The logging subscriber prefixes each line with a
/// `<label>: ` tag, so the JSON payload begins after the first `": "`;
/// downstream consumers (vector / loki) split on that before parsing.
impl<Op: Operation> std::fmt::Display for Event<Op> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match serde_json::to_string(self) {
            Ok(s) => f.write_str(&s),
            Err(e) => write!(f, r#"{{"error":"serialize event: {}"}}"#, e),
        }
    }
}

// ===========================================================================
// Filter trait + per-Op filter shapes
// ===========================================================================

/// A subscriber-declared predicate. Each `Operation` type has an associated
/// `Filter` type that implements this trait — most use the generic
/// `EventFilter<Op>`, but `EntityOperation` uses the richer `EntityEventFilter`.
pub trait SubscriberFilter<Op: Operation>: Default + Clone + Send + Sync + 'static {
    fn matches(&self, event: &Event<Op>) -> bool;
}

/// Default filter — used by Billing, Onboarding, Analytics, Auth, Discovery.
/// Restrict to a set of operation discriminants, or match all when `None`.
#[derive(Debug, Clone)]
pub struct EventFilter<Op: Operation> {
    pub operations: Option<Vec<<Op as IntoDiscriminant>::Discriminant>>,
}

impl<Op: Operation> Default for EventFilter<Op> {
    fn default() -> Self {
        Self::all()
    }
}

impl<Op: Operation> EventFilter<Op> {
    /// Match every event of this operation type.
    pub fn all() -> Self {
        Self { operations: None }
    }

    /// Match only the listed operation discriminants.
    pub fn ops(operations: Vec<<Op as IntoDiscriminant>::Discriminant>) -> Self {
        Self {
            operations: Some(operations),
        }
    }
}

impl<Op: Operation> SubscriberFilter<Op> for EventFilter<Op> {
    fn matches(&self, event: &Event<Op>) -> bool {
        match &self.operations {
            None => true,
            Some(ops) => ops.contains(&event.discriminant()),
        }
    }
}

/// Richer filter for `EntityOperation` — declares per-entity-type op gates so
/// the framework can dispatch only to subscribers that care.
///
/// `entity_discriminants`:
/// - `None` = match all entity events.
/// - `Some(map)`: each entry's presence means "match this entity type"; the
///   inner `Option<Vec<...>>` restricts to the listed ops (or all if `None`).
#[derive(Debug, Clone, Default)]
pub struct EntityEventFilter {
    pub entity_discriminants:
        Option<HashMap<EntityDiscriminants, Option<Vec<EntityOperationDiscriminants>>>>,
}

impl EntityEventFilter {
    /// Match every entity event.
    pub fn all() -> Self {
        Self {
            entity_discriminants: None,
        }
    }

    /// Restrict by entity type and (optionally) by operation per entity type.
    pub fn by_entity(
        map: HashMap<EntityDiscriminants, Option<Vec<EntityOperationDiscriminants>>>,
    ) -> Self {
        Self {
            entity_discriminants: Some(map),
        }
    }
}

impl SubscriberFilter<EntityOperation> for EntityEventFilter {
    fn matches(&self, event: &Event<EntityOperation>) -> bool {
        let Some(map) = &self.entity_discriminants else {
            return true;
        };
        let entity_disc = event.scope.entity_discriminant();
        let Some(allowed_ops) = map.get(&entity_disc) else {
            return false;
        };
        match allowed_ops {
            None => true,
            Some(ops) => ops.contains(&event.discriminant()),
        }
    }
}

// ===========================================================================
// Subscriber trait
// ===========================================================================

/// A subscriber for one operation type. Cross-event subscribers (PostHog,
/// Logging, Metrics, Email) implement this multiple times — once per
/// operation type they care about. Registration on the `EventBus` is handled
/// uniformly via `inventory::submit!(SubscriberRegistration::new::<Self, Op>())`
/// next to the impl block; see `shared/events/registry.rs`.
///
/// Identity: each (Service, Op) pair is named by the registry as
/// `<service_snake>:<op_snake>` — no per-impl `name()` method.
#[async_trait::async_trait]
pub trait Subscriber<Op: Operation>: Send + Sync {
    fn filter(&self) -> Op::Filter;
    /// Apply `events`. Report every failure as `Err`; never log and continue, or the failure
    /// reaches neither the bus retry nor `scanopy_event_subscriber_errors_total`.
    ///
    /// A plain `Err` tells the bus that calling `handle` again with the same events is safe, and
    /// it retries. Return [`NonRetryable`] instead when side effects already applied would repeat
    /// on a re-run (some emails of a batch sent, a non-idempotent create done); the bus counts it
    /// and does not retry.
    async fn handle(&self, events: Vec<Event<Op>>) -> anyhow::Result<()>;
    fn debounce_window_ms(&self) -> u64 {
        0
    }
}

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
        tracing::error!(subscriber = name, panic = message, "Typed subscriber panicked");
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
    use crate::server::shared::events::types::OnboardingOperation;

    use crate::server::shared::storage::lock::{LockError, LockKey};
    use std::sync::atomic::{AtomicUsize, Ordering};

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
            .publish(onboarding_event(OnboardingOperation::OnboardingModalCompleted))
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
                .publish(onboarding_event(OnboardingOperation::OnboardingModalCompleted))
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
                .publish(onboarding_event(OnboardingOperation::OnboardingModalCompleted))
                .await
                .unwrap();
        }
        tokio::time::timeout(Duration::from_secs(5), channel.flush())
            .await
            .expect("flush drains the batch without waiting an hour");
        assert_eq!(handled.load(Ordering::SeqCst), 3);
    }
}
