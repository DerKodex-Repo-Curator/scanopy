//! The newest promoted Scanopy release, for the upgrade banner on self-hosted
//! servers.
//!
//! `promote_to_latest.yml` marks a release as GitHub's latest in the same run
//! that retags `server:latest`, so GitHub's `releases/latest` names the image
//! `docker compose pull` fetches. Prereleases and drafts never appear there.
//!
//! A background loop writes the answer to [`AppCache`] and `/config` reads it,
//! so no request waits on GitHub. A failed check logs at `debug` and leaves
//! the cache alone.

use std::time::Duration;

use rand::Rng;
use semver::Version;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::server::metrics::dependency::{Dependency, observe_http};
use crate::server::openapi::SERVER_VERSION;
use crate::server::release_notes::catalog::CHANGELOG_URL;
use crate::server::shared::handlers::cache::AppCache;

/// GitHub's latest release for the Scanopy repository.
const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/scanopy/scanopy/releases/latest";

const CACHE_KEY: &str = "github_latest_release";

/// Base wait between checks. Each wait adds up to `CHECK_JITTER`.
const CHECK_INTERVAL: Duration = Duration::from_secs(12 * 60 * 60);
const CHECK_JITTER: Duration = Duration::from_secs(30 * 60);
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// How long an answer stays cached: through two failed checks after it, then
/// the banner drops.
const CACHE_TTL_HOURS: u64 = 36;

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

/// A newer release than the one this server runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AvailableUpgrade {
    /// Semver of the newest promoted release, without a leading `v`.
    pub version: String,
    /// That release's page on the public changelog.
    #[schema(format = "uri")]
    pub release_notes_url: String,
}

/// Whether `latest` is an upgrade for a server running `running`. A server
/// running a prerelease of `latest` counts as behind it; a server running
/// ahead of `latest` does not.
pub fn upgrade_available(running: &Version, latest: &Version) -> bool {
    latest > running
}

/// Ask `url` for the latest release and parse its tag.
async fn fetch_latest_release(http: &reqwest::Client, url: &str) -> Result<Version, anyhow::Error> {
    let send = http.get(url).header("User-Agent", "Scanopy").send();
    let response = observe_http(Dependency::Github, send)
        .await?
        .error_for_status()?;
    let release: GitHubRelease = response.json().await?;
    let tag = release.tag_name.trim();
    Ok(Version::parse(tag.strip_prefix('v').unwrap_or(tag))?)
}

/// Fetch the latest release and cache it. A failure leaves the cache as it was.
pub async fn refresh_latest_release(cache: &AppCache, http: &reqwest::Client, url: &str) {
    match fetch_latest_release(http, url).await {
        Ok(version) => cache.set(CACHE_KEY, version, CACHE_TTL_HOURS).await,
        Err(e) => tracing::debug!(error = %e, "Latest release check failed"),
    }
}

/// Check now, then every `CHECK_INTERVAL` plus jitter. Self-hosted servers
/// only; the caller spawns this.
pub async fn run_latest_release_checks(cache: std::sync::Arc<AppCache>) {
    let http = match reqwest::Client::builder().timeout(CHECK_TIMEOUT).build() {
        Ok(http) => http,
        Err(e) => {
            tracing::debug!(error = %e, "Could not build the latest release client");
            return;
        }
    };
    loop {
        refresh_latest_release(&cache, &http, LATEST_RELEASE_URL).await;
        let jitter = rand::rng().random_range(Duration::ZERO..=CHECK_JITTER);
        tokio::time::sleep(CHECK_INTERVAL + jitter).await;
    }
}

/// The cached latest release, when it is newer than this server.
pub async fn available_upgrade(cache: &AppCache) -> Option<AvailableUpgrade> {
    let latest = cache.get::<Version>(CACHE_KEY).await?;
    let running = Version::parse(SERVER_VERSION).expect("CARGO_PKG_VERSION is semver");
    upgrade_available(&running, &latest).then(|| AvailableUpgrade {
        release_notes_url: format!("{CHANGELOG_URL}/v{latest}"),
        version: latest.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn only_a_strictly_newer_release_is_an_upgrade() {
        assert!(upgrade_available(&v("0.18.1"), &v("0.18.2")));
        assert!(upgrade_available(&v("0.18.1"), &v("0.19.0")));
        assert!(!upgrade_available(&v("0.18.1"), &v("0.18.1")));
        assert!(!upgrade_available(&v("0.18.2"), &v("0.18.1")));
    }

    #[test]
    fn a_prerelease_server_is_behind_its_release() {
        assert!(upgrade_available(&v("0.18.2-rc.1"), &v("0.18.2")));
        assert!(!upgrade_available(&v("0.18.3-rc.1"), &v("0.18.2")));
    }

    /// A stand-in for GitHub answering every request with `respond()`.
    async fn github(respond: impl Fn() -> Response + Clone + Send + Sync + 'static) -> String {
        let app = axum::Router::new().route(
            "/latest",
            axum::routing::get(move || {
                let response = respond();
                async move { response }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}/latest")
    }

    fn release(tag: &'static str) -> impl Fn() -> Response + Clone + Send + Sync + 'static {
        move || axum::Json(serde_json::json!({ "tag_name": tag })).into_response()
    }

    async fn cached(cache: &AppCache) -> Option<Version> {
        cache.get::<Version>(CACHE_KEY).await
    }

    #[tokio::test]
    async fn a_good_answer_replaces_the_cached_release() {
        let cache = AppCache::new();
        let http = reqwest::Client::new();
        cache.set(CACHE_KEY, v("0.18.0"), 1).await;

        refresh_latest_release(&cache, &http, &github(release("v99.0.0")).await).await;

        assert_eq!(cached(&cache).await, Some(v("99.0.0")));
        assert_eq!(
            available_upgrade(&cache).await.map(|u| u.version),
            Some("99.0.0".to_string())
        );
    }

    #[tokio::test]
    async fn a_failed_check_keeps_the_cached_release() {
        let unreachable = {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            drop(listener);
            format!("http://{addr}/latest")
        };
        let failures = [
            github(|| (StatusCode::INTERNAL_SERVER_ERROR, "unavailable").into_response()).await,
            github(|| (StatusCode::FORBIDDEN, "rate limited").into_response()).await,
            github(|| "not json".into_response()).await,
            github(release("nightly")).await,
            unreachable,
        ];
        let http = reqwest::Client::new();

        for url in failures {
            let cache = AppCache::new();
            cache.set(CACHE_KEY, v("99.0.0"), 1).await;
            refresh_latest_release(&cache, &http, &url).await;
            assert_eq!(cached(&cache).await, Some(v("99.0.0")), "{url}");
        }
    }

    #[tokio::test]
    async fn a_server_at_the_latest_release_has_no_upgrade() {
        let cache = AppCache::new();
        assert_eq!(available_upgrade(&cache).await, None);

        cache.set(CACHE_KEY, v(SERVER_VERSION), 1).await;
        assert_eq!(available_upgrade(&cache).await, None);
    }
}
