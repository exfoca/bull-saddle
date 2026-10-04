<!-- BullSaddle — Developed by Orion Impact (https://orion-impact.com) -->

<!-- Licensed under the Apache License, Version 2.0. -->

<!-- SPDX-License-Identifier: Apache-2.0 -->

<p align="center">
  <img src="branding/bulls.png" alt="BullSaddle" width="160">
</p>

<h1 align="center">BullSaddle</h1>

<p align="center">
  <strong>Know your repositories. Understand your workspace.</strong>
</p>

BullSaddle is a local-first tool for discovering, observing and querying Git repositories across a computer.

Git remains authoritative over repository state. BullSaddle maintains its own persistent workspace view of repositories, locations, worktrees, observations and advisories without replacing Git or requiring a remote service.

> **Git knows the repository. BullSaddle knows the herd.**

## Table of contents

* [Quick start](#quick-start)
* [Command reference](#command-reference)
* [Repository selectors](#repository-selectors)
* [Configuration](#configuration)
* [Structured output](#structured-output)
* [Exit codes](#exit-codes)
* [Localization](#localization)
* [Operational model](#operational-model)
* [Platform support](#platform-support)
* [Build from source](#build-from-source)
* [Development and validation](#development-and-validation)
* [License](#license)

## Quick start

Discover repositories:

```bash
bulls discover ~/Projects
```

Observe their current local Git state:

```bash
bulls refresh
```

View the workspace:

```bash
bulls overview
```

List known repositories:

```bash
bulls repos
```

Inspect the repository represented by the current directory:

```bash
bulls inspect .
```

The lifecycle is explicit:

```text
discover
    ↓
persistent repository inventory
    ↓
refresh
    ↓
persistent Git observations
    ↓
repos / inspect / overview
```

Query commands do not silently perform discovery or refresh.

## Command reference

### `bulls discover`

Discover and reconcile Git repositories under explicit or configured filesystem roots.

```bash
bulls discover [--format <human|json>] [--verbose] [root ...]
```

Examples:

```bash
bulls discover
bulls discover ~/Projects
bulls discover ~/Projects ~/Research
bulls discover ~/Projects --verbose
bulls discover ~/Projects --format json
```

Explicit roots apply to that invocation.

`--verbose` exposes individual discovery diagnostics and is intended for human-readable output. Diagnostic output may contain local filesystem information.

Discovery does not modify discovered Git repositories.

---

### `bulls refresh`

Observe the local Git state of repositories and worktrees already known to BullSaddle.

```bash
bulls refresh [--format <human|json>]
```

Examples:

```bash
bulls refresh
bulls refresh --format json
```

Refresh does not discover new repositories.

It records local state such as:

* HEAD state;
* branch state;
* upstream state;
* ahead/behind relationship;
* staged changes;
* unstaged changes;
* untracked files;
* remotes;
* observation status and freshness.

Refresh does not perform `fetch`, `pull` or `push`.

---

### `bulls repos`

List repositories in the persisted workspace.

```bash
bulls repos [--offset <n>] [--limit <n>] [--format <human|json>]
```

Examples:

```bash
bulls repos
bulls repos --limit 10
bulls repos --offset 10 --limit 10
bulls repos --format json
```

The repository model preserves the distinction between repositories, locations and worktrees.

---

### `bulls inspect`

Inspect one known repository.

```bash
bulls inspect <repository-selector> [--format <human|json>]
```

Examples:

```bash
bulls inspect repository-00000000000000000000000000000001
bulls inspect .
bulls inspect ~/Projects/my_project
bulls inspect . --format json
```

The command reads persisted BullSaddle knowledge. It does not silently refresh Git state.

Observation freshness is shown where relevant so persisted knowledge is not confused with a live Git query.

---

### `bulls overview`

Show an aggregate view of the persisted workspace.

```bash
bulls overview [--format <human|json>]
```

Examples:

```bash
bulls overview
bulls overview --format json
```

The overview includes workspace-level information such as repository counts, worktrees, known local work, remote state and advisories.

Unknown state remains distinct from negative state.

---

### `bulls config show`

Inspect the effective runtime configuration.

```bash
bulls config show
```

The command reports:

* configuration file path;
* whether configuration came from a file or built-in defaults;
* locale;
* discovery roots;
* exclusions;
* bare-repository policy;
* symlink traversal policy;
* filesystem boundary policy;
* observation parallelism;
* Git process timeout.

This command is read-only.

---

### `bulls reset`

Discard BullSaddle-owned persisted workspace knowledge.

```bash
bulls reset
```

Reset removes BullSaddle workspace state such as:

* known repository inventory;
* locations;
* worktrees;
* observations;
* rebuildable derived state.

It does **not** modify:

* Git repositories;
* commits;
* branches;
* refs;
* project files;
* Git worktrees;
* user configuration.

After a reset, rebuild the workspace explicitly:

```bash
bulls discover ~/Projects
bulls refresh
```

## Repository selectors

`bulls inspect` accepts two canonical selector forms.

### Repository ID

```bash
bulls inspect repository-00000000000000000000000000000001
```

Repository IDs are stable machine identities.

### Known repository location

```bash
bulls inspect ~/Projects/example
```

Relative paths are resolved from the current working directory:

```bash
cd ~/Projects/example
bulls inspect .
```

If a path matches more than one repository, BullSaddle reports the ambiguity instead of selecting one silently.

Paths are selectors. They do not replace the repository's stable identity.

## Configuration

Use:

```bash
bulls config show
```

to determine the platform-resolved configuration path and effective values.

The current configuration schema is:

```toml
schema_version = 2
locale = "auto"

[discovery]
roots = ["/home/user/Projects"]
exclusions = ["vendor"]
bare_repositories = "disabled"
symlink_traversal = "do_not_follow"
filesystem_boundary = "stay_on_root_filesystem"

[observation]
max_parallelism = 4

[git]
process_timeout_ms = 10000
```

### Discovery

`roots`

Filesystem roots used when `bulls discover` is executed without explicit roots.

`exclusions`

Paths excluded from discovery traversal.

`bare_repositories`

```text
disabled
enabled
```

`symlink_traversal`

```text
do_not_follow
follow_within_root
```

`filesystem_boundary`

```text
stay_on_root_filesystem
cross_filesystems
```

### Observation

`max_parallelism`

Maximum observation concurrency.

The value must be greater than zero.

### Git process execution

`process_timeout_ms`

Maximum duration allowed for bounded Git subprocess execution.

The value must be greater than zero.

### Locale

Automatic locale selection:

```toml
locale = "auto"
```

Explicit locale:

```toml
locale = "pt-BR"
```

Supported locales are documented in [Localization](#localization).

Unknown configuration fields and unsupported schema versions are rejected rather than silently ignored.

## Structured output

The following commands support JSON:

```bash
bulls discover ~/Projects --format json
bulls refresh --format json
bulls repos --format json
bulls inspect . --format json
bulls overview --format json
```

Structured output is intended for scripts, integrations and future machine-facing adapters.

JSON contracts use stable semantic values rather than localized human-readable text.

Locale does not alter:

* field names;
* semantic codes;
* identifiers;
* schema versions;
* numeric values;
* protocol structure;
* exit semantics.

Operational JSON preserves completion state such as:

```text
complete
partial
failed
cancelled
```

Partial success is represented explicitly.

Consumers should use protocol fields and exit codes rather than parse human-readable output.

## Exit codes

BullSaddle uses explicit process exit codes:

|  Code | Meaning                                        |
| ----: | ---------------------------------------------- |
|   `0` | Success                                        |
|   `2` | Usage error                                    |
|   `3` | Expected failure or partial operational result |
|   `4` | Operational failure                            |
|  `70` | Internal failure                               |
| `130` | Interrupted                                    |

Scripts should treat exit status as part of the command contract.

## Localization

BullSaddle currently supports:

| Locale    | Language            |
| --------- | ------------------- |
| `en-US`   | English             |
| `pt-BR`   | Português do Brasil |
| `es`      | Español             |
| `de`      | Deutsch             |
| `ja`      | 日本語                 |
| `zh-Hans` | 简体中文                |

English remains the canonical language of:

* source code;
* identifiers;
* commands;
* flags;
* configuration keys;
* JSON fields;
* protocol values;
* semantic codes.

Localization applies only to human-facing presentation.

Examples:

```bash
LANG=pt_BR.UTF-8 bulls --help
LANG=de_DE.UTF-8 bulls overview
LANG=ja_JP.UTF-8 bulls repos
LANG=zh_CN.UTF-8 bulls inspect .
```

An explicit configuration value overrides automatic locale selection:

```toml
locale = "ja_JP"
```

Unsupported locales fall back to `en-US`.

Translation catalogs are validated and compiled into the executable. Supported locales must have complete catalogs; missing translations are build and release failures.

## Operational model

### Git remains authoritative

BullSaddle does not redefine Git state.

Git remains authoritative over:

* commits;
* branches;
* refs;
* HEAD;
* index;
* working trees;
* remotes;
* Git worktrees.

BullSaddle owns only BullSaddle state.

### Discovery and observation are separate

```text
discover
    find and reconcile repositories

refresh
    observe known repositories and worktrees

repos / inspect / overview
    query persisted BullSaddle knowledge
```

This separation prevents hidden filesystem traversal or Git execution behind read operations.

### No implicit network activity

Normal observation and query operations do not silently execute:

```text
fetch
pull
push
clone
```

Ahead/behind information reflects the upstream state currently known by the local Git repository.

It is not a claim about the current state of the remote server.

### Read-only toward Git repositories

Discovery, refresh and queries do not create or modify:

* commits;
* branches;
* refs;
* Git worktrees;
* stashes;
* index state;
* working-tree files.

BullSaddle may modify its own persistent metadata.

### Freshness is explicit

Persisted observations represent knowledge collected at a particular time.

BullSaddle distinguishes states such as:

```text
observed
partial
failed
never observed
unknown
```

Unknown does not mean false.

### Partial failure is isolated

A problem affecting one filesystem path, repository, worktree or Git process does not invalidate unrelated successful results.

## Platform support

The currently certified native release target is:

```text
x86_64-unknown-linux-gnu
```

A platform is considered supported only after its relevant runtime and distribution invariants have been validated natively.

Successful cross-compilation alone does not establish platform support.

## Build from source

BullSaddle uses the Rust toolchain pinned by `rust-toolchain.toml`.

Build the CLI:

```bash
cargo build --locked --profile dist -p bulls-cli --bin bulls
```

Run directly with Cargo:

```bash
cargo run --locked -p bulls-cli -- --help
```

With Nix:

```bash
nix build .#bulls
```

or:

```bash
nix run .#bulls -- --help
```

The workspace currently targets BullSaddle version `0.2.0`.

## Development and validation

The repository uses `just` as the primary development task interface.

### Standard quality gate

```bash
just quality
```

Runs:

* i18n catalog validation;
* i18n boundary audit;
* rustfmt check;
* Cargo check;
* Clippy with warnings denied;
* tests;
* unused dependency checks.

### Full repository verification

```bash
just verify
```

Extends validation with:

* foundation checks;
* Nix checks;
* quality checks;
* security and dependency policy checks.

### Release validation

```bash
just release-check
```

Validates the certified native distribution path, including representative CLI behavior, structured output, localization, reset behavior and source-tree purity.

### Individual commands

```bash
just fmt
just check
just lint
just test
just coverage
just audit
just deny
just unused
```

### Internationalization tooling

Validate all catalogs:

```bash
just i18n-check
```

Show translation coverage:

```bash
just i18n-coverage
```

Show missing entries for a locale:

```bash
just i18n-missing ja
```

Prepare a new locale:

```bash
just i18n-add fr
```

The internal `bulls-i18n` tool is development infrastructure and is not part of the public BullSaddle distribution.

## Acknowledgements

BullSaddle is built in an ecosystem shaped by decades of open-source work.

We acknowledge [Linus Torvalds](https://github.com/torvalds) for initiating the Linux kernel in 1991 and creating Git in 2005, and the communities of contributors who have continued to develop, maintain and strengthen both projects since then.

Linux and Git have become foundational tools of modern software development. BullSaddle exists on top of that foundation with respect for the responsibilities and boundaries of Git itself.

## License

BullSaddle is developed by [Orion Impact](https://orion-impact.com).

Licensed under the Apache License, Version 2.0.

See [`LICENSE`](LICENSE) for the complete license text.
