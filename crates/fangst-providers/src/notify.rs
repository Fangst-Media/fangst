//! Short texts for notifications, shared so every notifier and plugin says the same thing.

use crate::NotificationEvent;

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

impl NotificationEvent {
    /// The serialized `event` tag, e.g. `job_completed`.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::JobCompleted { .. } => "job_completed",
            Self::JobFailed { .. } => "job_failed",
            Self::DiskFull { .. } => "disk_full",
        }
    }

    #[must_use]
    pub fn title(&self) -> &'static str {
        match self {
            Self::JobCompleted { .. } => "Download completed",
            Self::JobFailed { .. } => "Download failed",
            Self::DiskFull { .. } => "Disk almost full",
        }
    }

    /// One line for the notification body.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::JobCompleted {
                name,
                category: Some(category),
                ..
            } => format!("{name} ({category})"),
            Self::JobCompleted { name, .. } => name.clone(),
            Self::JobFailed { name, reason, .. } => format!("{name}: {reason}"),
            Self::DiskFull { path, free_bytes } => {
                format!("{} has {} free", path.display(), readable(*free_bytes))
            }
        }
    }
}

fn readable(bytes: u64) -> String {
    if bytes >= GIB {
        // Integer tenths avoid a lossy float cast for a value only shown to people.
        let tenths = bytes % GIB * 10 / GIB;
        format!("{}.{tenths} GiB", bytes / GIB)
    } else {
        format!("{} MiB", bytes / MIB)
    }
}
