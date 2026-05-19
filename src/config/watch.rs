use crate::config::{config_paths, themes_dir};
use anyhow::Result;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

pub struct ConfigWatcher {
    rx: Receiver<notify::Result<notify::Event>>,
    _watcher: RecommendedWatcher,
    last_reload: Instant,
}

impl ConfigWatcher {
    pub fn try_new() -> Result<Option<Self>> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            move |result| {
                let _ = tx.send(result);
            },
            Config::default(),
        )?;

        let mut watched = false;
        for path in config_paths()? {
            if let Some(parent) = path.parent() {
                if parent.exists() {
                    watcher.watch(parent, RecursiveMode::NonRecursive)?;
                    watched = true;
                }
            }
        }
        if let Some(dir) = themes_dir() {
            if dir.exists() {
                watcher.watch(&dir, RecursiveMode::NonRecursive)?;
                watched = true;
            }
        }

        if !watched {
            return Ok(None);
        }

        Ok(Some(Self {
            rx,
            _watcher: watcher,
            last_reload: Instant::now(),
        }))
    }

    pub fn should_reload(&mut self) -> bool {
        let mut pending = false;
        while let Ok(Ok(event)) = self.rx.try_recv() {
            if matches!(
                event.kind,
                notify::EventKind::Modify(_)
                    | notify::EventKind::Create(_)
                    | notify::EventKind::Remove(_)
            ) {
                pending = true;
            }
        }
        if !pending {
            return false;
        }
        if self.last_reload.elapsed() < Duration::from_millis(250) {
            return false;
        }
        self.last_reload = Instant::now();
        true
    }
}
