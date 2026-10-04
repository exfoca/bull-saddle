#!/usr/bin/env sh
# BullSaddle — Developed by Orion Impact (https://orion-impact.com)
# Licensed under the Apache License, Version 2.0.
# SPDX-License-Identifier: Apache-2.0

set -eu

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

if [ ! -f LICENSE ] || ! grep -Fq 'Apache License' LICENSE || ! grep -Fq 'Version 2.0, January 2004' LICENSE; then
    printf '%s\n' 'BullSaddle Apache License 2.0 file is missing or invalid.' >&2
    exit 1
fi

license_header='SPDX-License-Identifier: Apache-2.0'
i18n_json_without_comment_headers='^crates/bulls-cli/i18n/(manifest\.json|messages\.json|locales/[^/]+\.json)$'
missing_license_headers=$(
    git grep -L -I -F "$license_header" -- . ':!flake.lock' ':!LICENSE' \
        | grep -Ev "$i18n_json_without_comment_headers" \
        || true
)

if [ -n "$missing_license_headers" ]; then
    printf '%s\n%s\n' 'BullSaddle license header is missing from tracked text files:' "$missing_license_headers" >&2
    exit 1
fi

metadata=$(cargo metadata --locked --format-version 1)

if ! printf '%s\n' "$metadata" | jq -e '
    def internal_deps($name):
        [.packages[]
         | select(.name == $name)
         | .dependencies[].name
         | select(startswith("bulls-"))];
    def dependency_kinds($package; $dependency):
        [.packages[]
         | select(.name == $package)
         | .dependencies[]
         | select(.name == $dependency)
         | .kind];
    def only($actual; $allowed):
        (($actual - $allowed) | length) == 0;

    ([.packages[].name | select(startswith("bulls-"))] | sort)
        == ["bulls-application", "bulls-cli", "bulls-core", "bulls-i18n", "bulls-infra", "bulls-runtime"]
    and only(internal_deps("bulls-core"); [])
    and only(internal_deps("bulls-application"); ["bulls-core"])
    and only(internal_deps("bulls-infra"); ["bulls-application", "bulls-core"])
    and only(internal_deps("bulls-runtime"); ["bulls-application", "bulls-infra"])
    and only(internal_deps("bulls-cli"); ["bulls-application", "bulls-runtime", "bulls-i18n"])
    and dependency_kinds("bulls-cli"; "bulls-i18n") == ["build"]
    and internal_deps("bulls-i18n") == []
' >/dev/null; then
    printf '%s\n' 'BullSaddle workspace dependency boundaries are invalid.' >&2
    exit 1
fi

for path in \
    .bulls/state \
    bulls.sqlite \
    bulls.sqlite-wal \
    bulls.sqlite-shm \
    inventory.bulls.idx \
    bulls.log
do
    if git check-ignore --no-index -q "$path"; then
        printf 'BullSaddle runtime state must not be hidden by .gitignore: %s\n' "$path" >&2
        exit 1
    fi
done

violations=$(
    find . \
        \( -path './.git' -o -path './target' -o -path './.direnv' -o -path './node_modules' -o -path './coverage' -o -path './test-results' \) -prune -o \
        \( -type f \( \
            -name '*.sqlite' -o \
            -name '*.sqlite-journal' -o \
            -name '*.sqlite-wal' -o \
            -name '*.sqlite-shm' -o \
            -name '*.sqlite3' -o \
            -name '*.sqlite3-journal' -o \
            -name '*.sqlite3-wal' -o \
            -name '*.sqlite3-shm' -o \
            -name '*.db' -o \
            -name '*.db-journal' -o \
            -name '*.db-wal' -o \
            -name '*.db-shm' -o \
            -name '*.bulls.idx' -o \
            -name 'bulls.log' \
        \) -o -type d -name '.bulls' \) -print
)

if [ -n "$violations" ]; then
    printf '%s\n%s\n' 'BullSaddle runtime state detected inside the source tree:' "$violations" >&2
    exit 1
fi
