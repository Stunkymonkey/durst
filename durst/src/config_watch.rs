//! Hot reload: reports changes of the config file.
//!
//! The directory is watched rather than the file: editors and home-manager
//! replace the file (write a new one, rename it over the old one), which
//! would end a watch on the file itself.

use std::path::PathBuf;
use std::time::Duration;

use futures::channel::mpsc::unbounded;
use futures::{SinkExt, Stream, StreamExt};
use iced::Subscription;
use notify::{RecursiveMode, Watcher};

/// Editors write in several steps; wait for them to finish.
const DEBOUNCE: Duration = Duration::from_millis(200);

/// Fires after the file at `path` was created, changed, replaced or removed.
pub fn subscription(path: PathBuf) -> Subscription<()> {
    Subscription::run_with(path, stream)
}

// `&PathBuf`, not `&Path`: `Subscription::run_with` passes a reference to its data
#[allow(clippy::ptr_arg)]
fn stream(path: &PathBuf) -> impl Stream<Item = ()> + use<> {
    let path = path.clone();
    iced::stream::channel(4, async move |mut output| {
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            return;
        };
        let name = name.to_owned();
        let (tx, mut rx) = unbounded();
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            // only changes: inotify also reports reads, and every reload reads
            // the file, which would trigger the next reload, forever
            if let Ok(event) = event
                && (event.kind.is_create() || event.kind.is_modify() || event.kind.is_remove())
                && event.paths.iter().any(|p| p.file_name() == Some(&name))
            {
                let _ = tx.unbounded_send(());
            }
        });
        let mut watcher = match watcher {
            Ok(watcher) => watcher,
            Err(e) => {
                log::warn!("cannot watch the config: {e}");
                return;
            }
        };
        if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
            // e.g. the directory doesn't exist yet
            log::info!("not watching {} for changes: {e}", dir.display());
            return;
        }
        log::debug!("watching {}", path.display());
        while rx.next().await.is_some() {
            tokio::time::sleep(DEBOUNCE).await;
            while rx.try_next().is_ok_and(|e| e.is_some()) {}
            if output.send(()).await.is_err() {
                break;
            }
        }
        drop(watcher);
    })
}
