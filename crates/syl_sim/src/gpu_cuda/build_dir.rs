//! Protect the short interval between creating a staging directory and
//! locking its owner file. The cache-wide lock never covers compilation.
use fs2::FileExt;
use std::{
    fs::{self, File},
    io,
    path::Path,
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
    pub(super) fn owner(&self) -> &File {
        &self.owner
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
