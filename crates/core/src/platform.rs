//! What differs between the systems Slatty runs on, Linux and Windows (docs/windows-port.md): on
//! Linux, each of these does what the code always did.

use std::fs::{DirBuilder, File, Metadata, OpenOptions};
use std::io;
use std::path::Path;

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

/// Lowers the calling thread's priority to the lowest, for work that may take minutes.
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
            GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN,
        };
        SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
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
