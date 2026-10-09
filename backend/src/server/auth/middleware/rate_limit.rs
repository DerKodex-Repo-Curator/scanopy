use crate::server::config::AppState;
#[cfg(not(feature = "generate-fixtures"))]
use crate::server::{auth::middleware::auth::AuthenticatedEntity, shared::types::api::ApiError};
#[cfg(not(feature = "generate-fixtures"))]
use axum::{extract::FromRequestParts, response::IntoResponse};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use axum_client_ip::ClientIp;
#[cfg(not(feature = "generate-fixtures"))]
use governor::{
    Quota, RateLimiter,
    clock::{Clock, DefaultClock},
    state::keyed::DashMapStateStore,
};
use std::sync::Arc;
#[cfg(not(feature = "generate-fixtures"))]
use std::{net::IpAddr, num::NonZeroU32, sync::OnceLock, time::Duration};
#[cfg(not(feature = "generate-fixtures"))]
use uuid::Uuid;

#[cfg(not(feature = "generate-fixtures"))]
use crate::server::email::handlers::{SNS_FEEDBACK_PATH, UNSUBSCRIBE_PATH_PREFIX};

#[cfg(not(feature = "generate-fixtures"))]
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum RateLimitKey {
    User(Uuid),
    Ip(IpAddr),
}

#[cfg(not(feature = "generate-fixtures"))]
type KeyedRateLimiter =
    Arc<RateLimiter<RateLimitKey, DashMapStateStore<RateLimitKey>, DefaultClock>>;

#[cfg(not(feature = "generate-fixtures"))]
struct RateLimiters {
    user: KeyedRateLimiter,
    anonymous: KeyedRateLimiter,
    demo: KeyedRateLimiter,
    external_service: KeyedRateLimiter,
}

#[cfg(not(feature = "generate-fixtures"))]
static RATE_LIMITERS: OnceLock<RateLimiters> = OnceLock::new();

#[cfg(not(feature = "generate-fixtures"))]
fn get_limiters() -> &'static RateLimiters {
    RATE_LIMITERS.get_or_init(|| {
        let limiters = RateLimiters {
            // Authenticated users and API keys: 300 requests per minute with burst of 150
            user: Arc::new(RateLimiter::keyed(
                Quota::per_minute(NonZeroU32::new(300).unwrap())
                    .allow_burst(NonZeroU32::new(150).unwrap()),
            )),
            // Anonymous/unauthenticated: 40 requests per minute with burst of 10
            anonymous: Arc::new(RateLimiter::keyed(
                Quota::per_minute(NonZeroU32::new(40).unwrap())
                    .allow_burst(NonZeroU32::new(10).unwrap()),
            )),
            // Demo users: 150 requests per minute with burst of 50
            // Shared demo account uses IP-based limiting with higher limits than anonymous
            demo: Arc::new(RateLimiter::keyed(
                Quota::per_minute(NonZeroU32::new(150).unwrap())
                    .allow_burst(NonZeroU32::new(50).unwrap()),
            )),
            // External services (Prometheus, etc.): 60 requests per minute with burst of 10
            // Sufficient for typical 15-30 second scrape intervals
            external_service: Arc::new(RateLimiter::keyed(
                Quota::per_minute(NonZeroU32::new(60).unwrap())
                    .allow_burst(NonZeroU32::new(10).unwrap()),
            )),
        };

        // Spawn cleanup task
        let user_limiter = Arc::clone(&limiters.user);
        let anonymous_limiter = Arc::clone(&limiters.anonymous);
        let demo_limiter = Arc::clone(&limiters.demo);
        let external_service_limiter = Arc::clone(&limiters.external_service);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                user_limiter.retain_recent();
                anonymous_limiter.retain_recent();
                demo_limiter.retain_recent();
                external_service_limiter.retain_recent();
                tracing::debug!(
                    "Rate limiter cleanup: user keys={}, anonymous keys={}, demo keys={}, external_service keys={}",
                    user_limiter.len(),
                    anonymous_limiter.len(),
                    demo_limiter.len(),
                    external_service_limiter.len()
                );
            }
        });

        limiters
    })
}

#[cfg(not(feature = "generate-fixtures"))]
#[derive(Debug, Clone)]
struct RateLimitInfo {
    limit: u32,
    remaining: u32,
    reset_in_secs: u64,
}

#[cfg(not(feature = "generate-fixtures"))]
impl RateLimitInfo {
    fn apply_headers(&self, response: &mut Response) {
        let headers = response.headers_mut();
        if let Ok(v) = self.limit.to_string().parse() {
            headers.insert("X-RateLimit-Limit", v);
        }
        if let Ok(v) = self.remaining.to_string().parse() {
            headers.insert("X-RateLimit-Remaining", v);
        }
        if let Ok(v) = self.reset_in_secs.to_string().parse() {
            headers.insert("X-RateLimit-Reset", v);
        }
    }

    fn to_error_response(&self) -> Response {
        let mut response = ApiError::too_many_requests(format!(
            "Rate limit exceeded. Try again in {} seconds.",
            self.reset_in_secs
        ))
        .into_response();

        self.apply_headers(&mut response);

        if let Ok(v) = self.reset_in_secs.to_string().parse() {
            response.headers_mut().insert("Retry-After", v);
        }

        response
    }
}

#[cfg(not(feature = "generate-fixtures"))]
fn check_user(user_id: Uuid) -> Result<RateLimitInfo, RateLimitInfo> {
    let limiters = get_limiters();
    let key = RateLimitKey::User(user_id);

    match limiters.user.check_key(&key) {
        Ok(_) => Ok(RateLimitInfo {
            limit: 100,
            remaining: 99,
            reset_in_secs: 60,
        }),
        Err(not_until) => {
            let wait_duration = not_until.wait_time_from(DefaultClock::default().now());
            let wait_time = wait_duration.as_secs() + u64::from(wait_duration.subsec_nanos() > 0);
            Err(RateLimitInfo {
                limit: 100,
                remaining: 0,
                reset_in_secs: wait_time,
            })
        }
    }
}

#[cfg(not(feature = "generate-fixtures"))]
fn check_anonymous(ip: IpAddr) -> Result<RateLimitInfo, RateLimitInfo> {
    let limiters = get_limiters();
    let key = RateLimitKey::Ip(ip);

    match limiters.anonymous.check_key(&key) {
        Ok(_) => Ok(RateLimitInfo {
            limit: 40,
            remaining: 39,
            reset_in_secs: 60,
        }),
        Err(not_until) => {
            let wait_duration = not_until.wait_time_from(DefaultClock::default().now());
            let wait_time = wait_duration.as_secs() + u64::from(wait_duration.subsec_nanos() > 0);
            Err(RateLimitInfo {
                limit: 40,
                remaining: 0,
                reset_in_secs: wait_time,
            })
        }
    }
}

#[cfg(not(feature = "generate-fixtures"))]
fn check_demo(ip: IpAddr) -> Result<RateLimitInfo, RateLimitInfo> {
    let limiters = get_limiters();
    let key = RateLimitKey::Ip(ip);

    match limiters.demo.check_key(&key) {
        Ok(_) => Ok(RateLimitInfo {
            limit: 150,
            remaining: 149,
            reset_in_secs: 60,
        }),
        Err(not_until) => {
            let wait_duration = not_until.wait_time_from(DefaultClock::default().now());
            let wait_time = wait_duration.as_secs() + u64::from(wait_duration.subsec_nanos() > 0);
            Err(RateLimitInfo {
                limit: 150,
                remaining: 0,
                reset_in_secs: wait_time,
            })
        }
    }
}

#[cfg(not(feature = "generate-fixtures"))]
fn check_external_service(ip: IpAddr) -> Result<RateLimitInfo, RateLimitInfo> {
    let limiters = get_limiters();
    let key = RateLimitKey::Ip(ip);

    match limiters.external_service.check_key(&key) {
        Ok(_) => Ok(RateLimitInfo {
            limit: 60,
            remaining: 59,
            reset_in_secs: 60,
        }),
        Err(not_until) => {
            let wait_duration = not_until.wait_time_from(DefaultClock::default().now());
            let wait_time = wait_duration.as_secs() + u64::from(wait_duration.subsec_nanos() > 0);
            Err(RateLimitInfo {
                limit: 60,
                remaining: 0,
                reset_in_secs: wait_time,
            })
        }
    }
}

pub async fn rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    ClientIp(ip): ClientIp,
    request: Request,
    next: Next,
) -> Result<Response, Response> {
    #[cfg(feature = "generate-fixtures")]
    {
        let _ = (state, ip);
        Ok(next.run(request).await)
    }

    #[cfg(not(feature = "generate-fixtures"))]
    {
        let path = request.uri().path();

        let exempt_paths = [
            "/api/billing/webhooks",
            "/api/config",
            "/api/auth/me",
            SNS_FEEDBACK_PATH,
        ];

        // Exempt static file serving, billing webhooks, SES feedback, config and metadata,
        // and one-click unsubscribes, which mail providers POST from shared addresses
        if !path.starts_with("/api/")
            || exempt_paths.contains(&path)
            || path.starts_with(UNSUBSCRIBE_PATH_PREFIX)
        {
            return Ok(next.run(request).await);
        }

        let (mut parts, body) = request.into_parts();

        let entity = AuthenticatedEntity::from_request_parts(&mut parts, &state)
            .await
            .ok();

        // Daemons and System are exempt from rate limiting
        if let Some(ref e) = entity
            && matches!(
                e,
                AuthenticatedEntity::Daemon { .. } | AuthenticatedEntity::System
            )
        {
            let request = Request::from_parts(parts, body);
            return Ok(next.run(request).await);
        }

        let check_result = match entity {
            Some(AuthenticatedEntity::User { user_id, .. }) => {
                // Shared user account, rely on IP limiting
                use crate::server::organizations::handlers::DEMO_USER_ID;
                if user_id == DEMO_USER_ID {
                    check_demo(ip)
                } else {
                    check_user(user_id)
                }
            }
            Some(AuthenticatedEntity::ApiKey { user_id, .. }) => check_user(user_id),
            Some(AuthenticatedEntity::ExternalService { .. }) => check_external_service(ip),
            _ => check_anonymous(ip),
        };

        match check_result {
            Ok(info) => {
                let request = Request::from_parts(parts, body);
                let mut response = next.run(request).await;
                info.apply_headers(&mut response);
                Ok(response)
            }
            Err(info) => Err(info.to_error_response()),
        }
    }
}
