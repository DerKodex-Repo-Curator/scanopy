use futures::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;

/// End an SSE event stream once server shutdown starts. axum's graceful shutdown waits for every
/// open response to finish, and a live-update stream otherwise never does.
pub fn until_shutdown<S: Stream>(
    stream: S,
    shutdown: &CancellationToken,
) -> impl Stream<Item = S::Item> + use<S> {
    stream.take_until(shutdown.clone().cancelled_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use std::time::Duration;

    #[tokio::test]
    async fn stream_ends_when_shutdown_starts() {
        let shutdown = CancellationToken::new();
        let mut events = Box::pin(until_shutdown(
            stream::iter([1, 2]).chain(stream::pending()),
            &shutdown,
        ));

        assert_eq!(events.next().await, Some(1));
        assert_eq!(events.next().await, Some(2));

        shutdown.cancel();
        let end = tokio::time::timeout(Duration::from_secs(1), events.next()).await;
        assert_eq!(end, Ok(None));
    }
}
