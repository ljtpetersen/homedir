// src/lib.rs
//
// Copyright (C) 2023-2024 James Petersen <m@jamespetersen.ca>
// Licensed under Apache 2.0 OR MIT. See LICENSE-APACHE or LICENSE-MIT

#![warn(missing_docs)]

//! This crate exists to provide a portable method to getting any user's home
//! directory. The API is rather simple: there are two main functions,
//! [`get_home`] and [`get_my_home`]. The former can get the home directory
//! of any user provided you have their username. The latter can get the home
//! directory of the user executing this process.
//!
//! If all that is necessary is the home directory of the user executing this process,
//! then other crates may be better options, such as
//! [`directories`](https://crates.io/crates/directories). As well, using the home directory to find the
//! documents, downloads, pictures, etc. directories may not be accurate.
//!
//! This crate aims to work on both Windows and Unix systems. However,
//! Unix systems do not have a unified API. This crate may not work
//! on Unix systems which do not have the `getpwnam_r(3)`, `getpwuid_r(3)`,
//! and `getuid(2)` functions. As well, Windows has its own set of issues.
//! See [for Windows users](#for-windows-users).
//!
//! # Usage
//! This crate is on [crates.io](https://crates.io/crates/homedir) and can be used by executing `cargo add homedir`
//! or adding the following to the dependencies in your `Cargo.toml` file.
//!
//! ```toml
//! [dependencies]
//! homedir = "0.3.0"
//! ```
//!
//! # Examples
//! ## Get the process' user's home directory.
//! ```no_run
//! use homedir::get_my_home;
//!
//! // This assumes that the process' user has "/home/jpetersen" as home directory.
//! assert_eq!(
//!     std::path::Path::new("/home/jpetersen"),
//!     get_my_home().unwrap().unwrap().as_path()
//! );
//! ```
//!
//! ## Get an arbitrary user's home directory.
//! ```no_run
//! use homedir::get_home;
//!
//! // This assumes there is a user named `Administrator` which has
//! // `C:\Users\Administrator` as a home directory.
//! assert_eq!(
//!     std::path::Path::new("C:\\Users\\Administrator"),
//!     get_home("Administrator").unwrap().unwrap().as_path()
//! );
//! assert!(get_home("NonExistentUser").unwrap().is_none());
//! ```
//!
//! # Upgrading from 0.2.1 to 0.3.0
//! The only change *breaking* changes in the API are the
//! variants of [`GetHomeError`] on Windows and the type of `UserIdentifier`
//! on Windows.
//!
//! This version upgrade removes the `wmi` and `serde` dependencies which rendered this crate
//! larger on the Windows version.
//!
//! # For Windows Users
//! This crate uses the
//! [COM library](https://learn.microsoft.com/en-us/windows/win32/com/the-com-library)
//! to access the Windows Management Instrumentation for [`get_home`] (not [`get_my_home`]).
//! To use this library, it is
//! required to call
//! [`CoInitializeEx`](https://learn.microsoft.com/en-us/windows/win32/api/combaseapi/nf-combaseapi-coinitializeex)
//! (or `CoInitialize`), which has
//! [some issues](https://github.com/microsoft/windows-rs/issues/1169). When using this crate,
//! this will only *possibly* present an issue to programs that also use the COM library.
//!
//! Referencing the solution provided in the linked issue, the
//! way that this crate uses the COM library is as follows. It will try to
//! [create an
//! instance](https://learn.microsoft.com/en-us/windows/win32/api/combaseapi/nf-combaseapi-cocreateinstance).
//! If this fails because the COM library is not yet initialized, it will call `CoInitializeEx`
//! using `COINIT_MULTITHREADED`, and it will not call `CoUninitialize` later. This will interfere
//! with libraries that use `OleInitialize`, which requires `COINIT_APARTMENTTHREADED`.
//!
//! To prevent these issues, the feature `windows_no_coinitialize` can be used. If it is specified,
//! then the program will no longer try to call `CoInitializeEx`. Thus, in order to use
//! [`get_home`], it will be necessary to first call `CoInitializeEx` with whatever parameters are
//! required, or initialize the other libraries that use it (for example
//! [`wmi`](https://crates.io/crates/wmi)) first.
//!
//! Finally, this program has been tested on a regular Windows 11 installation. According to MSN,
//! it should work as far back as Windows 7, though it hasn't been tested on it. However, it has
//! not been tested within any Active Directory Windows installation, and the implementation does
//! not test for this or try to account for it in any way. If it does work on these, it will likely
//! return the local profile path of the specified user.

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(windows)] {
        /// Contains the implementation of the crate for Windows systems.
        pub mod windows;
        pub use windows::get_home;
        pub use windows::get_my_home;
        pub use windows::GetHomeError;
    } else if #[cfg(unix)] {
        /// Contains the implementation of the crate for Unix systems.
        pub mod unix;
        pub use unix::get_home;
        pub use unix::get_my_home;
        pub use unix::GetHomeError;
    } else {
        compile_error!("this crate only supports windows and unix systems");
    }
}
