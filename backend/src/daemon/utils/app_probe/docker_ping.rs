//! Container engine API detection with `GET /_ping`, which needs no credential.
//!
//! A Docker Engine API on plain TCP, whether the daemon itself or a socket proxy in front of it,
//! answers `/_ping` before any auth. So does Podman's API service, whose compat layer copies the
//! Docker reply's headers (`Api-Version`, `Docker-Experimental`, `Ostype`). What tells them apart
//! is the `Server` product token: `Docker/29.1.2 (linux)` against `Libpod/6.0.0 (linux)`. That is
//! the only header either probe reads.
//!
//! The body (`OK`) identifies nothing: the endpoint scan matches bodies case-insensitively by
//! substring, so any page containing "ok" would pass, which is why this is a probe rather than a
//! `Pattern::Endpoint`.
//!
//! Ports: Docker's is 2375. Podman has none of its own: `podman system service tcp:` takes
//! whatever it is given, and the man page's example is 8888, which is where this looks. One probe
//! owns a port, so a Podman service someone put on 2375 is not claimed by either. 2376 is mutual
//! TLS and a local socket has no network signal, so those engines are still found only by the
//! credentialed probe.

use anyhow::Error;
use async_trait::async_trait;

use crate::daemon::utils::app_probe::{
    AppProbe, AppProbeOutcome, ProbeContext, presence, request_response,
};
use crate::server::ports::r#impl::base::PortType;
use crate::server::services::r#impl::patterns::ClientProbe;

/// No `Connection: close` and no half-close after writing: Docker Desktop's socket front end
/// answers a half-closed request with a 500.
const PING: &[u8] =
    b"GET /_ping HTTP/1.1\r\nHost: docker\r\nUser-Agent: scanopy\r\nAccept: */*\r\n\r\n";

/// Which engine answered a `/_ping`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Engine {
    Docker,
    Podman,
}

impl Engine {
    /// The `Server` product token each engine stamps on its replies.
    fn server_token(self) -> &'static str {
        match self {
            Self::Docker => "docker/",
            Self::Podman => "libpod/",
        }
    }
}

pub struct DockerPingProbe;

#[async_trait]
impl AppProbe for DockerPingProbe {
    fn port(&self) -> PortType {
        PortType::Docker
    }

    fn client_probe(&self) -> Option<ClientProbe> {
        Some(ClientProbe::DockerPing)
    }

    async fn run(&self, ctx: &ProbeContext) -> Result<AppProbeOutcome, Error> {
        Ok(presence(
            ping(ctx, self.port()).await == Some(Engine::Docker),
        ))
    }
}

/// Podman's API service on 8888.
pub struct PodmanPingProbe;

#[async_trait]
impl AppProbe for PodmanPingProbe {
    fn port(&self) -> PortType {
        PortType::Http8888
    }

    fn client_probe(&self) -> Option<ClientProbe> {
        Some(ClientProbe::PodmanPing)
    }

    async fn run(&self, ctx: &ProbeContext) -> Result<AppProbeOutcome, Error> {
        Ok(presence(
            ping(ctx, self.port()).await == Some(Engine::Podman),
        ))
    }
}

async fn ping(ctx: &ProbeContext, port: PortType) -> Option<Engine> {
    engine_of(&request_response(ctx, port, PING, 2048).await)
}

/// The engine a successful `/_ping` reply came from, by its `Server` header.
fn engine_of(bytes: &[u8]) -> Option<Engine> {
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut response = httparse::Response::new(&mut headers);
    if response.parse(bytes).is_err() || response.code != Some(200) {
        return None;
    }
    let server = response
        .headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("server"))?;
    let server = String::from_utf8_lossy(server.value).to_ascii_lowercase();
    [Engine::Docker, Engine::Podman]
        .into_iter()
        .find(|e| server.starts_with(e.server_token()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Docker 29.1.2's reply, as captured.
    const DOCKER: &[u8] = b"HTTP/1.1 200 OK\r\nApi-Version: 1.52\r\nBuilder-Version: 2\r\n\
Cache-Control: no-cache, no-store, must-revalidate\r\nContent-Type: text/plain; charset=utf-8\r\n\
Docker-Experimental: false\r\nOstype: linux\r\nPragma: no-cache\r\nServer: Docker/29.1.2 (linux)\r\n\
Swarm: inactive\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nOK\r\n0\r\n\r\n";

    /// Podman 6.0.0's reply, as captured. Every Docker header but `Server` and `Swarm` is copied.
    const PODMAN: &[u8] = b"HTTP/1.1 200 OK\r\nApi-Version: 1.44\r\nBuilder-Version: 1\r\n\
Buildkit-Version: \r\nCache-Control: no-cache\r\nDocker-Experimental: true\r\n\
Libpod-Api-Version: 6.0.0\r\nLibpod-Buildah-Version: 1.44.0\r\nOstype: linux\r\nPragma: no-cache\r\n\
Server: Libpod/6.0.0 (linux)\r\nX-Reference-Id: 0x52e93302010\r\nContent-Length: 2\r\n\
Content-Type: text/plain; charset=utf-8\r\n\r\nOK";

    #[test]
    fn each_engine_is_told_apart_by_its_server_token() {
        assert_eq!(engine_of(DOCKER), Some(Engine::Docker));
        assert_eq!(engine_of(PODMAN), Some(Engine::Podman));
    }

    #[test]
    fn a_web_page_saying_ok_is_neither() {
        assert_eq!(
            engine_of(
                b"HTTP/1.1 200 OK\r\nServer: nginx\r\nContent-Type: text/html\r\n\r\nToken OK"
            ),
            None
        );
    }

    #[test]
    fn a_refusal_is_not_a_ping() {
        assert_eq!(
            engine_of(
                b"HTTP/1.1 500 Internal Server Error\r\nServer: Docker/29.1.2 (linux)\r\n\r\n"
            ),
            None
        );
    }
}
