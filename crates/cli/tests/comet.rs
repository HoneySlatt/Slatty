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
    let dirs = private_dirs(&root);
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

/// Comet logs a failed token request with its address, whose query holds the account's refresh
/// token. Run without a network, so that the request fails and nothing reaches GOG:
/// `unshare -rn sh -c 'ip link set lo up && cargo test -- --ignored comets_log'`.
#[tokio::test]
#[ignore]
async fn comets_log_never_holds_the_refresh_token() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let root = std::env::temp_dir().join(format!("slatty-comet-log-{}", std::process::id()));
    let dirs = private_dirs(&root);
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
    // A game's sign-in: Comet asks GOG for a token of the game's own client.
    let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", slatty_core::comet::PORT))
        .await
        .unwrap();
    socket
        .write_all(&auth_info_request("1234567890", "[FAKE]secret"))
        .await
        .unwrap();
    // Comet answers, or drops the connection, once that request has failed.
    let mut answer = [0u8; 4096];
    let _ =
        tokio::time::timeout(std::time::Duration::from_secs(30), socket.read(&mut answer)).await;
    comet.stop().await.unwrap();
    let log = std::fs::read_to_string(dirs.logs().join("comet.log")).unwrap();
    assert!(
        !log.contains("FAKE-REFRESH-TOKEN"),
        "the refresh token is in comet.log"
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// Comet's `AUTH_INFO_REQUEST` (communication service, sort 1, type 3) as a game sends it: a
/// header's length, the header, then the request.
fn auth_info_request(client_id: &str, client_secret: &str) -> Vec<u8> {
    let field = |number: u8, value: &str| {
        let mut out = vec![number << 3 | 2, value.len() as u8];
        out.extend_from_slice(value.as_bytes());
        out
    };
    let mut request = field(1, client_id);
    request.extend(field(2, client_secret));
    request.extend([5 << 3, 1]);
    let header = [1 << 3, 1, 2 << 3, 3, 3 << 3, request.len() as u8];
    let mut out = (header.len() as u16).to_be_bytes().to_vec();
    out.extend(header);
    out.extend(request);
    out
}

/// Folders under `root`, which stands for `XDG_RUNTIME_DIR` too and is therefore the user's alone.
fn private_dirs(root: &std::path::Path) -> Dirs {
    use std::os::unix::fs::DirBuilderExt;
    let _ = std::fs::remove_dir_all(root);
    std::fs::DirBuilder::new().mode(0o700).create(root).unwrap();
    Dirs::under(root)
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
