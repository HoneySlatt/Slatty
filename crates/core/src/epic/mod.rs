//! The Epic Games Store: signing in and the account's library. Epic publishes no API for this;
//! what is known of its services comes from legendary (derrod/legendary, GPL-3.0:
//! `legendary/api/egs.py`, `legendary/core.py`), see docs/epic-integration.md.

pub mod auth;
pub mod library;

/// The user agent of the Epic Games Launcher legendary sends, so that Epic's services answer
/// Slatty as they answer the launcher whose OAuth client it signs in with.
const USER_AGENT: &str =
    "UELauncher/11.0.1-14907503+++Portal+Release-Live Windows/10.0.19041.1.256.64bit";
