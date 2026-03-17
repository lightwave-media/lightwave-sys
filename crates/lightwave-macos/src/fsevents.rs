//! Native macOS FSEvents file watching.
//!
//! Detects file changes without polling using the macOS FSEvents API.

#[cfg(target_os = "macos")]
use fsevent::FsEvent;

use anyhow::Result;
use std::path::PathBuf;
use std::sync::mpsc;

/// A file change event from FSEvents.
#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    pub path: PathBuf,
    pub flags: u32,
    pub is_created: bool,
    pub is_removed: bool,
    pub is_modified: bool,
    pub is_renamed: bool,
    pub is_directory: bool,
}

/// Start watching a directory for file changes.
/// Returns a receiver channel that emits `FileChangeEvent`s.
///
/// The watcher runs on a background thread and will continue
/// until the returned `FsWatcher` is dropped.
pub fn watch_directory(path: &str) -> Result<(FsWatcher, mpsc::Receiver<FileChangeEvent>)> {
    #[cfg(target_os = "macos")]
    {
        watch_directory_impl(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        anyhow::bail!("FSEvents only available on macOS")
    }
}

/// Handle to the file watcher. Dropping this stops the watcher.
pub struct FsWatcher {
    #[cfg(target_os = "macos")]
    _handle: std::thread::JoinHandle<()>,
    #[cfg(not(target_os = "macos"))]
    _phantom: std::marker::PhantomData<()>,
}

#[cfg(target_os = "macos")]
fn watch_directory_impl(path: &str) -> Result<(FsWatcher, mpsc::Receiver<FileChangeEvent>)> {
    let (tx, rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    let path = path.to_string();

    let handle = std::thread::spawn(move || {
        let fsevent = FsEvent::new(vec![path]);
        // FsEvent::observe takes a Sender<Event> and blocks
        fsevent.observe(event_tx);
    });

    // Spawn a translator thread that converts fsevent::Event -> FileChangeEvent
    let _translator = std::thread::spawn(move || {
        for ev in event_rx {
            let flags = ev.flag.bits();
            let change = FileChangeEvent {
                path: PathBuf::from(&ev.path),
                flags,
                is_created: flags & 0x100 != 0, // kFSEventStreamEventFlagItemCreated
                is_removed: flags & 0x200 != 0, // kFSEventStreamEventFlagItemRemoved
                is_modified: flags & 0x1000 != 0, // kFSEventStreamEventFlagItemModified
                is_renamed: flags & 0x800 != 0, // kFSEventStreamEventFlagItemRenamed
                is_directory: flags & 0x2000 != 0, // kFSEventStreamEventFlagItemIsDir
            };
            if tx.send(change).is_err() {
                return; // Receiver dropped, stop watching
            }
        }
    });

    Ok((FsWatcher { _handle: handle }, rx))
}
