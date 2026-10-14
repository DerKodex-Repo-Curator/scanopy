use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use semver::Version;

use crate::server::{
    email::{EmailService, messages::ReleaseNotes, rate_limit},
    release_notes::{
        catalog::{Catalog, MAX_SHOWN, ReleaseNote},
        storage::{ReleaseAnnouncement, ReleaseAnnouncementBase},
    },
    shared::{
        services::traits::CrudService,
        storage::{
            filter::StorableFilter,
            generic::GenericPostgresStorage,
            lock::{DEFAULT_LOCK_TIMEOUT, LockKey},
            traits::{Storable, Storage},
        },
    },
    users::{r#impl::base::User, service::UserService},
};

/// Release notes for the modal, and the once-per-version release email.
pub struct ReleaseNotesService {
    storage: Arc<GenericPostgresStorage<ReleaseAnnouncement>>,
    user_service: Arc<UserService>,
    email_service: Option<Arc<EmailService>>,
    /// Paces the release email below the transport's own limit, so other
    /// emails keep part of the quota while it goes out.
    bulk_limiter: DefaultDirectRateLimiter,
    catalog: &'static Catalog,
    running: Version,
    /// `SCANOPY_HOLD_RELEASE_EMAIL`: leave an emailed release for a later start.
    hold_email: bool,
}

/// Whether a start may send the release email now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmailReadiness {
    Ready,
    NotConfigured,
    /// Held by `SCANOPY_HOLD_RELEASE_EMAIL`.
    Held,
}

/// Releases to show, newest first, and whether older ones were left out.
#[derive(Debug, PartialEq, Eq)]
pub struct Shown<'a> {
    pub notes: Vec<&'a ReleaseNote>,
    pub truncated: bool,
}

impl<'a> Shown<'a> {
    fn capped(mut notes: Vec<&'a ReleaseNote>) -> Self {
        let truncated = notes.len() > MAX_SHOWN;
        notes.truncate(MAX_SHOWN);
        Self { notes, truncated }
    }
}

/// A user's unseen releases, and whether they open the modal on their own.
#[derive(Debug, PartialEq, Eq)]
pub struct Unseen<'a> {
    pub shown: Shown<'a>,
    pub auto_open: bool,
}

/// What starting on the running version calls for.
#[derive(Debug, PartialEq, Eq)]
enum Plan<'a> {
    /// Already recorded, or older than a recorded version.
    Nothing,
    /// Record the version, and send these notes when there are any.
    Record(Option<Shown<'a>>),
}

/// Decide what starting on `running` calls for, given every recorded version.
///
/// No recorded version and no users is a fresh install: it records its
/// version and sends nothing. No recorded version with users is an upgrade
/// from a release before this table existed, and is announced like any other.
fn plan<'a>(
    running: &Version,
    recorded: &[ReleaseAnnouncement],
    users_exist: bool,
    catalog: &'a Catalog,
) -> Plan<'a> {
    let versions: Vec<(Version, bool)> = recorded
        .iter()
        .filter_map(|row| {
            let version = Version::parse(&row.base.version).ok()?;
            Some((version, row.base.email_sent_at.is_some()))
        })
        .collect();

    if versions.iter().any(|(version, _)| version >= running) {
        return Plan::Nothing;
    }
    if versions.is_empty() && !users_exist {
        return Plan::Record(None);
    }
    match catalog.get(running) {
        Some(note) if note.email => {}
        Some(_) => return Plan::Record(None),
        None => {
            tracing::warn!(version = %running, "No release notes for this version; sending no release email");
            return Plan::Record(None);
        }
    }

    // The email covers every release since the last one emailed, or since the
    // oldest version on record when none has been.
    let last_emailed = versions
        .iter()
        .filter(|(_, emailed)| *emailed)
        .map(|(v, _)| v)
        .max();
    let since = last_emailed.or_else(|| versions.iter().map(|(v, _)| v).min());
    let notes = match since {
        Some(since) => catalog.between(since, running),
        None => catalog.get(running).into_iter().collect(),
    };
    Plan::Record(Some(Shown::capped(notes)))
}

/// The releases after `seen` up to `running`, or `running` alone when nothing
/// is seen yet. They open the modal when any of them asks for it, even one
/// past the cap.
fn unseen<'a>(catalog: &'a Catalog, seen: Option<&Version>, running: &Version) -> Unseen<'a> {
    let notes = match seen {
        Some(seen) => catalog.between(seen, running),
        None => catalog.get(running).into_iter().collect(),
    };
    Unseen {
        auto_open: notes.iter().any(|note| note.modal),
        shown: Shown::capped(notes),
    }
}

impl ReleaseNotesService {
    pub fn new(
        storage: Arc<GenericPostgresStorage<ReleaseAnnouncement>>,
        user_service: Arc<UserService>,
        email_service: Option<Arc<EmailService>>,
        bulk_quota: Quota,
        hold_email: bool,
    ) -> Self {
        let mut service = Self::with_catalog(
            storage,
            user_service,
            email_service,
            bulk_quota,
            Catalog::embedded(),
            Version::parse(env!("CARGO_PKG_VERSION")).expect("CARGO_PKG_VERSION is semver"),
        );
        service.hold_email = hold_email;
        service
    }

    fn with_catalog(
        storage: Arc<GenericPostgresStorage<ReleaseAnnouncement>>,
        user_service: Arc<UserService>,
        email_service: Option<Arc<EmailService>>,
        bulk_quota: Quota,
        catalog: &'static Catalog,
        running: Version,
    ) -> Self {
        Self {
            storage,
            user_service,
            email_service,
            bulk_limiter: RateLimiter::direct(bulk_quota),
            catalog,
            running,
            hold_email: false,
        }
    }

    pub fn running_version(&self) -> &Version {
        &self.running
    }

    /// The releases `user` hasn't dismissed, up to the running version. A user
    /// from before release notes existed sees the running release only.
    ///
    /// The modal opens on its own when any unseen release asks for it, and then
    /// lists every unseen release. Releases that don't ask for it stay unseen
    /// until it opens.
    pub fn unseen(&self, user: &User) -> Unseen<'static> {
        let seen = user
            .base
            .release_notes_seen_version
            .as_deref()
            .and_then(|v| Version::parse(v).ok());
        unseen(self.catalog, seen.as_ref(), &self.running)
    }

    /// Record that `user` has seen every release up to the running one.
    pub async fn mark_seen(&self, user: User) -> Result<()> {
        self.user_service
            .mark_release_notes_seen(user, &self.running.to_string())
            .await
    }

    /// Record the running version and send its release email, once per
    /// version across every container. Never fails startup: errors are logged.
    pub async fn announce_running_version(&self) {
        let readiness = if self.email_service.is_none() {
            EmailReadiness::NotConfigured
        } else if self.hold_email {
            EmailReadiness::Held
        } else {
            EmailReadiness::Ready
        };
        let claimed = match self.claim(readiness).await {
            Ok(claimed) => claimed,
            Err(e) => {
                tracing::error!(error = %e, version = %self.running, "Could not record the server version");
                return;
            }
        };
        if let Some((row, shown)) = claimed
            && let Err(e) = self.send(row, shown).await
        {
            tracing::error!(error = %e, version = %self.running, "Release email failed");
        }
    }

    /// Record the running version unless it already is. Returns the new row
    /// and the notes to email when this call recorded it and the release asks
    /// for an email. Serialized by an advisory lock, so of several containers
    /// starting together exactly one records the version.
    ///
    /// A release that asks for an email is left unrecorded unless email is
    /// `Ready`, so the first start that can send it still does.
    async fn claim(
        &self,
        readiness: EmailReadiness,
    ) -> Result<Option<(ReleaseAnnouncement, Shown<'static>)>> {
        let guard = self
            .storage
            .session_lock(LockKey::ReleaseAnnouncement, DEFAULT_LOCK_TIMEOUT)
            .await?;
        let result = self.claim_locked(readiness).await;
        guard.release().await?;
        result
    }

    async fn claim_locked(
        &self,
        readiness: EmailReadiness,
    ) -> Result<Option<(ReleaseAnnouncement, Shown<'static>)>> {
        let recorded = self
            .storage
            .get_all(StorableFilter::new_for_server_wide_job())
            .await?;
        let users_exist = self.user_service.any_exist().await?;
        let email = match plan(&self.running, &recorded, users_exist, self.catalog) {
            Plan::Nothing => return Ok(None),
            Plan::Record(email) => email,
        };
        if email.is_some() {
            match readiness {
                EmailReadiness::Ready => {}
                EmailReadiness::NotConfigured => {
                    tracing::warn!(
                        version = %self.running,
                        "Email is not configured; the release email waits for a start with email configured"
                    );
                    return Ok(None);
                }
                EmailReadiness::Held => {
                    tracing::info!(
                        version = %self.running,
                        "Release email held by SCANOPY_HOLD_RELEASE_EMAIL; it sends on the first start without it"
                    );
                    return Ok(None);
                }
            }
        }
        let row = self
            .storage
            .create(&ReleaseAnnouncement::new(ReleaseAnnouncementBase {
                version: self.running.to_string(),
                email_sent_at: None,
            }))
            .await?;
        tracing::info!(version = %self.running, email = email.is_some(), "Recorded a new server version");
        Ok(email.map(|shown| (row, shown)))
    }

    /// Email `shown` to every verified user, paced by the bulk limiter. A
    /// relay that keeps refusing for now (a daily quota, an outage) stops the
    /// run rather than failing every remaining recipient the same way.
    async fn send(&self, mut row: ReleaseAnnouncement, shown: Shown<'static>) -> Result<()> {
        let Some(email_service) = &self.email_service else {
            tracing::info!("Email is not configured; the release email was not sent");
            return Ok(());
        };
        let users = self
            .user_service
            .get_all(StorableFilter::new_for_server_wide_job())
            .await?;
        let recipients: Vec<&User> = users.iter().filter(|u| u.base.email_verified).collect();
        let email = ReleaseNotes {
            notes: &shown.notes,
            truncated: shown.truncated,
        };

        let (mut processed, mut failed) = (0usize, 0usize);
        for user in &recipients {
            self.bulk_limiter.until_ready().await;
            match email_service
                .send_release_notes(user.base.email.clone(), &email)
                .await
            {
                Ok(()) => processed += 1,
                Err(e) if rate_limit::is_transient(&e) => {
                    tracing::error!(
                        error = %e,
                        processed,
                        unsent = recipients.len() - processed - failed,
                        "The relay keeps refusing the release email; stopping"
                    );
                    break;
                }
                Err(e) => {
                    failed += 1;
                    tracing::warn!(user_id = %user.id, error = %e, "Release email not sent");
                }
            }
        }
        tracing::info!(processed, failed, recipients = recipients.len(), version = %self.running, "Release email done");

        row.base.email_sent_at = Some(Utc::now());
        row.updated_at = Utc::now();
        self.storage.update(&mut row).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::release_notes::catalog::tests::{note_file, note_file_with};
    use crate::server::shared::storage::factory::StorageFactory;
    use crate::server::users::r#impl::base::UserBase;
    use crate::tests::test_storage;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn row(version: &str, emailed: bool) -> ReleaseAnnouncement {
        ReleaseAnnouncement::new(ReleaseAnnouncementBase {
            version: version.into(),
            email_sent_at: emailed.then(Utc::now),
        })
    }

    /// Releases 0.18.0 through 0.18.7 (email off) and 0.19.0 (email on).
    fn catalog() -> &'static Catalog {
        let mut files: Vec<_> = (0..=7)
            .map(|p| note_file(&format!("0.18.{p}"), false))
            .collect();
        files.push(note_file("0.19.0", true));
        Box::leak(Box::new(Catalog::from_files(files)))
    }

    fn emailed(plan: Plan<'_>) -> Option<Vec<String>> {
        match plan {
            Plan::Record(Some(shown)) => {
                Some(shown.notes.iter().map(|n| n.version.to_string()).collect())
            }
            Plan::Record(None) => Some(vec![]),
            Plan::Nothing => None,
        }
    }

    #[test]
    fn a_recorded_or_older_version_does_nothing() {
        let catalog = catalog();
        assert_eq!(
            plan(&v("0.19.0"), &[row("0.19.0", true)], true, catalog),
            Plan::Nothing
        );
        assert_eq!(
            plan(&v("0.18.7"), &[row("0.19.0", true)], true, catalog),
            Plan::Nothing
        );
    }

    #[test]
    fn a_fresh_install_records_without_email() {
        assert_eq!(
            plan(&v("0.19.0"), &[], false, catalog()),
            Plan::Record(None)
        );
    }

    #[test]
    fn the_first_upgrade_after_this_feature_announces_the_running_release() {
        assert_eq!(
            emailed(plan(&v("0.19.0"), &[], true, catalog())),
            Some(vec!["0.19.0".into()])
        );
    }

    #[test]
    fn a_release_without_email_records_only() {
        assert_eq!(
            plan(&v("0.18.3"), &[row("0.18.2", false)], true, catalog()),
            Plan::Record(None)
        );
    }

    /// The next emailed release carries every release since the last email,
    /// capped, with the overflow flagged.
    #[test]
    fn an_emailed_release_covers_everything_since_the_last_email() {
        let catalog = catalog();
        let recorded = [
            row("0.18.0", true),
            row("0.18.4", false),
            row("0.18.6", false),
        ];
        match plan(&v("0.19.0"), &recorded, true, catalog) {
            Plan::Record(Some(shown)) => {
                let versions: Vec<_> = shown.notes.iter().map(|n| n.version.to_string()).collect();
                assert_eq!(versions, ["0.19.0", "0.18.7", "0.18.6", "0.18.5", "0.18.4"]);
                assert!(
                    shown.truncated,
                    "0.18.1 to 0.18.3 are left for the changelog"
                );
            }
            other => panic!("expected an email, got {other:?}"),
        }
    }

    fn service(storage: &StorageFactory, running: &str) -> ReleaseNotesService {
        let user_service = Arc::new(UserService::new(
            storage.users.clone(),
            storage.user_site_access.clone(),
            Arc::new(crate::server::shared::events::bus::EventBus::new()),
        ));
        ReleaseNotesService::with_catalog(
            storage.release_announcements.clone(),
            user_service,
            None,
            rate_limit::per_second(1000),
            catalog(),
            v(running),
        )
    }

    /// Containers starting on the same new version at once: exactly one
    /// records it and goes on to send, and a restart afterwards records nothing.
    #[tokio::test]
    async fn concurrent_starts_claim_the_version_once() {
        let (storage, _container) = test_storage().await;
        storage
            .release_announcements
            .create(&row("0.18.0", true))
            .await
            .unwrap();

        let services: Vec<_> = (0..8).map(|_| service(&storage, "0.19.0")).collect();
        let claims =
            futures::future::join_all(services.iter().map(|s| s.claim(EmailReadiness::Ready)))
                .await;
        let claimed = claims
            .into_iter()
            .map(|claim| claim.unwrap())
            .filter(Option::is_some)
            .count();
        assert_eq!(claimed, 1);

        let recorded = storage
            .release_announcements
            .get_all(StorableFilter::new_for_server_wide_job())
            .await
            .unwrap();
        assert_eq!(recorded.len(), 2);

        assert!(
            service(&storage, "0.19.0")
                .claim(EmailReadiness::Ready)
                .await
                .unwrap()
                .is_none()
        );
    }

    /// A start that can't send yet, without email configured or with the email
    /// held, leaves an emailed release pending, so the next start that can send
    /// it does instead of finding it recorded.
    #[tokio::test]
    async fn an_emailed_release_waits_for_a_start_that_can_send_it() {
        let (storage, _container) = test_storage().await;
        storage
            .release_announcements
            .create(&row("0.18.0", true))
            .await
            .unwrap();
        let service = service(&storage, "0.19.0");

        for readiness in [EmailReadiness::NotConfigured, EmailReadiness::Held] {
            assert!(service.claim(readiness).await.unwrap().is_none());
        }
        let recorded = storage
            .release_announcements
            .get_all(StorableFilter::new_for_server_wide_job())
            .await
            .unwrap();
        assert_eq!(recorded.len(), 1, "0.19.0 stays unrecorded");

        assert!(
            service
                .claim(EmailReadiness::Ready)
                .await
                .unwrap()
                .is_some()
        );
    }

    /// Dismissing advances the user to the running version, and the stored
    /// value survives the reload a second device would do.
    #[tokio::test]
    async fn unseen_spans_the_seen_version_to_the_running_one() {
        let (storage, _container) = test_storage().await;
        let service = service(&storage, "0.18.6");
        let versions = |user: &User| -> Vec<String> {
            let unseen = service.unseen(user);
            unseen
                .shown
                .notes
                .iter()
                .map(|n| n.version.to_string())
                .collect()
        };

        let mut user = User::new(UserBase::default());
        user.base.release_notes_seen_version = Some("0.18.4".into());
        assert_eq!(versions(&user), ["0.18.6", "0.18.5"]);

        user.base.release_notes_seen_version = None;
        assert_eq!(
            versions(&user),
            ["0.18.6"],
            "a user from before release notes"
        );

        let organization = storage
            .organizations
            .create(&crate::tests::organization())
            .await
            .unwrap();
        let user = storage
            .users
            .create(&crate::tests::user(&organization.id))
            .await
            .unwrap();
        service.mark_seen(user.clone()).await.unwrap();
        let reloaded = storage.users.get_by_id(&user.id).await.unwrap().unwrap();
        assert!(versions(&reloaded).is_empty());
    }

    /// `auto_open` and the versions listed for a user who last saw `seen`
    /// (`None`: from before release notes), over releases 0.18.0 through
    /// 0.18.8 running 0.18.8, with `modal` on only for `with_modal`.
    fn unseen_from(with_modal: &[&str], seen: Option<&str>) -> (bool, Vec<String>) {
        let files = (0..=8).map(|p| {
            let version = format!("0.18.{p}");
            let modal = with_modal.contains(&version.as_str());
            note_file_with(&version, &format!("email: false\nmodal: {modal}"))
        });
        let catalog = Catalog::from_files(files);
        let unseen = unseen(&catalog, seen.map(v).as_ref(), &v("0.18.8"));
        let versions = unseen
            .shown
            .notes
            .iter()
            .map(|n| n.version.to_string())
            .collect();
        (unseen.auto_open, versions)
    }

    #[test]
    fn a_single_release_opens_the_modal_only_when_it_asks_to() {
        for seen in [Some("0.18.7"), None] {
            let (auto_open, versions) = unseen_from(&["0.18.8"], seen);
            assert!(auto_open);
            assert_eq!(versions, ["0.18.8"]);

            let (auto_open, versions) = unseen_from(&[], seen);
            assert!(!auto_open);
            assert_eq!(versions, ["0.18.8"], "the notes are still there to open");
        }
    }

    /// Across missed releases the modal opens when any of them asks for it,
    /// even one past the cap, and lists the ones that didn't ask too.
    #[test]
    fn missed_releases_open_the_modal_when_any_asks_to() {
        let (auto_open, versions) = unseen_from(&[], Some("0.18.5"));
        assert!(!auto_open);
        assert_eq!(versions, ["0.18.8", "0.18.7", "0.18.6"]);

        let (auto_open, versions) = unseen_from(&["0.18.6"], Some("0.18.5"));
        assert!(auto_open);
        assert_eq!(versions, ["0.18.8", "0.18.7", "0.18.6"]);

        let (auto_open, _) = unseen_from(&["0.18.1"], Some("0.18.0"));
        assert!(
            auto_open,
            "0.18.1 is past the cap but still asks for the modal"
        );
    }

    /// A user created now starts at the running release, so the release it
    /// signed up on never shows as new.
    #[test]
    fn a_new_user_has_nothing_unseen() {
        let running = v(env!("CARGO_PKG_VERSION"));
        let catalog: &'static Catalog = Box::leak(Box::new(Catalog::from_files([note_file(
            &running.to_string(),
            false,
        )])));
        let user = User::new(UserBase::new_password(
            email_address::EmailAddress::new_unchecked("new@example.com"),
            true,
            "hash".into(),
            uuid::Uuid::new_v4(),
            crate::server::users::r#impl::permissions::UserOrgPermissions::Member,
            vec![],
            None,
        ));
        let seen =
            Version::parse(user.base.release_notes_seen_version.as_deref().unwrap()).unwrap();
        assert!(catalog.between(&seen, &running).is_empty());
    }
}
