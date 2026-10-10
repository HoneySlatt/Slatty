//! Helpers shared by the background work of the interface.

use std::time::{Duration, Instant};

use iced::futures::{SinkExt, Stream};
use slatty_core::account::Account;
use slatty_core::installer::Progress;

use crate::{Core, Message, err};

/// Passes progress from the core to the interface at most four times a second.
pub struct Throttle {
    tx: tokio::sync::mpsc::UnboundedSender<Progress>,
    last: std::sync::Mutex<Option<Instant>>,
}

impl Throttle {
    pub fn report(&self, p: Progress) {
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_none_or(|t| t.elapsed() >= Duration::from_millis(250)) {
            *last = Some(Instant::now());
            let _ = self.tx.send(p);
        }
    }
}

/// Runs `work` and streams its progress, then the message it ends with.
pub fn progress_stream<F, Fut>(
    work: F,
    on_progress: impl Fn(Progress) -> Message + Send + 'static,
) -> impl Stream<Item = Message>
where
    F: FnOnce(Throttle) -> Fut + Send + 'static,
    Fut: Future<Output = Message> + Send,
{
    iced::stream::channel(64, async move |mut output| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let run = work(Throttle {
            tx,
            last: std::sync::Mutex::new(None),
        });
        let mut forward_out = output.clone();
        let forward = async move {
            while let Some(p) = rx.recv().await {
                let _ = forward_out.send(on_progress(p)).await;
            }
        };
        let (done, ()) = tokio::join!(run, forward);
        let _ = output.send(done).await;
    })
}

/// Valid GOG tokens of the signed-in account.
pub async fn tokens(core: &Core) -> Result<slatty_core::auth::Tokens, String> {
    let mut account = Account::load(&core.db, &core.dirs).await.map_err(err)?;
    account.tokens(&core.http).await.map_err(err).cloned()
}

/// Refuses queued account work if sign-in changed before the task loaded its credentials.
pub async fn tokens_for(core: &Core, user_id: &str) -> Result<slatty_core::auth::Tokens, String> {
    let mut account = Account::load_for(&core.db, &core.dirs, user_id)
        .await
        .map_err(err)?;
    account.tokens(&core.http).await.map_err(err).cloned()
}

/// `None` when the user paused the work, else the error to show.
pub fn paused_or(e: slatty_core::Error) -> Option<String> {
    match e {
        slatty_core::Error::Cancelled => None,
        e => Some(e.to_string()),
    }
}
