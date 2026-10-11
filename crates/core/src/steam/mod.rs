//! Steam games beside GOG's. SlattyLauncher reads the local Steam installation and never writes
//! it; installs, uninstalls and launches go to the Steam client itself, through `steam://` URLs.

pub mod appinfo;
pub mod client;
pub mod library;
pub mod local;
pub mod vdf;
