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
/// let mut modprobe = Modprobe::new().unwrap();
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
/// let mut modprobe = Modprobe::new().unwrap();
///
/// modprobe.probe_exact("virtio-bt", "").unwrap();
/// modprobe.probe_exact("module-doesn't exist", "").unwrap(); // This will fail
///
/// // modprobe will fail in the checks above, ensuring we never get here, as a result
/// // virtio-bt will also not be loaded.
/// modprobe.execute().unwrap();
/// ```
pub struct Modprobe<'a> {
    inner: StagedModprobe<'a>,
    alias: hr_parser::AliasParser,
}

impl<'a> Modprobe<'a> {
    /// Creates a new Modprobe manager using the default ModuleDir system location
    pub fn new() -> Result<Self, std::io::Error> {
        let mdir = ModuleDir::new();
        let alias = hr_parser::AliasParser::new(&mdir)?;

        Ok(Self {
            inner: StagedModprobe::from_module_dir(mdir),
            alias: alias,
        })
    }

    /// Probes the given module which may be an alias
    ///
    /// When the given module string is an alias that maps to more than one real
    /// module, each will be loaded. For this reason, specific parameters cannot be
    /// used.
    ///
    /// If you know what module you want to use, it is better to use [Self::probe_exact]
    /// with its non-aliased name, or [Self::probe_one] with an alias that only maps to
    /// one real module.
    ///
    /// Use this method when you want to load enough to get a device working, such
    /// as when responding to a udev event or looking at a modalias in sysfs.
    pub fn probe_loose(&mut self, module: &str) -> Result<(), std::io::Error> {
        match self.alias.lookup(module) {
            None => self.inner.probe(module, ""),
            Some(aliases) => {
                for alias in aliases {
                    self.inner.probe(&alias, "")?;
                }
                Ok(())
            }
        }
    }

    /// Probes the given module which may be an alias
    ///
    /// As this method allows parameters to be specified, it is expected that the given
    /// module is either a non-alias, or it is an alias that only maps to one module.
    ///
    /// If it is an alias that maps to multiple modules, this method will fail. Use it
    /// when you don't want extraneous modules to be loaded.
    pub fn probe_one(&mut self, module: &str, params: &'a str) -> Result<(), std::io::Error> {
        match self.alias.lookup(module) {
            None => self.inner.probe(module, params),
            Some(aliases) if aliases.len() == 1 => self.inner.probe(&aliases[0], params),
            _ => Err(Error::new(ErrorKind::Other, "More than one module")),
        }
    }

    /// Probes the given module name, skipping looking it up in the alias
    /// list.
    ///
    /// Use this method when you know the given name is not an alias, so there
    /// is no need to waste time looking it up.
    pub fn probe_exact(&mut self, module: &str, params: &'a str) -> Result<(), std::io::Error> {
        self.inner.probe(module, params)
    }

    /// Inserts all staged modules and parameters into the kernel.
    pub fn execute(self) -> Result<(), raw::Error> {
        self.inner.execute()
    }
}

pub struct StagedModprobe<'a> {
    module_dir: ModuleDir,

    staged: Vec<(File, &'a str, raw::FinitFlags)>,
}

impl<'a> StagedModprobe<'a> {
    /// Creates a new Modprobe manager
    pub fn from_module_dir(mdir: ModuleDir) -> Self {
        Self {
            module_dir: mdir,
            staged: Vec::new(),
        }
    }

    /// Stages a module for insertion by looking up its dependencies and pre-opening
    /// all module files for loading.
    pub fn probe(&mut self, module: &str, params: &'a str) -> Result<(), std::io::Error> {
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
            raw::finit_module(module, params, flags).or_else(|err| match err {
                // A module being already loaded ain't no big deal.
                // Modprobe only wants to assure modules get loaded, if they can't
                // be loaded because they already are, well that's fine.
                raw::Error::AlreadyLoaded => Ok(()),
                e => Err(e),
            })?;
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
        let mut modprobe = StagedModprobe::from_module_dir(ModuleDir::new());

        modprobe.probe("virtio_bt", "test").expect("Probing works");

        assert_eq!(3, modprobe.staged.len());
        assert_eq!(modprobe.staged[0].1, "");
        assert_eq!(modprobe.staged[1].1, "");
        assert_eq!(modprobe.staged[2].1, "test");
    }
}
