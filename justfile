# BullSaddle — Developed by Orion Impact (https://orion-impact.com)
# Licensed under the Apache License, Version 2.0.
# SPDX-License-Identifier: Apache-2.0

set dotenv-load := false

default:
    @just --list

check:
    cargo check --locked --workspace --all-targets

build:
    cargo build --locked --workspace

build-release:
    cargo build --locked --workspace --release

run *args:
    cargo run --locked -p bulls-cli -- {{args}}

package:
    nix build .#bulls

package-run *args:
    nix run .#bulls -- {{args}}

dist-plan:
    ./tools/dist.sh plan

build-dist:
    cargo build --locked --profile dist -p bulls-cli --bin bulls

release-check:
    ./tools/check-release.sh

i18n-check:
    cargo run --locked -p bulls-i18n -- --root crates/bulls-cli/i18n check

i18n-audit:
    ./tools/check-i18n-boundary.sh

i18n-coverage:
    cargo run --locked -p bulls-i18n -- --root crates/bulls-cli/i18n coverage

i18n-missing locale:
    cargo run --locked -p bulls-i18n -- --root crates/bulls-cli/i18n missing {{locale}}

i18n-add locale:
    cargo run --locked -p bulls-i18n -- --root crates/bulls-cli/i18n add-locale {{locale}}

fmt:
    cargo fmt --all -- --check

fmt-fix:
    cargo fmt --all

lint:
    cargo clippy --locked --workspace --all-targets --all-features -- -D warnings

test:
    cargo nextest run --locked --workspace --all-features
    cargo test --locked --workspace --all-features --doc

coverage:
    cargo llvm-cov nextest --locked --workspace --all-features

audit:
    cargo audit

deny:
    cargo deny check

unused:
    cargo machete

outdated:
    cargo outdated --workspace

foundation:
    ./tools/check-foundation.sh

nix-check:
    nixfmt --check flake.nix
    statix check flake.nix
    deadnix --fail flake.nix
    nix flake check --no-update-lock-file

quality: i18n-check i18n-audit fmt check lint test unused

security: audit deny

verify: foundation nix-check quality security
