//! Protect the short interval between creating a staging directory and
//! locking its owner file. The cache-wide lock never covers compilation.
use fs2::FileExt;
use std::{
    fs::{self, File},
    io,
    path::Path,
    process::Command,
};
use tempfile::TempDir;

fn directory_lock(cache: &Path) -> io::Result<File> {
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(cache.join(".staging.lock"))?;
    FileExt::lock_exclusive(&lock)?;
    Ok(lock)
}

/// Let the compiler and its descendants keep the build directory alive if
/// the launching process exits. Wrappers may freely redirect standard I/O.
fn inherit_owner(compiler: &mut Command, owner: &File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::{fd::AsRawFd, unix::process::CommandExt};
        let owner = owner.try_clone()?;
        // SAFETY: the closure owns the file and only calls async-signal-safe
        // fcntl in the child. F_DUPFD allocates an unused, non-CLOEXEC fd >= 3.
        unsafe {
            compiler.pre_exec(move || {
                if libc::fcntl(owner.as_raw_fd(), libc::F_DUPFD, 3) == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    #[cfg(not(unix))]
    compiler.stdin(owner.try_clone()?);
    Ok(())
}

pub(super) struct BuildDir {
    dir: TempDir,
    owner: File,
}

impl BuildDir {
    pub(super) fn create(cache: &Path) -> io::Result<Self> {
        let _guard = directory_lock(cache)?;
        let dir = tempfile::Builder::new()
            .prefix(".compile-")
            .tempdir_in(cache)?;
        let owner = File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(dir.path().join("owner.lock"))?;
        FileExt::lock_exclusive(&owner)?;
        Ok(Self { dir, owner })
    }
    pub(super) fn path(&self) -> &Path {
        self.dir.path()
    }
    pub(super) fn compiler(&self, nvcc: &Path) -> io::Result<Command> {
        let mut compiler = Command::new(nvcc);
        inherit_owner(&mut compiler, &self.owner)?;
        Ok(compiler)
    }
}

pub(super) fn reclaim(cache: &Path) -> io::Result<()> {
    let _guard = directory_lock(cache)?;
    for entry in fs::read_dir(cache)? {
        let entry = entry?;
        if !entry
            .file_name()
            .as_encoded_bytes()
            .starts_with(b".compile-")
        {
            continue;
        }
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => {}
            Ok(_) => continue,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        }
        let path = entry.path();
        let _owner = match File::options()
            .read(true)
            .write(true)
            .open(path.join("owner.lock"))
        {
            Ok(owner) => match FileExt::try_lock_exclusive(&owner) {
                Ok(()) => Some(owner),
                Err(e) if e.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
                    continue;
                }
                Err(e) => return Err(e),
            },
            // Interrupted initialization or a staging directory from an older runner.
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        match fs::remove_dir_all(path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
