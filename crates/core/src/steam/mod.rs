//! Steam games beside GOG's. Slatty reads the local Steam installation and never writes
//! it; installs, uninstalls and launches go to the Steam client itself, through `steam://` URLs.

pub mod appinfo;
pub mod client;
pub mod library;
pub mod local;
pub mod running;
pub mod stats;
pub mod vdf;
pub mod web;

/// Opens one of the Steam client's files to read it, when it is a regular file: a pipe in its
/// place would hold the reading thread for ever, a device give no end.
pub(crate) fn open(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::fcntl::OFlag::O_NONBLOCK.bits())
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other("not a regular file"));
    }
    Ok(file)
}
