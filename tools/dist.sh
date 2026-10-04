#!/usr/bin/env sh
# BullSaddle — Developed by Orion Impact (https://orion-impact.com)
# Licensed under the Apache License, Version 2.0.
# SPDX-License-Identifier: Apache-2.0

set -eu

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

required_version=$(sed -n 's/^cargo-dist-version = "\([^"]*\)"$/\1/p' dist-workspace.toml)

if [ -z "$required_version" ]; then
    printf '%s\n' 'Unable to determine the pinned dist version.' >&2
    exit 1
fi

if ! command -v dist >/dev/null 2>&1; then
    printf 'dist %s is required for release tooling.\n' "$required_version" >&2
    exit 1
fi

actual_version=$(dist --version | awk '{print $NF}')

if [ "$actual_version" != "$required_version" ]; then
    printf 'Expected dist %s, found %s.\n' "$required_version" "$actual_version" >&2
    exit 1
fi

exec dist "$@"
