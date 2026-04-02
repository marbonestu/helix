use std::io;
use std::path::PathBuf;

use tokio::sync::mpsc::Sender;

use crate::file_tree::FileTreeUpdate;

/// Spawn a blocking task that creates a new file and notifies the tree.
pub fn spawn_create_file(tx: Sender<FileTreeUpdate>, parent: PathBuf, name: String) {
    tokio::task::spawn_blocking(move || {
        let dest = parent.join(&name);
        let result = std::fs::File::create(&dest).map(|_| ());
        let update = match result {
            Ok(()) => FileTreeUpdate::FsOpComplete {
                refresh_parent: parent,
                select_path: Some(dest),
            },
            Err(e) => FileTreeUpdate::FsOpError {
                message: format!("Create file failed: {}", e),
            },
        };
        let _ = tx.blocking_send(update);
        helix_event::request_redraw();
    });
}

/// Spawn a blocking task that creates a new directory and notifies the tree.
pub fn spawn_create_dir(tx: Sender<FileTreeUpdate>, parent: PathBuf, name: String) {
    tokio::task::spawn_blocking(move || {
        let dest = parent.join(&name);
        let result = std::fs::create_dir_all(&dest);
        let update = match result {
            Ok(()) => FileTreeUpdate::FsOpComplete {
                refresh_parent: parent,
                select_path: Some(dest),
            },
            Err(e) => FileTreeUpdate::FsOpError {
                message: format!("Create directory failed: {}", e),
            },
        };
        let _ = tx.blocking_send(update);
        helix_event::request_redraw();
    });
}

/// Spawn a blocking task that deletes a file or directory and notifies the tree.
pub fn spawn_delete(tx: Sender<FileTreeUpdate>, path: PathBuf, is_dir: bool) {
    tokio::task::spawn_blocking(move || {
        let parent = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let result = if is_dir {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        let update = match result {
            Ok(()) => FileTreeUpdate::FsOpComplete {
                refresh_parent: parent,
                select_path: None,
            },
            Err(e) => FileTreeUpdate::FsOpError {
                message: format!("Delete failed: {}", e),
            },
        };
        let _ = tx.blocking_send(update);
        helix_event::request_redraw();
    });
}

/// Spawn a blocking task that copies a file to a destination directory.
pub fn spawn_copy_file(tx: Sender<FileTreeUpdate>, src: PathBuf, dest_dir: PathBuf) {
    tokio::task::spawn_blocking(move || {
        let file_name = match src.file_name() {
            Some(n) => n.to_os_string(),
            None => {
                let _ = tx.blocking_send(FileTreeUpdate::FsOpError {
                    message: "Copy failed: source has no file name".to_string(),
                });
                helix_event::request_redraw();
                return;
            }
        };
        let dest = dest_dir.join(&file_name);
        let result = std::fs::copy(&src, &dest).map(|_| ());
        let update = match result {
            Ok(()) => FileTreeUpdate::FsOpComplete {
                refresh_parent: dest_dir,
                select_path: Some(dest),
            },
            Err(e) => FileTreeUpdate::FsOpError {
                message: format!("Copy failed: {}", e),
            },
        };
        let _ = tx.blocking_send(update);
        helix_event::request_redraw();
    });
}

/// Spawn a blocking task that moves a file or directory to a destination directory.
///
/// Tries `std::fs::rename` first; if it fails with a cross-device error (os error 18),
/// falls back to copy-then-delete.
pub fn spawn_move_path(tx: Sender<FileTreeUpdate>, src: PathBuf, dest_dir: PathBuf) {
    tokio::task::spawn_blocking(move || {
        let file_name = match src.file_name() {
            Some(n) => n.to_os_string(),
            None => {
                let _ = tx.blocking_send(FileTreeUpdate::FsOpError {
                    message: "Move failed: source has no file name".to_string(),
                });
                helix_event::request_redraw();
                return;
            }
        };
        let dest = dest_dir.join(&file_name);
        let rename_result = std::fs::rename(&src, &dest);

        let result: io::Result<()> = match rename_result {
            Ok(()) => Ok(()),
            Err(e) if is_cross_device(&e) => {
                // Cross-device move: copy then delete
                match std::fs::copy(&src, &dest).map(|_| ()) {
                    Ok(()) => {
                        if src.is_dir() {
                            std::fs::remove_dir_all(&src)
                        } else {
                            std::fs::remove_file(&src)
                        }
                    }
                    Err(e) => Err(e),
                }
            }
            Err(e) => Err(e),
        };

        let update = match result {
            Ok(()) => FileTreeUpdate::FsOpComplete {
                refresh_parent: dest_dir,
                select_path: Some(dest),
            },
            Err(e) => FileTreeUpdate::FsOpError {
                message: format!("Move failed: {}", e),
            },
        };
        let _ = tx.blocking_send(update);
        helix_event::request_redraw();
    });
}

fn is_cross_device(e: &io::Error) -> bool {
    // io::ErrorKind::CrossesDevices is unstable on older toolchains;
    // fall back to raw OS error 18 (EXDEV) as a reliable cross-platform check.
    #[cfg(unix)]
    {
        e.raw_os_error() == Some(18)
    }
    #[cfg(not(unix))]
    {
        // On Windows there is no direct equivalent, but moves across volumes
        // return ERROR_NOT_SAME_DEVICE (17). Treat any rename failure as
        // potentially cross-device to be safe.
        let _ = e;
        false
    }
}
