use super::uname::uname;
use std::fs::File;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

pub mod hr_parser;
pub mod raw;

/// Modprobe manager
///
/// The manager is capable of looking up the dependecies of a given module and will then
/// ensure they are loaded in the correct order.
///
/// This manager works as a two-stage system, whereby all modules are "staged" first and then
/// you must call [Modprobe::execute] to actually load them. This is designed this way so
/// that, when you are loading multiple modules, as many checks can be performed as possible
/// prior to attempting an operation, to reduce the risk of only some modules loading.
///
/// # Examples
///
/// Loading multiple modules successfully:
///
/// ```no_run
/// use kmod_rs::modprobe::Modprobe;
///
/// let mut modprobe = Modprobe::new();
/// modprobe.probe_exact("virtio-bt", "").unwrap();
/// modprobe.probe_exact("virtio-pci", "").unwrap();
///
/// // Finally, actually do the operation
/// // This will attempt to atomically load virtio-bt, virtio-pci and their required
/// // deps.
/// modprobe.execute().unwrap();
/// ```
///
/// An example of a failed module protection:
///
/// ```no_run
/// use kmod_rs::modprobe::Modprobe;
///
/// let mut modprobe = Modprobe::new();
///
/// modprobe.probe_exact("virtio-bt", "").unwrap();
/// modprobe.probe_exact("module-doesn't exist", "").unwrap();
///
/// // modprobe will fail in the checks above, ensuring we never get here, as a result
/// // virtio-bt will also not be loaded.
/// modprobe.execute().unwrap();
/// ```
pub struct Modprobe<'a> {
    module_dir: ModuleDir,

    staged: Vec<(File, &'a str, raw::FinitFlags)>,
}

impl<'a> Modprobe<'a> {
    /// Creates a new Modprobe manager
    pub fn new() -> Self {
        Self {
            module_dir: ModuleDir::new(),
            staged: Vec::new(),
        }
    }

    /// Stages a module for insertion without looking it up in the aliases first.
    pub fn probe_exact(
        &mut self,
        module: &str,
        params: &'a str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut deps = hr_parser::DepParser::new(&self.module_dir)?;

        let mut modules = deps
            .lookup(module)?
            .ok_or(Error::new(ErrorKind::Other, ""))?;

        while modules.len() > 1 {
            self.stage(&modules.pop().unwrap(), "")?;
        }

        Ok(self.stage(&modules.pop().unwrap(), params)?)
    }

    /// Stages a module for insertion
    fn stage(&mut self, module: &str, params: &'a str) -> Result<(), std::io::Error> {
        let flags = if module.ends_with(".ko") {
            raw::FinitFlags::empty()
        } else {
            raw::FinitFlags::COMPRESSED_FILE
        };

        let f = self.module_dir.open(&module)?;

        self.staged.push((f, params, flags));

        Ok(())
    }

    /// Inserts all staged modules and parameters into the kernel.
    pub fn execute(self) -> Result<(), raw::Error> {
        for (module, params, flags) in self.staged {
            raw::finit_module(module, params, flags)?;
        }

        Ok(())
    }
}

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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn probe_exact() {
        let mut modprobe = Modprobe::new();

        modprobe
            .probe_exact("virtio_bt", "test")
            .expect("Probing works");

        assert_eq!(3, modprobe.staged.len());
        assert_eq!(modprobe.staged[0].1, "");
        assert_eq!(modprobe.staged[1].1, "");
        assert_eq!(modprobe.staged[2].1, "test");
    }
}
