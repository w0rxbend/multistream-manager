//! Logging setup.
//!
//! The terminal UI owns the screen, so nothing can be printed to stdout while it
//! is running — a stray `println!` would punch a hole in the interface. All
//! diagnostics therefore go to a file, which you can watch from another terminal
//! with `tail -f`.

use anyhow::Result;
use tracing_subscriber::EnvFilter;

/// How large `msm.log` may be at start-up before it is rotated away.
///
/// Chosen to be generous for a debugging session at `MSM_LOG=debug` while
/// still bounding what accumulates in a config directory that nobody ever
/// looks at.
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

/// How many rotated logs to keep beside the live one.
const KEEP_LOGS: usize = 3;

/// Start logging to `msm.log` in the config directory.
///
/// The verbosity is controlled by the `MSM_LOG` environment variable using the
/// usual `tracing` syntax, for example `MSM_LOG=debug` or
/// `MSM_LOG=multistream_manager::youtube=trace`.
///
/// Returns a guard that must be kept alive for the lifetime of the program;
/// dropping it flushes anything still buffered.
pub fn init() -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let path = crate::paths::log_file()?;

    rotate_if_large(&path, MAX_LOG_BYTES, KEEP_LOGS);

    let file = open_log_file(&path)?;

    let (writer, guard) = tracing_appender::non_blocking(file);

    let filter = EnvFilter::try_from_env("MSM_LOG").unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        // ANSI colour codes would be written into the file as escape sequences.
        .with_ansi(false)
        .with_target(true)
        .init();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting");
    Ok(guard)
}

/// Move an oversized log aside, keeping a few older ones, before it is
/// reopened for appending.
///
/// The log had no bound at all: it was opened in append mode and grew for the
/// life of the config directory, which for anybody running at `MSM_LOG=debug`
/// meant a file that only ever got bigger. The chat log next door has rotated
/// by size since it was written; this is the same idea with the simplest
/// policy that fits an application log.
///
/// Rotation happens at start-up rather than mid-write, which means one
/// session's log is never split in half — the thing you are `tail -f`ing does
/// not move under you — at the cost of a single very long session still being
/// able to exceed the cap. That trade is deliberate: a session's own log is
/// worth more intact than trimmed.
///
/// Every failure here is ignored. A log that cannot be rotated is not a
/// reason to refuse to start; the worst case is that it keeps growing, which
/// is exactly what it did before.
fn rotate_if_large(path: &std::path::Path, max_bytes: u64, keep: usize) {
    let too_big = std::fs::metadata(path).is_ok_and(|meta| meta.len() > max_bytes);
    if !too_big {
        return;
    }

    // Shift the numbered logs up from the oldest, so nothing is overwritten
    // before it has been moved: msm.log.2 → msm.log.3, msm.log.1 → msm.log.2.
    let numbered = |n: usize| path.with_extension(format!("log.{n}"));
    if keep > 0 {
        let _ = std::fs::remove_file(numbered(keep));
        for n in (1..keep).rev() {
            let _ = std::fs::rename(numbered(n), numbered(n + 1));
        }
        let _ = std::fs::rename(path, numbered(1));
    } else {
        let _ = std::fs::remove_file(path);
    }
}

/// Open the log file for appending, with owner-only permissions.
///
/// Split out of [`init`] so the permission handling can be tested without
/// installing a global tracing subscriber.
fn open_log_file(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);

    // The log is owner-only, like `tokens.json`, because it records what the
    // program was doing with which accounts. The token parser deliberately
    // withholds response bodies from its errors for this reason (see
    // `auth::oauth::token_set_from_body`), but permissions are the backstop
    // for everything nobody thought to withhold: a world-readable log hands
    // it all to every other account on the machine.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let file = options.open(path)?;

    // `mode` above only applies when the file is created. A log left behind by
    // an older version is already world-readable, so tighten it as well.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(file)
}

#[cfg(test)]
mod rotation_tests {
    #[test]
    fn a_small_log_is_left_exactly_where_it_is() {
        let dir = std::env::temp_dir().join(format!("msm-rot-small-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("msm.log");
        std::fs::write(&path, b"a few bytes").unwrap();

        super::rotate_if_large(&path, 1024, 3);

        assert_eq!(std::fs::read(&path).unwrap(), b"a few bytes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_oversized_log_is_moved_aside_and_older_ones_shift_up() {
        let dir = std::env::temp_dir().join(format!("msm-rot-big-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("msm.log");
        std::fs::write(&path, vec![b'x'; 200]).unwrap();
        std::fs::write(dir.join("msm.log.1"), b"previous").unwrap();

        super::rotate_if_large(&path, 100, 3);

        assert!(!path.exists(), "the oversized log is moved out of the way");
        assert_eq!(
            std::fs::read(dir.join("msm.log.1")).unwrap().len(),
            200,
            "the log that was too big becomes .1"
        );
        assert_eq!(
            std::fs::read(dir.join("msm.log.2")).unwrap(),
            b"previous",
            "the previous .1 shifts up rather than being overwritten"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_is_kept_beyond_the_retention_count() {
        let dir = std::env::temp_dir().join(format!("msm-rot-keep-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("msm.log");
        std::fs::write(&path, vec![b'x'; 200]).unwrap();
        for n in 1..=2 {
            std::fs::write(dir.join(format!("msm.log.{n}")), b"old").unwrap();
        }

        super::rotate_if_large(&path, 100, 2);

        assert!(dir.join("msm.log.1").exists());
        assert!(dir.join("msm.log.2").exists());
        assert!(
            !dir.join("msm.log.3").exists(),
            "keeping 2 must not leave a third behind"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    fn mode_of(path: &std::path::Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    /// `init` itself installs a global subscriber and so cannot be called twice
    /// in one test binary. What is tested here is the file-opening behaviour it
    /// performs, on a scratch path.
    #[test]
    fn the_log_file_is_owner_only_whether_it_is_new_or_left_over() {
        let dir = std::env::temp_dir().join(format!("msm-log-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("msm.log");
        let _ = std::fs::remove_file(&path);

        super::open_log_file(&path).unwrap();
        assert_eq!(
            mode_of(&path),
            0o600,
            "a newly created log must be owner-only"
        );

        // A log written by an older version is world-readable; reopening must
        // tighten it rather than leaving the old permissions in place.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        super::open_log_file(&path).unwrap();
        assert_eq!(mode_of(&path), 0o600, "an existing log must be tightened");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
