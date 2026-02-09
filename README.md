# slurm_jd
![License](https://img.shields.io/badge/license-MIT-blue)
[![Build status](https://img.shields.io/github/actions/workflow/status/chem-william/slurm_jd/check.yml?label=Build%20status)](https://github.com/chem-william/slurm_jd/actions/workflows/check.yml)

A small program to list finished jobs on a SLURM queue system

![jobs_done output](assets/screenshot.png)

## Installation

### Quick install (recommended)

```sh
curl -fsSL https://raw.githubusercontent.com/chem-william/slurm_jd/main/install.sh | bash
```

The script downloads the latest release to `~/.local/bin/`, checks your `PATH`,
and optionally sets up a login hook and alias (see [Setup](#setup) below).

### Download release manually

Grab the latest tarball from the
[Releases page](https://github.com/chem-william/slurm_jd/releases/latest),
extract it, and place the `jobs_done` binary somewhere on your `PATH`:

```sh
tar -xzf jobs_done-*.tar.gz
install -m 755 jobs_done-*/jobs_done ~/.local/bin/
```

### Build from source

Requires the [Rust toolchain](https://rustup.rs/):

```sh
cargo install --git https://github.com/chem-william/slurm_jd
```

## Setup

The install script can configure these for you automatically. To set them up
manually, add the following to your shell config files:

**Show finished jobs on login** — add to `~/.bash_profile`:

```sh
jobs_done
```

**Quick shortcut for the last 24 hours** — add to `~/.bashrc`:

```sh
alias jd="jobs_done --day"
```

## Usage

```sh
# Jobs since last session (default)
jobs_done

# Jobs from the last 4 hours
jobs_done 4

# Jobs from today
jobs_done --day

# Jobs since a specific time
jobs_done --since 2025-01-01T00:00:00

# Filter by job state
jobs_done --state FAILED
jobs_done --state FAILED --state TIMEOUT

# Query a specific user
jobs_done -u <username>
```

## Contributing

Contributions are welcome! Open a pull request to fix a bug, or [open an issue][]
to discuss a new feature or change.

Check out the [Contributing][] section in the docs for more info.

[Contributing]: CONTRIBUTING.md
[open an issue]: https://github.com/chem-william/slurm_jd/issues

## License

This project is licensed under the MIT license.

`slurm_jd` can be distributed according to the MIT license. Contributions
will be accepted under the same license.

## Authors

* [William Bro-Jørgensen](https://github.com/chem-william)
