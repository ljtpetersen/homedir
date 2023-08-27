# Change Log

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2023-08-27

### Added
- `get_id`, `get_home_from_id`, and `get_my_id` functions.
- A `UserIdentifier` type alias.

### Changed
- The `get_my_home` implementation on Unix systems now checks
    the `$HOME` environment variable first before checking the `/etc/passwd`
    directory.

### Removed
- The `check_env` feature flag.

## [0.1.0] - 2023-08-12
The first release of this crate.

[0.2.0]: https://github.com/ljtpetersen/homedir/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/ljtpetersen/homedir/releases/tag/v0.1.0
