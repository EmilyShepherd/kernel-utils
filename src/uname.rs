use std::ffi::{CStr, OsStr};
use std::os::unix::ffi::OsStrExt;
use syscalls::{Sysno, raw_syscall};

#[repr(C)]
#[derive(Debug)]
struct utsname {
    sysname: [u8; 65],
    nodename: [u8; 65],
    release: [u8; 65],
    version: [u8; 65],
    machine: [u8; 65],
    domainname: [u8; 65],
}

impl utsname {
    pub fn new() -> Self {
        Self {
            sysname: [0; 65],
            nodename: [0; 65],
            release: [0; 65],
            version: [0; 65],
            machine: [0; 65],
            domainname: [0; 65],
        }
    }
}

/// The returned system information from the Linux Kernel.
pub struct UtsName {
    inner: utsname,
}

impl From<utsname> for UtsName {
    fn from(u: utsname) -> Self {
        Self { inner: u }
    }
}

macro_rules! hydrate {
    ($i:ident) => {
        /// Lazily checks that the returned string from the kernel is valid (null-terminated)
        /// and returns it in a OsStr represenation.
        pub fn $i(&self) -> &OsStr {
            OsStr::from_bytes(
                CStr::from_bytes_until_nul(&self.inner.$i)
                    .expect("Kernel returned a non-null terminated string!")
                    .to_bytes()
            )
        }
    };
}

impl UtsName {
    hydrate! {sysname}
    hydrate! {nodename}
    hydrate! {release}
    hydrate! {version}
    hydrate! {machine}
    hydrate! {domainname}
}

/// Fetches system information from the kernel.
///
/// The returned UtsName struct wraps the returned values and lazily validates each
/// field as used.
pub fn uname() -> UtsName {
    let n = utsname::new();

    // SAFETY: We have instantiated the utsname C-compatible struct within this
    // function, so passing a pointer to it to the uname syscall is safe.
    if unsafe { raw_syscall!(Sysno::uname, &n as *const utsname) } != 0 {
        // Uname can only fail if the provided buffer is invalid. As we've are
        // solely in control of that in this function, this is a panic state.
        panic!("Uname failed");
    }

    n.into()
}
