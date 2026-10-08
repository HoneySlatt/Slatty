use slatty_core::auth::Tokens;
use slatty_core::comet::Comet;
use slatty_core::paths::Dirs;
use slatty_core::secret::Secret;

/// Needs `comet` in PATH and port 9977 free: `cargo test -- --ignored comet`.
/// Uses fake tokens, so Comet's own calls to GOG simply fail.
#[tokio::test]
#[ignore]
async fn comet_gets_tokens_without_argv_and_cleans_up() {
    let root = std::env::temp_dir().join(format!("slatty-comet-it-{}", std::process::id()));
    let dirs = Dirs::under(&root);
    let tokens = Tokens {
        user_id: "1234".into(),
        access_token: Secret::new("FAKE-ACCESS-TOKEN-xyz"),
        refresh_token: Secret::new("FAKE-REFRESH-TOKEN-xyz"),
        expires_at: 0,
    };
    let bin = slatty_core::doctor::find_in_path("comet").expect("comet in PATH");
    let comet = Comet::start(&bin, &tokens, "tester", &dirs)
        .await
        .expect("comet starts");

    let pids = std::process::Command::new("pgrep")
        .args(["-x", "comet"])
        .output()
        .unwrap();
    for pid in String::from_utf8_lossy(&pids.stdout).split_whitespace() {
        let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        assert!(
            !String::from_utf8_lossy(&cmdline).contains("FAKE-"),
            "token visible in argv"
        );
    }
    let runtime = dirs.runtime.clone().unwrap();
    let leftovers: Vec<_> = walk(&runtime)
        .into_iter()
        .filter(|p| p.ends_with(".gog.token"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "token file left behind: {leftovers:?}"
    );

    comet.stop().await.unwrap();
    assert!(slatty_core::doctor::comet_port_free());
    assert!(walk(&runtime).is_empty(), "private dir not removed");
    std::fs::remove_dir_all(root).unwrap();
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().is_dir() {
            out.extend(walk(&e.path()));
        } else {
            out.push(e.path());
        }
    }
    out
}
