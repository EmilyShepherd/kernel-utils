use super::uname::uname;
use std::fs::File;
use std::path::{Path, PathBuf};

pub mod hr_parser;
pub mod raw;

/// Represents a Module directory (eg /lib/modules/(release-name)) in its standard
/// format.
///
/// This is a bit of a hacky solution until
/// [std::fs::Dir](https://doc.rust-lang.org/std/fs/struct.Dir.html#method.open) is GA.
pub struct ModuleDir {
    dir: PathBuf,
}

impl ModuleDir {
    /// Creates a new ModuleDir by assuming it exists in the standard location
    /// (`/lib/modules`) and uses the currently running system's release information
    /// to locate the correct subfolder.
    pub fn new() -> Self {
        let info = uname();
        let release = info.release();
        let mut modules_dir = Path::new("/lib/modules").to_path_buf();
        modules_dir.push(release);

        Self { dir: modules_dir }
    }

    /// Opens a given path within the module directory.
    pub fn open<T: AsRef<Path>>(&self, file: T) -> Result<File, std::io::Error> {
        let mut new_dir = self.dir.clone();

        new_dir.push(file);

        File::open(new_dir.as_path())
    }
}
