# homedir

This crate exists to provide a portable method to getting any user's home
directory. The API is rather simple: there are two functions,
`get_home` and `get_my_home`. The former can get the home directory
of any user provided you have their username. The latter can get the home
directory of the user executing this process.

This crate aims to work on both Windows and Unix systems. However,
Unix systems do not have a unified API. This crate may not work
on Unix systems which do not have the `getpwnam_r(3)`, `getpwuid_r(3)`,
and `getuid(2)` functions.

This crate is a small crate with very few dependencies. It depends on the [`cfg-if`](https://crates.io/crates/cfg-if) crate,
as well as a couple others depending on the operating system. On Unix systems,
the other dependency is the [`nix`](https://crates.io/crates/nix) package, whereas on Windows systems,
it depends on the [`windows-sys`](https://crates.io/crates/windows-sys) and [`widestring`](https://crates.io/crates/widestring)
packages.

## Usage
This crate is on [crates.io](https://crates.io/crates/homedir) and can be used by executing `cargo add homedir`
or adding the following to the dependencies in your `Cargo.toml` file.

```toml
[dependencies]
homedir = "0.1.0"
```

By default, on Unix, if the library cannot find the current user's home directory
in the `/etc/passwd` file, it will return `None`. However, it is possible that
some users want it to check the `$HOME` environment variable instead.
The `check_env` feature flag can be set to enable this behaviour. To enable
this feature, replace the above lines in the `Cargo.toml` with

```toml
[dependencies]
homedir = { version = "0.1.0", features = ["fast_fail"] }
```

This feature is only useful on Unix systems; it has no effect on Windows.

## Examples
### Get the process' user's home directory.
```rust
use homedir::get_my_home;

// This assumes that the process' user has "/home/jpetersen" as home directory.
assert_eq!(
    "/home/jpetersen".as_ref(),
    get_my_home().unwrap().unwrap().as_path()
);
```

### Get an arbitrary user's home directory.
```rust
use homedir::get_home;

// This assumes there is a user named `Administrator` which has
// `C:\Users\Administrator` as a home directory.
assert_eq!(
    "C:\\Users\\Administrator".as_ref(),
    get_home("Administrator").unwrap().unwrap().as_path()
);
assert!(get_home("NonExistentUser").unwrap().is_none());
```

## Licensing
Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT License
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitely state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.

Feel free to put a copyright header in your name in any files you contribute to.

## Copyright
Copyright (C) 2023 James Petersen <m@jamespetersen.ca>.
