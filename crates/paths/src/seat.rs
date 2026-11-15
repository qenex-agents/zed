//! QENEX seat binding.
//!
//! A Zed process that runs inside a QENEX seat keeps all of its writable
//! editor state inside that seat's workspace instead of the user's global
//! config, data, state and log directories. Reads from system locations are
//! unaffected.
//!
//! Seat binding is controlled by three environment variables:
//!
//! * `QENEX_SEAT` - the seat identifier. When it is unset or empty, the
//!   process is not seat-bound and keeps the legacy, global locations.
//! * `QENEX_SEAT_WORKSPACE` - the absolute path of the seat workspace, the
//!   only writable root while seat-bound.
//! * `QENEX_SEAT_ALLOW_GLOBAL` - when set to `1`, writes outside the seat
//!   workspace are allowed. This is the escape hatch for maintenance use.

use std::env;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use crate::APP_NAME_LOWERCASE;

/// The environment variable holding the seat identifier.
pub const SEAT_ENV_VAR: &str = "QENEX_SEAT";

/// The environment variable holding the absolute path of the seat workspace.
pub const SEAT_WORKSPACE_ENV_VAR: &str = "QENEX_SEAT_WORKSPACE";

/// The environment variable that disables seat confinement when set to `1`.
pub const SEAT_ALLOW_GLOBAL_ENV_VAR: &str = "QENEX_SEAT_ALLOW_GLOBAL";

/// The writable roots that can be scoped to a seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    /// The configuration directory.
    Config,
    /// The data directory.
    Data,
    /// The state directory.
    State,
    /// The logs directory.
    Logs,
    /// The temporary directory.
    Temp,
}

/// The seat a process is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seat {
    name: String,
    workspace: PathBuf,
}

impl Seat {
    /// Creates a seat with the given name and workspace path.
    pub fn new(name: impl Into<String>, workspace: impl Into<PathBuf>) -> Self {
        Seat {
            name: name.into(),
            workspace: workspace.into(),
        }
    }

    /// The seat identifier, e.g. `coder`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The seat workspace, which is the only writable root while seat-bound.
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// The configuration directory inside the seat workspace.
    pub fn config_dir(&self) -> PathBuf {
        self.workspace.join(".config").join(APP_NAME_LOWERCASE)
    }

    /// The data directory inside the seat workspace.
    pub fn data_dir(&self) -> PathBuf {
        self.workspace.join(".data").join(APP_NAME_LOWERCASE)
    }

    /// The state directory inside the seat workspace.
    pub fn state_dir(&self) -> PathBuf {
        self.workspace.join(".state").join(APP_NAME_LOWERCASE)
    }

    /// The logs directory inside the seat workspace.
    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir().join("logs")
    }

    /// The temporary directory inside the seat workspace.
    pub fn temp_dir(&self) -> PathBuf {
        self.workspace.join(".tmp").join(APP_NAME_LOWERCASE)
    }

    /// Whether `path` is inside the seat workspace.
    pub fn contains(&self, path: &Path) -> bool {
        if normalize(path).starts_with(normalize(self.workspace())) {
            return true;
        }

        // Fall back to resolving symlinks when both paths exist.
        match (self.workspace().canonicalize(), path.canonicalize()) {
            (Ok(workspace), Ok(path)) => path.starts_with(workspace),
            _ => false,
        }
    }

    /// The seat identity line, e.g. `seat=coder workspace=/var/lib/qenex`.
    pub fn info_line(&self) -> String {
        format!("seat={} workspace={}", self.name, self.workspace.display())
    }
}

/// The seat-relevant part of the process environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeatEnv {
    /// The value of `QENEX_SEAT`, if it is set and not empty.
    pub seat: Option<String>,
    /// The value of `QENEX_SEAT_WORKSPACE`, if it is set and not empty.
    pub workspace: Option<String>,
    /// Whether `QENEX_SEAT_ALLOW_GLOBAL` is set to `1`.
    pub allow_global: bool,
}

impl SeatEnv {
    /// Reads the seat environment from the process environment.
    pub fn from_process() -> Self {
        SeatEnv {
            seat: non_empty_env(SEAT_ENV_VAR),
            workspace: non_empty_env(SEAT_WORKSPACE_ENV_VAR),
            allow_global: allows_global_writes(),
        }
    }
}

/// How a process is bound to a QENEX seat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeatBinding {
    /// No seat is configured, so the legacy, global locations are used.
    Unbound,
    /// The process is bound to a seat.
    Bound(Seat),
    /// A seat was requested, but the seat workspace is missing or unusable.
    Misconfigured(String),
}

/// Determines how a process is bound to a seat, given `env`.
pub fn binding(env: &SeatEnv) -> SeatBinding {
    let Some(name) = env.seat.as_deref() else {
        return SeatBinding::Unbound;
    };

    let workspace = env.workspace.as_deref().unwrap_or_default();
    if workspace.is_empty() {
        return SeatBinding::Misconfigured(format!(
            "{SEAT_ENV_VAR} is set to {name:?}, but {SEAT_WORKSPACE_ENV_VAR} is not set; \
             set {SEAT_WORKSPACE_ENV_VAR} to the seat workspace, or unset {SEAT_ENV_VAR} \
             (set {SEAT_ALLOW_GLOBAL_ENV_VAR}=1 to keep the global locations)"
        ));
    }

    let workspace = PathBuf::from(workspace);
    if !workspace.is_absolute() {
        return SeatBinding::Misconfigured(format!(
            "{SEAT_WORKSPACE_ENV_VAR} must be an absolute path, but it is {workspace:?}"
        ));
    }

    SeatBinding::Bound(Seat::new(name, workspace))
}

/// Returns the seat this process is bound to, if any.
///
/// # Panics
///
/// Panics if the seat environment is misconfigured, because the writable roots
/// cannot be confined to a seat workspace in that case.
pub fn current_seat() -> Option<Seat> {
    match binding(&SeatEnv::from_process()) {
        SeatBinding::Unbound => None,
        SeatBinding::Bound(seat) => Some(seat),
        SeatBinding::Misconfigured(message) => panic!("{message}"),
    }
}

/// Returns the seat-scoped location of `kind`, or `None` when the process is
/// not seat-bound and the legacy location has to be used.
pub fn scoped_root(seat: Option<&Seat>, kind: RootKind) -> Option<PathBuf> {
    let seat = seat?;
    Some(match kind {
        RootKind::Config => seat.config_dir(),
        RootKind::Data => seat.data_dir(),
        RootKind::State => seat.state_dir(),
        RootKind::Logs => seat.logs_dir(),
        RootKind::Temp => seat.temp_dir(),
    })
}

/// The seat identity line for info output, or `None` when not seat-bound.
pub fn info_line() -> Option<String> {
    current_seat().map(|seat| seat.info_line())
}

/// The error returned when a write would escape the seat workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatWriteError {
    /// The path that was refused.
    pub path: PathBuf,
    /// The seat workspace that confines writes.
    pub workspace: PathBuf,
}

impl fmt::Display for SeatWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "refusing to write outside the QENEX seat workspace: {} (seat workspace: {}); \
             set {SEAT_ALLOW_GLOBAL_ENV_VAR}=1 to allow writes outside the seat workspace",
            self.path.display(),
            self.workspace.display()
        )
    }
}

impl std::error::Error for SeatWriteError {}

/// Checks a write against a seat binding.
///
/// * Unbound processes may write anywhere, which keeps the legacy behaviour.
/// * Seat-bound processes may only write inside the seat workspace, unless
///   `allow_global` is set.
pub fn check_write(
    seat: Option<&Seat>,
    allow_global: bool,
    path: &Path,
) -> Result<(), SeatWriteError> {
    let Some(seat) = seat else {
        return Ok(());
    };

    if allow_global || seat.contains(path) {
        return Ok(());
    }

    Err(SeatWriteError {
        path: path.to_path_buf(),
        workspace: seat.workspace().to_path_buf(),
    })
}

/// Checks that `path` may be written to by this process.
pub fn ensure_writable(path: &Path) -> Result<(), SeatWriteError> {
    let seat = current_seat();
    check_write(seat.as_ref(), allows_global_writes(), path)
}

/// Whether writes outside the seat workspace are allowed.
pub fn allows_global_writes() -> bool {
    env::var(SEAT_ALLOW_GLOBAL_ENV_VAR)
        .map(|value| value.trim() == "1")
        .unwrap_or(false)
}

/// Reads an environment variable, ignoring unset and empty values.
fn non_empty_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Lexically normalizes a path by resolving `.` and `..` components.
fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\qenex\seats\coder\workspace")
        } else {
            PathBuf::from("/var/lib/qenex/seats/coder/workspace")
        }
    }

    fn seat_env(seat: Option<&str>, workspace: Option<&str>) -> SeatEnv {
        SeatEnv {
            seat: seat.map(str::to_string),
            workspace: workspace.map(str::to_string),
            allow_global: false,
        }
    }

    #[test]
    fn unbound_processes_are_not_seat_bound() {
        assert_eq!(binding(&seat_env(None, None)), SeatBinding::Unbound);
        assert_eq!(binding(&seat_env(Some(""), None)), SeatBinding::Unbound);
    }

    #[test]
    fn unbound_processes_keep_the_legacy_locations() {
        let seat = match binding(&seat_env(None, None)) {
            SeatBinding::Unbound => None,
            other => panic!("expected an unbound seat, got {other:?}"),
        };

        assert_eq!(scoped_root(seat.as_ref(), RootKind::Config), None);
        assert_eq!(scoped_root(seat.as_ref(), RootKind::Data), None);
        assert_eq!(scoped_root(seat.as_ref(), RootKind::State), None);
        assert_eq!(scoped_root(seat.as_ref(), RootKind::Logs), None);
        assert_eq!(scoped_root(seat.as_ref(), RootKind::Temp), None);
    }

    #[test]
    fn a_seat_without_a_usable_workspace_is_misconfigured() {
        assert!(matches!(
            binding(&seat_env(Some("coder"), None)),
            SeatBinding::Misconfigured(_)
        ));
        assert!(matches!(
            binding(&seat_env(Some("coder"), Some("relative/workspace"))),
            SeatBinding::Misconfigured(_)
        ));
    }

    #[test]
    fn seat_roots_resolve_inside_the_workspace() {
        let seat = match binding(&seat_env(Some("coder"), Some(workspace().to_str().unwrap()))) {
            SeatBinding::Bound(seat) => seat,
            other => panic!("expected a seat binding, got {other:?}"),
        };

        assert_eq!(seat.name(), "coder");
        assert_eq!(seat.workspace(), workspace());
        assert_eq!(
            seat.config_dir(),
            workspace().join(".config").join(APP_NAME_LOWERCASE)
        );
        assert_eq!(
            seat.data_dir(),
            workspace().join(".data").join(APP_NAME_LOWERCASE)
        );
        assert_eq!(
            seat.state_dir(),
            workspace().join(".state").join(APP_NAME_LOWERCASE)
        );
        assert_eq!(seat.logs_dir(), seat.data_dir().join("logs"));
        assert_eq!(
            seat.temp_dir(),
            workspace().join(".tmp").join(APP_NAME_LOWERCASE)
        );

        for root in [
            seat.config_dir(),
            seat.data_dir(),
            seat.state_dir(),
            seat.logs_dir(),
            seat.temp_dir(),
        ] {
            assert!(
                root.starts_with(&workspace()),
                "{root:?} escaped the seat workspace"
            );
        }

        assert_eq!(
            scoped_root(Some(&seat), RootKind::Config),
            Some(seat.config_dir())
        );
    }

    #[test]
    fn unbound_processes_may_write_anywhere() {
        assert!(check_write(None, false, Path::new("/etc/zed/settings.json")).is_ok());
    }

    #[test]
    fn writes_inside_the_workspace_are_allowed() {
        let seat = Seat::new("coder", workspace());
        let inside = seat.config_dir().join("settings.json");

        assert!(check_write(Some(&seat), false, &inside).is_ok());
    }

    #[test]
    fn writes_outside_the_workspace_are_refused() {
        let seat = Seat::new("coder", workspace());
        let outside = workspace().join("..").join("other-seat").join("settings.json");

        let error = check_write(Some(&seat), false, &outside).unwrap_err();
        assert!(error.to_string().contains(&outside.display().to_string()));
        assert!(error.to_string().contains("QENEX_SEAT_ALLOW_GLOBAL=1"));

        // Traversal out of the workspace is refused as well.
        let traversal = workspace().join("..").join("escape");
        assert!(check_write(Some(&seat), false, &traversal).is_err());
    }

    #[test]
    fn allow_global_disables_confinement() {
        let seat = Seat::new("coder", workspace());
        let outside = workspace().join("..").join("other-seat").join("settings.json");

        assert!(check_write(Some(&seat), true, &outside).is_ok());
    }
}
