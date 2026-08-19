use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tracing_subscriber::fmt::writer::MakeWriterExt;

const LOG_DIRECTORY: &str = "logs";
const LOG_FILE: &str = "agent.log";
const PREVIOUS_LOG_FILE: &str = "agent.previous.log";
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Clone)]
struct SharedFileWriter {
    file: Arc<Mutex<File>>,
}

struct LockedFileWriter {
    file: Arc<Mutex<File>>,
}

impl<'writer> tracing_subscriber::fmt::MakeWriter<'writer> for SharedFileWriter {
    type Writer = LockedFileWriter;

    fn make_writer(&'writer self) -> Self::Writer {
        LockedFileWriter {
            file: Arc::clone(&self.file),
        }
    }
}

impl Write for LockedFileWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.file
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .flush()
    }
}

pub(crate) fn init(app_data_dir: &Path) -> Option<PathBuf> {
    match open_log_file(app_data_dir) {
        Ok((path, file)) => {
            let file_writer = SharedFileWriter {
                file: Arc::new(Mutex::new(file)),
            };
            let writer = std::io::stderr.and(file_writer);
            let initialized = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .with_target(false)
                .with_ansi(false)
                .with_writer(writer)
                .try_init()
                .is_ok();
            initialized.then_some(path)
        }
        Err(error) => {
            let _ = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .with_target(false)
                .try_init();
            eprintln!("agent log file could not be opened: {error}");
            None
        }
    }
}

fn open_log_file(app_data_dir: &Path) -> io::Result<(PathBuf, File)> {
    let directory = app_data_dir.join(LOG_DIRECTORY);
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(LOG_FILE);
    rotate_if_needed(&path, &directory.join(PREVIOUS_LOG_FILE))?;
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    Ok((path, file))
}

fn rotate_if_needed(path: &Path, previous: &Path) -> io::Result<()> {
    let Some(metadata) = std::fs::metadata(path).ok() else {
        return Ok(());
    };
    if metadata.len() < MAX_LOG_BYTES {
        return Ok(());
    }
    match std::fs::remove_file(previous) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    std::fs::rename(path, previous)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_log_is_reused_without_rotation() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(LOG_DIRECTORY);
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(LOG_FILE);
        let previous = directory.join(PREVIOUS_LOG_FILE);
        std::fs::write(&path, b"existing log").unwrap();

        rotate_if_needed(&path, &previous).unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"existing log");
        assert!(!previous.exists());
    }
}
