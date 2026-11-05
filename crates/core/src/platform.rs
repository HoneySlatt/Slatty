//! What differs between the systems Slatty runs on, Linux and Windows (docs/windows-port.md): on
//! Linux, each of these does what the code always did.

use std::fs::{DirBuilder, File, Metadata, OpenOptions};
use std::io;
use std::path::Path;

/// Refuses on Windows what Linux alone has: umu and Proton, Wine, isolation, Linux games, Proton
/// builds, the Galaxy service in a prefix (ADR-0027). Their code builds for Windows too, but each
/// way into it asks this first. `Error::Unsupported` on Windows, naming `what`; nothing on Linux.
pub fn linux_only(what: &str) -> crate::Result<()> {
    if cfg!(windows) {
        return Err(crate::Error::Unsupported(format!(
            "not available on Windows: {what}"
        )));
    }
    Ok(())
}

/// Opens without waiting: on Linux a pipe in a game's folder would hold its reader until a writer
/// comes, for ever. Windows' files never wait.
pub fn without_waiting(opts: &mut OpenOptions) -> &mut OpenOptions {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(nix::fcntl::OFlag::O_NONBLOCK.bits())
    }
    #[cfg(windows)]
    {
        opts
    }
}

/// Opens the file itself, never what a link in its place leads to. Linux refuses to open a
/// link then; Windows opens the link itself, which `refuse_link` then refuses.
pub fn not_through_link(opts: &mut OpenOptions) -> &mut OpenOptions {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(nix::libc::O_NOFOLLOW)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        opts.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
    }
}

/// Refuses a file opened `not_through_link` that is a link (on Windows, any reparse point: a
/// symbolic link, a junction). On Linux the opening refused it already.
pub fn refuse_link(file: &File) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::other("a link is in its place"));
        }
    }
    #[cfg(unix)]
    let _ = file;
    Ok(())
}

/// Reading and writing at a place in a file, whatever its cursor.
pub trait FileAt {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()>;
    fn write_all_at(&self, buf: &[u8], offset: u64) -> io::Result<()>;
}

impl FileAt for File {
    #[cfg(unix)]
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> io::Result<()> {
        std::os::unix::fs::FileExt::read_exact_at(self, buf, offset)
    }

    #[cfg(unix)]
    fn write_all_at(&self, buf: &[u8], offset: u64) -> io::Result<()> {
        std::os::unix::fs::FileExt::write_all_at(self, buf, offset)
    }

    #[cfg(windows)]
    fn read_exact_at(&self, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            match self.seek_read(buf, offset) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => {
                    buf = &mut buf[n..];
                    offset += n as u64;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    #[cfg(windows)]
    fn write_all_at(&self, mut buf: &[u8], mut offset: u64) -> io::Result<()> {
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            match self.seek_write(buf, offset) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => {
                    buf = &buf[n..];
                    offset += n as u64;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

/// Gives `path` the permissions `mode` on Linux (0755 to make a program executable, 0700 for a
/// private folder). Windows has no such modes: a program is one by its name, and the user's
/// profile keeps what is in it from other users.
pub fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
    }
    #[cfg(windows)]
    {
        let _ = (path, mode);
        Ok(())
    }
}

/// Whether a file can be run as a program by its permissions; on Windows, by its name.
pub fn is_executable(path: &Path, meta: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = path;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(windows)]
    {
        let _ = meta;
        path.extension().is_some_and(|e| {
            ["exe", "bat", "cmd", "com"]
                .iter()
                .any(|x| e.eq_ignore_ascii_case(x))
        })
    }
}

/// Makes `link` a symbolic link to `target`. Only Linux's tools (umu, Proton, Wine prefixes) are
/// given links: on Windows, making one needs a right users do not have by default.
pub fn symlink(target: &Path, link: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        let _ = (target, link);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "links are made on Linux alone",
        ))
    }
}

/// Deletes a file, or a link itself, never what it leads to. Windows deletes a link to a folder
/// (a junction, a folder's symbolic link) as a folder, and refuses to as a file.
pub fn remove_file_or_link(path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;
        if std::fs::symlink_metadata(path)?
            .file_type()
            .is_symlink_dir()
        {
            return std::fs::remove_dir(path);
        }
    }
    std::fs::remove_file(path)
}

/// Makes `link` lead to the folder `target`, as a game or a person could: a symbolic link on
/// Linux, a junction on Windows, which any user can make there (`mklink /J`).
#[cfg(test)]
pub(crate) fn link_folder(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    {
        let made = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .args([link, target])
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(made.success(), "mklink /J {}", link.display());
    }
}

/// Folders made readable by the user alone (0700 on Linux).
pub fn private_dirs(builder: &mut DirBuilder) -> &mut DirBuilder {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700)
    }
    #[cfg(windows)]
    {
        builder
    }
}

/// A file made readable by the user alone (0600 on Linux).
pub fn private_file(opts: &mut OpenOptions) -> &mut OpenOptions {
    with_mode(opts, 0o600)
}

/// A file made with the permissions `mode` on Linux.
pub fn with_mode(opts: &mut OpenOptions, mode: u32) -> &mut OpenOptions {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(mode)
    }
    #[cfg(windows)]
    {
        let _ = mode;
        opts
    }
}

/// Lets the file at `path` be replaced or deleted, as Linux lets any file whose folder is
/// writable be: Windows refuses to for a file marked read-only, as a game or its setup may leave
/// its saves or its files. The mark is taken off; the file that replaces it has none, as on Linux.
/// Nothing at `path` is no error.
pub fn make_replaceable(path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        let mut permissions = match std::fs::symlink_metadata(path) {
            Ok(meta) => meta.permissions(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        if permissions.readonly() {
            permissions.set_readonly(false);
            std::fs::set_permissions(path, permissions)?;
        }
    }
    #[cfg(unix)]
    let _ = path;
    Ok(())
}

/// When a file was last changed, in seconds and nanoseconds since 1970, as Linux's `stat` gives
/// it.
pub fn modified(meta: &Metadata) -> (i64, i64) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        (meta.mtime(), meta.mtime_nsec())
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        since_1970(meta.last_write_time() as i64)
    }
}

/// The bytes a file takes on the disk: less than its size while it has holes (a download writing
/// its parts out of order). Windows' are taken as its size.
pub fn allocated(meta: &Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.blocks() * 512
    }
    #[cfg(windows)]
    {
        meta.len()
    }
}

/// Seconds and nanoseconds since 1970 of a Windows time (100 ns since 1601).
#[cfg(windows)]
fn since_1970(ticks: i64) -> (i64, i64) {
    const FROM_1601: i64 = 11_644_473_600;
    let ticks = ticks - FROM_1601 * 10_000_000;
    (
        ticks.div_euclid(10_000_000),
        ticks.rem_euclid(10_000_000) * 100,
    )
}

/// What tells a file from another put in its place: the device and inode on Linux, the volume and
/// file index on Windows; on Linux, when it was made, which tells it from an older file whose inode
/// it got back; and its last change of content and of any kind (Linux's change time, Windows' change
/// time), which writes move and no program sets back. `None` when nothing is at `path`; a link
/// there is followed with `follow`, else described.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    pub device: u64,
    pub file: u64,
    /// `None` where the filesystem keeps no creation time (ext4 with 128-byte inodes, some network
    /// filesystems), and on Windows. There the file index already changes for a file made again
    /// (NTFS's holds a sequence number), and the creation time can lie: Windows gives a name made
    /// again within 15 seconds the old one's, and Wine gives the last write's.
    pub created: Option<(i64, i64)>,
    pub modified: (i64, i64),
    pub changed: (i64, i64),
}

pub fn identity(path: &Path, follow: bool) -> Option<Identity> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = match follow {
            true => std::fs::metadata(path),
            false => std::fs::symlink_metadata(path),
        }
        .ok()?;
        Some(Identity {
            device: m.dev(),
            file: m.ino(),
            created: m
                .created()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| (d.as_secs() as i64, d.subsec_nanos() as i64)),
            modified: (m.mtime(), m.mtime_nsec()),
            changed: (m.ctime(), m.ctime_nsec()),
        })
    }
    #[cfg(windows)]
    {
        windows::identity(path, follow)
    }
}

/// A folder of the user's that Windows knows, where games keep their saves.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserFolder {
    /// `Saved Games`.
    SavedGames,
    /// `Documents`.
    Documents,
    /// `AppData\Local`.
    LocalAppData,
    /// `AppData\LocalLow`, for programs run with low rights (browsers' plug-ins, some games).
    LocalAppDataLow,
    /// `AppData\Roaming`.
    RoamingAppData,
}

/// Where `folder` is for the user running Slatty: wherever the person, or their organisation,
/// moved it (`SHGetKnownFolderPath`), not where it usually is.
#[cfg(windows)]
pub fn user_folder(folder: UserFolder) -> io::Result<std::path::PathBuf> {
    windows::user_folder(folder)
}

/// The bytes available to the user on the filesystem holding `path`.
pub fn free_space(path: &Path) -> io::Result<u64> {
    #[cfg(unix)]
    {
        let st = nix::sys::statvfs::statvfs(path)?;
        Ok(st.blocks_available() as u64 * st.fragment_size() as u64)
    }
    #[cfg(windows)]
    {
        windows::free_space(path)
    }
}

/// Flushes what was written to the filesystem holding the folder `dir`. Windows has no such call
/// for a user: each file's own flush is all there is.
pub fn sync_filesystem(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        nix::unistd::syncfs(File::open(dir)?).map_err(io::Error::from)
    }
    #[cfg(windows)]
    {
        let _ = dir;
        Ok(())
    }
}

/// A moment on a clock that stops while the computer sleeps, for play time: a night asleep with
/// a game open is not play, nor is the wall clock set meanwhile. Linux's `Instant` is such a clock
/// (`CLOCK_MONOTONIC`); Windows' counts sleep (`QueryPerformanceCounter`, as Microsoft's
/// "Acquiring high-resolution time stamps" says), its unbiased interrupt time does not.
#[derive(Debug, Clone, Copy)]
pub struct Awake {
    #[cfg(unix)]
    at: std::time::Instant,
    /// In 100 ns since the computer started, sleep left out.
    #[cfg(windows)]
    at: u64,
}

impl Awake {
    /// This moment.
    pub fn now() -> Self {
        #[cfg(unix)]
        {
            Self {
                at: std::time::Instant::now(),
            }
        }
        #[cfg(windows)]
        {
            let mut at = 0;
            // SAFETY: writes the time to a valid u64; it cannot fail since Windows 7.
            unsafe {
                windows_sys::Win32::System::WindowsProgramming::QueryUnbiasedInterruptTime(&mut at)
            };
            Self { at }
        }
    }

    /// The time awake since `self`. On Windows its resolution is the system's timer, about 16 ms.
    pub fn elapsed(&self) -> std::time::Duration {
        #[cfg(unix)]
        {
            self.at.elapsed()
        }
        #[cfg(windows)]
        {
            std::time::Duration::from_nanos(Self::now().at.saturating_sub(self.at) * 100)
        }
    }
}

/// Lowers the calling thread's priority to the lowest, for work that may take minutes. On Windows
/// its disk and memory priorities too (background mode), and its processor priority to idle, as
/// nice 19 does: background mode alone left it at the usual one under Wine.
pub fn lowest_priority() {
    #[cfg(unix)]
    // SAFETY: sets the calling thread's nice value, which Linux keeps per thread.
    unsafe {
        nix::libc::setpriority(nix::libc::PRIO_PROCESS, 0, 19);
    }
    #[cfg(windows)]
    // SAFETY: the pseudo handle names the calling thread.
    unsafe {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN, THREAD_PRIORITY_IDLE,
        };
        SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_IDLE);
    }
}

#[cfg(windows)]
mod windows {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;

    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_BASIC_INFO, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FileBasicInfo, GetDiskFreeSpaceExW,
        GetFileInformationByHandle, GetFileInformationByHandleEx,
    };

    use super::{Identity, since_1970};

    pub fn identity(path: &Path, follow: bool) -> Option<Identity> {
        // A folder opens only with backup semantics.
        let link = if follow {
            0
        } else {
            FILE_FLAG_OPEN_REPARSE_POINT
        };
        let file = std::fs::OpenOptions::new()
            .access_mode(0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | link)
            .open(path)
            .ok()?;
        let handle = file.as_raw_handle();
        // SAFETY: the handle is open for the calls, and each structure is as large as told.
        unsafe {
            let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
            if GetFileInformationByHandle(handle, &mut info) == 0 {
                return None;
            }
            let mut basic: FILE_BASIC_INFO = std::mem::zeroed();
            if GetFileInformationByHandleEx(
                handle,
                FileBasicInfo,
                (&mut basic as *mut FILE_BASIC_INFO).cast(),
                size_of::<FILE_BASIC_INFO>() as u32,
            ) == 0
            {
                return None;
            }
            Some(Identity {
                device: info.dwVolumeSerialNumber as u64,
                file: (info.nFileIndexHigh as u64) << 32 | info.nFileIndexLow as u64,
                created: None,
                modified: since_1970(basic.LastWriteTime),
                changed: since_1970(basic.ChangeTime),
            })
        }
    }

    pub fn user_folder(folder: super::UserFolder) -> io::Result<std::path::PathBuf> {
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::System::Com::CoTaskMemFree;
        use windows_sys::Win32::UI::Shell::{
            FOLDERID_Documents, FOLDERID_LocalAppData, FOLDERID_LocalAppDataLow,
            FOLDERID_RoamingAppData, FOLDERID_SavedGames, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
        };
        let id = match folder {
            super::UserFolder::SavedGames => &FOLDERID_SavedGames,
            super::UserFolder::Documents => &FOLDERID_Documents,
            super::UserFolder::LocalAppData => &FOLDERID_LocalAppData,
            super::UserFolder::LocalAppDataLow => &FOLDERID_LocalAppDataLow,
            super::UserFolder::RoamingAppData => &FOLDERID_RoamingAppData,
        };
        let mut path = std::ptr::null_mut();
        // SAFETY: Windows writes a nul-terminated string it allocated to `path`, freed here once
        // copied, also after a failure, as its documentation asks.
        unsafe {
            let result =
                SHGetKnownFolderPath(id, KF_FLAG_DEFAULT as u32, std::ptr::null_mut(), &mut path);
            let found = (result >= 0 && !path.is_null()).then(|| {
                let len = (0..).take_while(|&i| *path.add(i) != 0).count();
                std::ffi::OsString::from_wide(std::slice::from_raw_parts(path, len))
            });
            CoTaskMemFree(path.cast());
            found.map(std::path::PathBuf::from).ok_or_else(|| {
                io::Error::other(format!("Windows does not know the folder {folder:?}"))
            })
        }
    }

    pub fn free_space(path: &Path) -> io::Result<u64> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let mut available = 0u64;
        // SAFETY: the name ends with a nul, and the count is written to a valid u64.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(available)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clock of play time moves with the time awake, at the timer's resolution.
    #[test]
    fn the_awake_clock_follows_the_time_awake() {
        let start = Awake::now();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let elapsed = start.elapsed();
        assert!(
            elapsed >= std::time::Duration::from_millis(150)
                && elapsed < std::time::Duration::from_secs(5),
            "{elapsed:?}"
        );
    }

    /// The ways into what Linux alone has that no other test of Windows reaches refuse there,
    /// before anything is read, written or asked of the network (ADR-0027): a GOG Linux build's
    /// install (`--restart` included, which would delete its download), a Proton build's download,
    /// the Galaxy service in a prefix. The others are tested where they are: `runner`, `install`,
    /// and the command line's `play_windows.rs`.
    #[cfg(windows)]
    #[tokio::test]
    async fn on_windows_what_linux_alone_has_is_refused_before_anything() {
        use crate::Error;
        use crate::install::{Install, Platform};
        let root = std::env::temp_dir().join(format!("slatty-linux-only-{}", std::process::id()));
        let dirs = crate::paths::Dirs::under(&root);
        let db = crate::db::Db::in_memory().unwrap();
        let http = crate::http::client().unwrap();
        let tokens = crate::auth::Tokens {
            user_id: "1".into(),
            access_token: crate::secret::Secret::new("[FAKE]"),
            refresh_token: crate::secret::Secret::new("[FAKE]"),
            expires_at: i64::MAX,
        };
        let unsupported = |r: &Error| matches!(r, Error::Unsupported(_));
        let partial = crate::installer::partial_dir(&root.join("games"), "[FAKE] Game");
        std::fs::create_dir_all(&partial).unwrap();
        crate::installer::InstallJob {
            game_id: "1".into(),
            build_id: "linux:1.0".into(),
            language: "en-US".into(),
            root: root.join("games"),
            directory: "[FAKE] Game".into(),
            state: crate::installer::PAUSED.into(),
            dlcs: Vec::new(),
            proton: None,
        }
        .save(&db)
        .unwrap();
        for (platform, restart) in [(Platform::Linux, false), (Platform::Linux, true)] {
            let request = crate::installer::InstallRequest {
                game_id: "1".into(),
                language: None,
                build: None,
                platform,
                root: root.join("games"),
                proton: std::path::PathBuf::new(),
                dlcs: crate::installer::DlcSelection::AllOwned,
                restart,
            };
            let cancel = tokio_util::sync::CancellationToken::new();
            let result =
                crate::installer::install(&db, &dirs, &http, &tokens, request, |_| {}, cancel)
                    .await;
            assert!(result.as_ref().is_err_and(unsupported), "{result:?}");
        }
        assert!(
            partial.is_dir(),
            "the Linux build's download is left as it was"
        );

        crate::settings::set_proton_downloads(&db, true).unwrap();
        assert!(crate::protons::check_allowed(&db).is_err_and(|e| unsupported(&e)));
        let release = crate::protons::Release {
            source: crate::protons::Source::GeProton,
            version: "[FAKE]".into(),
            name: "[FAKE]".into(),
            size: 1,
            url: "https://[FAKE]/a.tar.gz".into(),
            archive: "a.tar.gz".into(),
            checksum_url: "https://[FAKE]/a.sha512sum".into(),
        };
        let cancel = tokio_util::sync::CancellationToken::new();
        let result = crate::protons::install(&http, &dirs, &release, |_| {}, &cancel).await;
        assert!(result.as_ref().is_err_and(unsupported), "{result:?}");

        let install = Install {
            umu_id: None,
            game_id: "1".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: root.join("games/[FAKE] Game"),
            client_id: None,
            runner: crate::runner::Runner::Native,
            isolated: false,
        };
        let exe = root.join("GalaxyCommunication.exe");
        let result = crate::galaxy_service::ensure(&dirs, &install, &exe, &exe).await;
        assert!(result.as_ref().is_err_and(unsupported), "{result:?}");
        let _ = std::fs::remove_dir_all(root);
    }
}
