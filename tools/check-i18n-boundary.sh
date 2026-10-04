#!/usr/bin/env sh
# BullSaddle — Developed by Orion Impact (https://orion-impact.com)
# Licensed under the Apache License, Version 2.0.
# SPDX-License-Identifier: Apache-2.0

set -eu

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

fail() {
    printf '%s\n' "$1" >&2
    exit 1
}

source_root=crates/bulls-cli/src

check_forbidden() {
    pattern=$1
    description=$2

    matches=$(grep -R -n -E \
        --include='*.rs' \
        --exclude-dir=i18n \
        "$pattern" \
        "$source_root" || true)
    if [ -n "$matches" ]; then
        printf '%s\n%s\n' "$description" "$matches" >&2
        exit 1
    fi
}

check_forbidden \
    'Locale::[[:alnum:]_]+' \
    'Concrete locale variants must remain inside the i18n infrastructure:'
check_forbidden \
    '\.select[[:space:]]*\(' \
    'Legacy locale selection helpers must not reappear outside the i18n infrastructure:'
check_forbidden \
    'match[[:space:]]+locale([[:space:]]|\{)' \
    'Locale-specific branching must not reappear outside the i18n infrastructure:'
check_forbidden \
    '"(LC_ALL|LC_MESSAGES|LANG)"' \
    'Locale environment variables must be read only by the i18n infrastructure:'

if [ ! -f crates/bulls-cli/i18n/manifest.json ] \
    || [ ! -f crates/bulls-cli/i18n/messages.json ]; then
    fail 'Declarative i18n contracts are missing from bulls-cli.'
fi

printf '%s\n' 'i18n boundary check: OK'
