use bitflags::bitflags;
use std::ffi::CString;
use std::os::fd::{AsFd, AsRawFd};
use syscalls::{Errno, Sysno, syscall};

/// Errors returned by [finit_module] or [delete_module].
#[derive(Debug)]
pub enum Error {
    /// The `params` string given to [finit_module] contained a null byte.
    NulError,

    /// The caller was not privileged (did not have the `CAP_SYS_MODULE`
    /// capability), or module loading is disabled (see
    /// /proc/sys/kernel/modules_disabled in
    /// [proc(5)](https://man7.org/linux/man-pages/man5/proc.5.html)).
    Permission,

    /// The module file is too large.
    FileTooBig,

    /// Out of memory.
    OutOfMemory,

    /// A module with this name is already loaded.
    AlreadyLoaded,

    /// The module file is not opened for reading.
    FileNotReadable,

    /// Module signature is invalid or the kernel does not have a key
    /// for this module.  This error is returned only if the kernel was
    /// configured with `CONFIG_MODULE_SIG_FORCE`; if the kernel was not
    /// configured with this option, then an invalid or unsigned module
    /// simply taints the kernel.
    InvalidSignature,

    /// Module signature is misformatted.
    MisformattedSignature,
    DecompressionCheckFailed,
    InvalidParamsOrBrokenELF,

    /// The flag [`FinitFlags::COMPRESSED_FILE`] is set to load a compressed
    /// module, and the kernel was built without
    /// `CONFIG_MODULE_DECOMPRESS`.
    KernelDecompressionNotSupported,

    /// The module could not be unloaded because it is not yet "live" (i.e., it is
    /// still being initialized or is already marked for removal); or, the module
    /// has an init function but has no exit function, and the delete mode was not
    /// [`DeleteMode::Force`].
    ModuleCannotBeUnloaded,

    /// The module could not be unloaded because no module by that name is currently
    /// loaded.
    ModuleDoesNotExist,

    /// The module could not be unloaded because other modules depend on it, or
    /// because the reference count of the module is non zero and the delete mode
    /// was [`DeleteMode::FailEasily`].
    ModuleInUse,
}

bitflags! {
    /// Flags for [finit_module].
    #[derive(Debug)]
    pub struct FinitFlags: usize {
        /// Ignore symbol version hashes.
        const IGNORE_MODVERSIONS = 1;

        /// Ignore kernel version magic.
        const IGNORE_VERMAGIC = 2;

        /// Use in-kernel module decompression.
        const COMPRESSED_FILE = 4;
    }
}

/// Loads a kernel module from the given open file descriptor.
///
/// The given file descriptor must be opened for reading. If the file is compressed,
/// [FinitFlags::COMPRESSED_FILE] should be included in the `flags`. The given
/// param string must not contain any nul bytes.
///
/// # Examples
///
/// Load an uncompressed module with some parameters:
///
/// ```no_run
/// use std::fs::File;
/// use kmod_rs::modprobe::raw::{finit_module, FinitFlags};
///
/// let module_file =
///     File::open("/lib/modules/7.2.11/kernel/drivers/net/virtio_net.ko").unwrap();
///
/// finit_module(module_file, "gso", FinitFlags::empty()).unwrap();
/// ```
///
/// Load a compressed module without any parameters:
///
/// ```no_run
/// use std::fs::File;
/// use kmod_rs::modprobe::raw::{finit_module, FinitFlags};
///
/// let module_file =
///     File::open("/lib/modules/7.2.11/kernel/drivers/net/virtio_net.ko.zst").unwrap();
///
/// finit_module(module_file, "", FinitFlags::COMPRESSED_FILE).unwrap();
/// ```
pub fn finit_module<F, P>(module_file: F, params: P, flags: FinitFlags) -> Result<(), Error>
where
    F: AsFd,
    P: Into<Vec<u8>>,
{
    let ret = unsafe {
        syscall!(
            Sysno::finit_module,
            module_file.as_fd().as_raw_fd(),
            CString::new(params).or(Err(Error::NulError))?.as_ptr(),
            flags.bits()
        )
    };

    match ret {
        Ok(..) => Ok(()),
        Err(Errno::EFBIG) => Err(Error::FileTooBig),
        Err(Errno::EPERM) => Err(Error::Permission),
        Err(Errno::EBADF) => Err(Error::FileNotReadable),
        Err(Errno::ENOMEM) => Err(Error::OutOfMemory),
        Err(Errno::EEXIST) => Err(Error::AlreadyLoaded),
        Err(Errno::ENOKEY) => Err(Error::InvalidSignature),
        Err(Errno::EINVAL) => Err(Error::InvalidParamsOrBrokenELF),
        Err(Errno::ETXTBSY) => Err(Error::FileNotReadable),
        Err(Errno::EBADMSG) => Err(Error::MisformattedSignature),
        Err(Errno::EOPNOTSUPP) => Err(Error::KernelDecompressionNotSupported),

        // This error means we've passed an invalid memory location to the syscall.
        // This is a panic state because we're responsible for passing the memory
        // location in this function, so this error shouldn't be possible.
        Err(Errno::EFAULT) => panic!("Memory provided by CString ptr was invalid"),

        // This error is returned from finit_module if the given fd does not refer to
        // an open file. As we use a BorrowedFd for this method, rust's ownership
        // checking should prevent this from ever happening, so if it does, this is a
        // panic state.
        Err(Errno::ENOEXEC) => panic!("The given fd does not refer to an open file"),

        e => panic!("Unknown response from kernel: {e:?}"),
    }
}

///
pub enum DeleteMode {
    FailEasily,
    Force,
    Block,
}

bitflags! {
    struct OFlags: usize {
        const NONBLOCK = 04000;
        const TRUNC = 01000;
    }
}

/// Unloads a kernel module
///
pub fn delete_module<N: Into<Vec<u8>>>(name: N, mode: DeleteMode) -> Result<(), Error> {
    let raw_flags = match mode {
        DeleteMode::FailEasily => OFlags::NONBLOCK,
        DeleteMode::Block => OFlags::empty(),
        DeleteMode::Force => OFlags::NONBLOCK | OFlags::TRUNC,
    };

    let ret = unsafe {
        syscall!(
            Sysno::delete_module,
            CString::new(name).or(Err(Error::NulError))?.as_ptr(),
            raw_flags.bits()
        )
    };

    match ret {
        Ok(..) => Ok(()),
        Err(Errno::EBUSY) => Err(Error::ModuleCannotBeUnloaded),
        Err(Errno::ENOENT) => Err(Error::ModuleDoesNotExist),
        Err(Errno::EPERM) => Err(Error::Permission),
        Err(Errno::EWOULDBLOCK) => Err(Error::ModuleInUse),

        // This error means we've passed an invalid memory location to the syscall.
        // This is a panic state because we're responsible for passing the memory
        // location in this function, so this error shouldn't be possible.
        Err(Errno::EFAULT) => panic!("Memory provided by CString ptr was invalid"),

        e => panic!("Unknown response from kernel: {e:?}"),
    }
}
