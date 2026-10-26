# linux-utils

A rust-native implementation of linux kernel module (modprobe) management and uname.

## Features

- Sensible low-level implementations of `finit_module` and `delete_module` with flags
  and return codes mapped to meaningful rust Error types.
- Sensible implementation of `uname` with return values lazily converted from `CStrings`
  into `&OsStr` values on access.
- A higher level implementation of `modprobe`-like functionality:
  - Resolving module alias names to modules.
  - Optional enforcement that an alias may only resolve to one module, or allowing
    all matches to be loaded.
  - Fuzzy matching module names with underscores or hyphens in their names
  - Resolving module dependencies and 
  - Batching of module loads into a semi-atomic operation, so one failure can avoid
    a half modules loaded situation.


