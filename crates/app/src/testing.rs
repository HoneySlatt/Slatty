//! A state with fake data, for the interfaces' tests (`test-support`): a core of its own in a
//! throwaway folder and an in-memory database, and `[FAKE]` games.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use slatty_core::account::AccountInfo;
use slatty_core::db::Db;
use slatty_core::install::{Install, Platform};
use slatty_core::library::{LibraryGame, MetadataSource};
use slatty_core::paths::Dirs;
use slatty_core::runner::Runner;

use crate::{App, Core};

/// A core in `root`, with locks of its own: queueing an install or starting an update takes the
/// game's lock for a moment, which tests running at the same time on the same games would find
/// held. Paths and today's date show as the tests expect (`format::fix_for_tests`).
pub fn core(root: &Path) -> Core {
    crate::format::fix_for_tests(root.to_path_buf());
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut dirs = Dirs::under(root);
    dirs.state = root.join(format!("state-{n}"));
    Core {
        dirs,
        db: Arc::new(Db::in_memory().unwrap()),
        http: slatty_core::http::client().unwrap(),
        downloads: Arc::new(tokio::sync::Semaphore::new(1)),
    }
}

/// Fictitious data for interface tests only; titles say so.
pub fn fake_game(id: &str, title: &str) -> LibraryGame {
    LibraryGame {
        id: id.into(),
        title: format!("[FAKE] {title}"),
        cover: None,
        icon: None,
        background: None,
        logo: None,
        os: vec!["windows".into()],
        metadata: MetadataSource::Missing,
        steam: None,
    }
}

/// A state with nothing in it but `core`.
pub fn empty(core: Core) -> App {
    App {
        core: Some(core),
        ..Default::default()
    }
}

/// A state signed in, with fourteen games, the second with a Linux build, the third installed.
pub fn library(core: Core) -> App {
    let mut app = empty(core);
    app.account = Some(AccountInfo {
        user_id: "0".into(),
        username: "testeur".into(),
    });
    app.library = (1..=14)
        .map(|i| fake_game(&i.to_string(), &format!("Game {i}")))
        .collect();
    app.library[1].os.push("linux".into());
    app.fetched_at = Some(1_791_500_000);
    app.gog_titles = app
        .library
        .iter()
        .map(|g| (g.id.clone(), g.title.clone()))
        .collect();
    app.installs.insert(
        "3".into(),
        Install {
            umu_id: None,
            game_id: "3".into(),
            title: "[FAKE] Game 3".into(),
            platform: Platform::Windows,
            path: PathBuf::from("/games/Game 3"),
            client_id: Some("1".into()),
            runner: Runner::Umu {
                proton: "/proton/GE-Proton".into(),
                prefix: "/prefixes/jeu3".into(),
            },
            isolated: true,
        },
    );
    app
}
