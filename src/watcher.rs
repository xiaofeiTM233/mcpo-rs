use anyhow::Result;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::mpsc as async_mpsc;

pub struct ConfigWatcher {
    _tx: async_mpsc::Sender<String>,
}

impl ConfigWatcher {
    pub fn new(config_path: String, reload_tx: async_mpsc::Sender<String>) -> Result<Self> {
        let path = Path::new(&config_path);
        let watch_dir = path.parent().unwrap_or(Path::new("."));

        let (tx, rx) = mpsc::channel();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                match event.kind {
                    EventKind::Modify(_) | EventKind::Create(_) => {
                        tx.send(()).ok();
                    }
                    _ => {}
                }
            }
        })?;

        watcher.watch(watch_dir, RecursiveMode::NonRecursive)?;

        let tx_for_task = reload_tx.clone();
        tokio::spawn(async move {
            loop {
                if rx.recv().is_ok() {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    if tx_for_task.send("reload".to_string()).await.is_err() {
                        break;
                    }
                }
            }
        });

        std::mem::forget(watcher);

        Ok(ConfigWatcher { _tx: reload_tx })
    }
}
