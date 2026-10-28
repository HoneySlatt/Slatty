//! Steam games beside GOG's. Slatty reads the local Steam installation and never writes
//! it; installs, uninstalls and launches go to the Steam client itself, through `steam://` URLs.

pub mod appinfo;
pub mod client;
pub mod library;
pub mod local;
#[cfg(unix)]
pub mod running;
/// The Steam games running are not found yet on Windows (docs/windows-port.md, step 4).
#[cfg(windows)]
pub mod running {
    pub fn running(_: &std::path::Path) -> std::collections::BTreeSet<u32> {
        Default::default()
    }
}
pub mod stats;
pub mod vdf;
pub mod web;

/// Opens one of the Steam client's files to read it, when it is a regular file: a pipe in its
/// place would hold the reading thread for ever, a device give no end.
pub(crate) fn open(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let file =
        crate::platform::without_waiting(std::fs::OpenOptions::new().read(true)).open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other("not a regular file"));
    }
    Ok(file)
}
