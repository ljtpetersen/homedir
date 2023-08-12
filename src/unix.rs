// src/unix.rs
//
// Copyright (C) 2023 James Petersen <m@jamespetersen.ca>
// Licensed under Apache 2.0 OR MIT. See LICENSE-APACHE or LICENSE-MIT

use std::path::PathBuf;

use cfg_if::cfg_if;

use nix::unistd::Uid;
use nix::unistd::User;

/// The error type returned by [`get_home`] upon failure.
pub type GetHomeError = nix::errno::Errno;

/// Get a user's home directory path.
///
/// If some error occurs when obtaining the path, `Err` is returned. If no user
/// associated with `username` could be found, `Ok(None)` is returned. Otherwise,
/// the path to the user's home directory is returned.
///
/// # Example
/// ```no_run
/// use homedir::get_home;
///
/// // This assumes there is a user named `root` which has
/// // `/root` as a home directory.
/// assert_eq!(
///     "/root".as_ref(),
///     get_home("root").unwrap().unwrap().as_path()
/// );
/// assert!(get_home("nonexistentuser").unwrap().is_none());
/// ```
///
/// This function uses the [`User::from_name`](nix::unistd::User::from_name)
/// method provided by the nix crate. That method uses the
/// [`getpwnam_r(3)`](https://man7.org/linux/man-pages/man3/getpwnam.3.html)
/// library function to get the home directory from the `/etc/passwd` file.
pub fn get_home<S: AsRef<str>>(username: S) -> Result<Option<PathBuf>, GetHomeError> {
    Ok(User::from_name(username.as_ref())?.map(|user| user.dir))
}

/// Get this process' user's home directory path.
///
/// If some error occurs when obtaining the path, `Err` is returned. If no
/// entry in the `/etc/passwd` file corresponds to the process' uid then `Ok(None)` is returned.
/// However, some users may want their `$HOME` environment variable
/// to be checked in that case. This behaviour can be enabled using the `check_env` feature.
///
/// # Example
/// ```no_run
/// use homedir::get_my_home;
///
/// // This assumes that the process' user has "/home/jpetersen" as home directory.
/// assert_eq!(
///     "/home/jpetersen".as_ref(),
///     get_my_home().unwrap().unwrap().as_path()
/// );
///
/// // If there was no entry in the /etc/passwd file corresponding to the uid
/// // of the program, get_my_home() would return Ok(None). If we want
/// // to check the $HOME environment variable as well, we can enable
/// // the check_env feature. In that case, get_my_home() would return
/// // Ok(std::env::var_os("HOME")).
/// ```
///
/// This function uses the [`User::from_uid`](nix::unistd::User::from_uid)
/// method provided by the nix crate. That method uses the
/// [`getpwuid_r(3)`](https://man7.org/linux/man-pages/man3/getpwnam.3.html)
/// library function to get the home directory from the `/etc/passwd` file.
/// The user id is obtained using nix' [`Uid::current`](nix::unistd::Uid::current)
/// method, which uses [`getuid(3)`](https://man7.org/linux/man-pages/man3/getuid.3p.html).
pub fn get_my_home() -> Result<Option<PathBuf>, GetHomeError> {
    Ok(check_env(
        User::from_uid(Uid::current())?.map(|user| user.dir),
    ))
}

cfg_if! {
    if #[cfg(feature = "check_env")] {
        use std::env::var_os;

        fn check_env(opt: Option<PathBuf>) -> Option<PathBuf> {
            opt.or_else(|| var_os("HOME").map(PathBuf::from))
        }
    } else {
        fn check_env(opt: Option<PathBuf>) -> Option<PathBuf> {
            opt
        }
    }
}
