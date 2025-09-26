use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Utc};

use crate::Error;

/// Finalize a download by renaming the part file to the destination file.
pub async fn finalize_download(
    part_file: &Path,
    sidecar_file: &Path,
    destination: &Path,
    modified: Option<&DateTime<Utc>>,
) -> Result<(), Error> {
    let part_file = part_file.to_owned();
    let sidecar_file = sidecar_file.to_owned();
    let destination = destination.to_owned();
    let modified = modified.map(|dt| dt.to_owned());

    tokio::task::spawn_blocking(move || {
        finalize_download_sync(&sidecar_file, &part_file, &destination, modified.as_ref())
    })
    .await
    .unwrap()?;

    Ok(())
}

fn finalize_download_sync(
    sidecar_file: &Path,
    part_file: &Path,
    destination: &Path,
    modified: Option<&DateTime<Utc>>,
) -> Result<(), Error> {
    // Rename the .part file to the final file.
    fs::rename(part_file, destination).map_err(|e| Error::Write {
        action: "renaming part file",
        path: part_file.to_path_buf(),
        cause: e,
    })?;

    // Delete the sidecar file.
    let _ = fs::remove_file(sidecar_file);

    // Update the modified time if we have one.
    if let Some(modified) = modified {
        let modified = modified.timestamp_millis();
        let modified = if modified >= 0 {
            SystemTime::UNIX_EPOCH + Duration::from_millis(modified as u64)
        } else {
            SystemTime::UNIX_EPOCH - Duration::from_millis(-modified as u64)
        };
        let file = fs::File::open(destination).map_err(|e| Error::Write {
            action: "opening file to update modified time",
            path: destination.to_path_buf(),
            cause: e,
        })?;
        file.set_modified(modified).map_err(|e| Error::Write {
            action: "updating modified time",
            path: destination.to_path_buf(),
            cause: e,
        })?;
    }

    Ok(())
}
