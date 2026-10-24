# Contributing to MeowServer

Thank you for helping. Please read this page before opening a pull request.

## Before you start

- Read the product requirements in [`docs/PRD.md`](docs/PRD.md). Features
  outside its scope (§5.3) need a discussion first.
- For anything bigger than a small fix, open an issue to agree on the approach
  before writing code.
- Be kind. See [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).

## Sign your commits (DCO)

MeowServer uses the [Developer Certificate of Origin](https://developercertificate.org/).
By signing off a commit you state that you wrote the code, or otherwise have
the right to contribute it under the project's license (GPL-3.0-or-later with
the additional terms in `NOTICE`).

Sign off with `-s`:

```
git commit -s -m "Fix port detection on Windows"
```

which adds a line like:

```
Signed-off-by: Your Name <you@example.com>
```

Pull requests with unsigned commits fail CI. To sign off commits you already
made: `git rebase --signoff main`.

You keep the copyright to your contribution.

## Development

Requirements: Rust stable (see `rust-version` in `Cargo.toml`). On Windows,
the MSVC toolchain.

```
cargo build
cargo test
cargo clippy --all-targets
cargo fmt
```

## Code rules

These come from the PRD (§11) and are checked in review:

- `crates/core` holds all logic. `cli`, `daemon`, and `app` only call it.
- `crates/daemon` must not depend on a GUI toolkit.
- OS-specific code (`#[cfg(target_os = …)]`) lives only in
  `crates/core/src/platform/`.
- Service configs are templates in `templates/`, not strings in Rust code.
- English for code, comments, commit messages, and docs.
- Every source file starts with the license header:

  ```rust
  // SPDX-License-Identifier: GPL-3.0-or-later
  // Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
  // Additional terms under GPLv3 section 7(b) apply; see NOTICE.
  ```

## Translations

GUI texts are written in English in `crates/app/ui/*.slint` with `@tr("…")`.
Translations are gettext files in `locales/<lang>/LC_MESSAGES/meow-app.po`
and are compiled into the binary. To add a language, copy
`locales/id/LC_MESSAGES/meow-app.po` to your language code, translate every
`msgstr`, and open a pull request. A handful of status texts built in Rust are
in `crates/app/src/tr.rs`.

## Code from other projects

Do not copy code from projects without a license or with a license
incompatible with GPL-3.0 (for example Laragon). Code adapted from MIT or BSD
projects is allowed: add its copyright notice and license text to `NOTICE` in
the same pull request.
