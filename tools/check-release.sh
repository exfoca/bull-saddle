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

require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        fail "Required release command is unavailable: $1"
    fi
}

require_cargo_subcommand() {
    if ! cargo "$1" --version >/dev/null 2>&1; then
        fail "Required Cargo subcommand is unavailable: cargo $1"
    fi
}

require_dist_setting() {
    if ! grep -Fqx "$1" dist-workspace.toml; then
        fail "Required distribution setting is missing: $1"
    fi
}

verify_sha256() {
    verify_archive=$1
    verify_checksum=$2
    expected=$(awk 'NF { print $1; exit }' "$verify_checksum" | tr 'A-F' 'a-f')

    case "$expected" in
        '' | *[!0-9A-Fa-f]*) fail "Invalid SHA-256 checksum file: $verify_checksum" ;;
    esac
    if [ "${#expected}" -ne 64 ]; then
        fail "Invalid SHA-256 checksum length: $verify_checksum"
    fi

    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$verify_archive" | awk '{ print $1 }')
    elif command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$verify_archive" | awk '{ print $1 }')
    else
        fail 'A SHA-256 implementation is required: sha256sum or shasum.'
    fi

    if [ "$actual" != "$expected" ]; then
        fail "Distribution artifact checksum mismatch: $verify_archive"
    fi
}

extract_archive() {
    extract_archive_path=$1
    extract_destination=$2

    case "$extract_archive_path" in
        *.zip)
            require_command unzip
            unzip -q "$extract_archive_path" -d "$extract_destination"
            ;;
        *.tar | *.tar.gz | *.tgz | *.tar.xz | *.tar.bz2 | *.tar.zst)
            require_command tar
            tar -xf "$extract_archive_path" -C "$extract_destination"
            ;;
        *)
            fail "Unsupported distribution archive format: $extract_archive_path"
            ;;
    esac
}

initial_status=$(git status --short --untracked-files=all)
if [ -n "$initial_status" ]; then
    printf '%s\n%s\n' 'Release verification requires a clean source tree:' "$initial_status" >&2
    exit 1
fi

for command in awk cargo cat cmp cp env find git grep jq just mv rustc sed tr wc; do
    require_command "$command"
done
for subcommand in audit auditable cyclonedx; do
    require_cargo_subcommand "$subcommand"
done

host_target=$(rustc -vV | sed -n 's/^host: //p')
if [ -z "$host_target" ]; then
    fail 'Unable to determine the native Rust target.'
fi

configured_targets=$(
    sed -n '
        /^targets = \[$/,/^\]$/ {
            s/^[[:space:]]*"\([^"]*\)",*[[:space:]]*$/\1/p
        }
    ' dist-workspace.toml
)
configured_target_count=$(
    printf '%s\n' "$configured_targets" | sed '/^$/d' | wc -l | tr -d '[:space:]'
)

if [ "$configured_target_count" -ne 1 ]; then
    fail "Release verification requires exactly one natively certified distribution target; found $configured_target_count."
fi

certified_target=$(printf '%s\n' "$configured_targets" | sed -n '1p')
if [ "$host_target" != "$certified_target" ]; then
    fail "Native target $host_target is not the certified release target: $certified_target"
fi

require_dist_setting 'checksum = "sha256"'
require_dist_setting 'cargo-auditable = true'
require_dist_setting 'cargo-cyclonedx = true'

scratch=$(mktemp -d "${TMPDIR:-/tmp}/bulls-release.XXXXXX")
cleanup() {
    rm -rf "$scratch"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

printf 'Certifying native release target: %s\n' "$host_target"
./tools/dist.sh plan >/dev/null
just verify
./tools/dist.sh build \
    --artifacts=host \
    --target="$host_target" \
    --output-format=json >"$scratch/dist-manifest.json"

cargo cyclonedx -v
find . \
    -path './target' -prune -o \
    -type f -name '*.cdx.xml' \
    -exec mv '{}' target/distrib/ ';'

archive=$(jq -er --arg target "$host_target" '
    [
        .artifacts[]
        | select(.kind == "executable-zip")
        | select((.target_triples // []) | index($target))
        | .path
        | select(type == "string")
    ]
    | if length == 1 then .[0] else error("expected exactly one native executable archive") end
' "$scratch/dist-manifest.json") || fail 'Unable to identify exactly one native executable archive.'

if [ ! -f "$archive" ]; then
    fail "Distribution manifest points to a missing native archive: $archive"
fi
jq -e --arg archive "$archive" '
    (.upload_files | type == "array")
    and (.upload_files | index($archive) != null)
' "$scratch/dist-manifest.json" >/dev/null || fail 'Native archive is not present in dist upload files.'

checksum="${archive}.sha256"
if [ ! -f "$checksum" ]; then
    fail "Native archive checksum was not produced: $checksum"
fi
jq -e --arg checksum "$checksum" '
    .upload_files | index($checksum) != null
' "$scratch/dist-manifest.json" >/dev/null || fail 'Native checksum is not present in dist upload files.'
verify_sha256 "$archive" "$checksum"

extracted="$scratch/extracted"
mkdir -p "$extracted"
extract_archive "$archive" "$extracted"

license_list="$scratch/licenses.txt"
find "$extracted" -type f -name 'LICENSE' -print >"$license_list"
license_count=$(wc -l <"$license_list" | tr -d '[:space:]')
if [ "$license_count" -ne 1 ]; then
    fail "Expected exactly one LICENSE file in the native archive, found $license_count."
fi
distributed_license=$(sed -n '1p' "$license_list")
if ! cmp -s LICENSE "$distributed_license"; then
    fail "Native archive LICENSE does not match the repository LICENSE: $distributed_license"
fi

binary_name=bulls
case "$host_target" in
    *-windows-*) binary_name=bulls.exe ;;
esac

binary_list="$scratch/binaries.txt"
find "$extracted" -type f -name "$binary_name" -print >"$binary_list"
binary_count=$(wc -l <"$binary_list" | tr -d '[:space:]')
if [ "$binary_count" -ne 1 ]; then
    fail "Expected exactly one $binary_name executable in the native archive, found $binary_count."
fi
binary=$(sed -n '1p' "$binary_list")
if [ ! -x "$binary" ]; then
    fail "Native archive does not contain an executable BullSaddle binary: $binary"
fi

internal_tool_list="$scratch/internal-tools.txt"
find "$extracted" -type f \
    \( -name 'bulls-i18n' -o -name 'bulls-i18n.exe' \) \
    -print >"$internal_tool_list"
if [ -s "$internal_tool_list" ]; then
    fail 'Native archive unexpectedly contains the internal bulls-i18n tool.'
fi

external_catalog_list="$scratch/external-i18n-catalogs.txt"
find "$extracted" -type f \
    \( -name 'manifest.json' -o -name 'messages.json' \
       -o -name 'en-US.json' -o -name 'pt-BR.json' -o -name 'es.json' \
       -o -name 'de.json' -o -name 'ja.json' -o -name 'zh-Hans.json' \) \
    -print >"$external_catalog_list"
if [ -s "$external_catalog_list" ]; then
    fail 'Native archive unexpectedly contains external i18n catalogs.'
fi

sbom_list="$scratch/sboms.txt"
jq -er '
    [
        .artifacts[]
        | select(.kind == "sbom")
        | .path
        | select(type == "string")
    ]
    | if length >= 1 then .[] else error("expected at least one CycloneDX SBOM artifact") end
' "$scratch/dist-manifest.json" >"$sbom_list" \
    || fail 'Unable to identify a CycloneDX SBOM in dist artifacts.'
while IFS= read -r sbom; do
    if [ ! -f "$sbom" ]; then
        fail "Distribution manifest points to a missing CycloneDX SBOM: $sbom"
    fi
    jq -e --arg sbom "$sbom" '
        .upload_files | index($sbom) != null
    ' "$scratch/dist-manifest.json" >/dev/null \
        || fail "CycloneDX SBOM is not present in dist upload files: $sbom"
    if [ ! -s "$sbom" ] || ! grep -q '<bom' "$sbom"; then
        fail "Invalid CycloneDX XML SBOM: $sbom"
    fi
done <"$sbom_list"

cargo audit bin "$binary"

home="$scratch/home"
config="$scratch/config"
data="$scratch/data"
state="$scratch/state"
cache="$scratch/cache"
app_data="$scratch/appdata"
local_app_data="$scratch/local-appdata"
user_profile="$scratch/profile"
workspace="$scratch/workspace"
repository="$workspace/repository"

mkdir -p \
    "$home" "$config" "$data" "$state" "$cache" \
    "$app_data" "$local_app_data" "$user_profile" "$repository"
git -C "$repository" init -q

run_bulls_locale() {
    locale=$1
    shift

    env \
        HOME="$home" \
        XDG_CONFIG_HOME="$config" \
        XDG_DATA_HOME="$data" \
        XDG_STATE_HOME="$state" \
        XDG_CACHE_HOME="$cache" \
        APPDATA="$app_data" \
        LOCALAPPDATA="$local_app_data" \
        USERPROFILE="$user_profile" \
        LC_ALL="$locale" \
        LC_MESSAGES= \
        LANG= \
        "$binary" "$@"
}

run_bulls() {
    run_bulls_locale C "$@"
}

project_metadata="$scratch/cargo-metadata.json"
cargo metadata --locked --no-deps --format-version 1 >"$project_metadata"
project_version=$(jq -r '.packages[] | select(.name == "bulls-cli") | .version' "$project_metadata")
project_license=$(jq -r '.packages[] | select(.name == "bulls-cli") | .license' "$project_metadata")

jq -e '
    [.packages[] | select(.name == "bulls-i18n")] as $packages
    | ($packages | length) == 1
    and ($packages[0].publish == [])
    and (($packages[0].metadata.dist.dist // false) == false)
' "$project_metadata" >/dev/null \
    || fail 'bulls-i18n must remain an internal, non-distributed workspace tool.'

if [ -z "$project_version" ] || [ "$project_version" = "null" ]; then
    fail 'Unable to determine the BullSaddle package version.'
fi
if [ "$project_license" != "Apache-2.0" ]; then
    fail "Distributed package must declare the Apache-2.0 license, found: $project_license"
fi

version_output=$(run_bulls --version)
if [ "$version_output" != "bulls $project_version" ]; then
    fail 'Distributed executable reported an unexpected version.'
fi

smoke_locale() {
    locale=$1
    expected_usage=$2
    expected_error=$3

    help_output="$scratch/help-$locale.txt"
    error_output="$scratch/error-$locale.txt"

    run_bulls_locale "$locale" --help >"$help_output"
    grep -Fxq "$expected_usage:" "$help_output" \
        || fail "Localized help heading smoke check failed for locale: $locale"
    grep -Fxq '  bulls <command>' "$help_output" \
        || fail "Canonical help syntax smoke check failed for locale: $locale"

    if run_bulls_locale "$locale" unknown >"$scratch/error-$locale.stdout" 2>"$error_output"; then
        locale_error_status=0
    else
        locale_error_status=$?
    fi
    if [ "$locale_error_status" -ne 2 ]; then
        fail "Localized parsing error returned unexpected status for locale $locale: $locale_error_status"
    fi
    grep -Fq "$expected_error" "$error_output" \
        || fail "Localized parsing error smoke check failed for locale: $locale"
}

smoke_locale en_US.UTF-8 'Usage' 'Unknown command: unknown'
smoke_locale pt_BR.UTF-8 'Uso' 'Comando desconhecido: unknown'
smoke_locale es_MX.UTF-8 'Uso' 'Comando desconocido: unknown'
smoke_locale de_DE.UTF-8 'Verwendung' 'Unbekannter Befehl: unknown'
smoke_locale ja_JP.UTF-8 '使用方法' '不明なコマンド: unknown'
smoke_locale zh_CN.UTF-8 '用法' '未知命令: unknown'

mkdir -p "$config/bulls"
configuration_path="$config/bulls/config.toml"
cat >"$configuration_path" <<'EOF'
schema_version = 2
locale = "auto"

[discovery]
roots = []
exclusions = []
bare_repositories = "disabled"
symlink_traversal = "do_not_follow"
filesystem_boundary = "stay_on_root_filesystem"

[observation]
max_parallelism = 2

[git]
process_timeout_ms = 10000
EOF
cp "$configuration_path" "$scratch/config-before-reset.toml"

run_bulls config show >"$scratch/config-show.txt"
grep -Fq 'Source: file' "$scratch/config-show.txt" \
    || fail 'Configuration inspection did not report the persisted configuration source.'
grep -Fq 'Observation parallelism: 2' "$scratch/config-show.txt" \
    || fail 'Configuration inspection did not expose the effective observation policy.'
grep -Fq 'Git process timeout: 10000 ms' "$scratch/config-show.txt" \
    || fail 'Configuration inspection did not expose the effective Git timeout.'

run_bulls discover "$workspace" --format json >"$scratch/discover.json"
jq -e '
    .schema_version == 1
    and .status == "complete"
    and (.workspace_revision | type == "number")
    and .payload.requested_root_count == 1
    and .payload.completed_root_count == 1
    and .payload.failed_root_count == 0
    and .payload.repository_count == 1
    and .payload.worktree_count == 1
' "$scratch/discover.json" >/dev/null || fail 'Discovery JSON smoke check failed.'

run_bulls repos --format json >"$scratch/repos-before-refresh.json"
jq -e '
    .schema_version == 1
    and (.workspace_revision | type == "number")
    and (.payload.items | length == 1)
' "$scratch/repos-before-refresh.json" >/dev/null \
    || fail 'Repository JSON smoke check failed before refresh.'

repository_id=$(jq -r '.payload.items[0].id' "$scratch/repos-before-refresh.json")
if [ -z "$repository_id" ] || [ "$repository_id" = "null" ]; then
    fail 'Repository JSON did not expose an inspectable repository ID.'
fi

run_bulls inspect "$repository_id" >"$scratch/inspect-before-refresh.txt"
grep -Fq 'never observed' "$scratch/inspect-before-refresh.txt" \
    || fail 'Human inspection did not expose never-observed freshness before refresh.'
if grep -Fq 'Workspace revision:' "$scratch/inspect-before-refresh.txt"; then
    fail 'Human inspection exposed workspace revision in default output.'
fi

missing_root="$scratch/missing-root"
if run_bulls discover "$workspace" "$missing_root" --format json >"$scratch/discover-partial.json"; then
    partial_discovery_status=0
else
    partial_discovery_status=$?
fi
if [ "$partial_discovery_status" -ne 3 ]; then
    fail "Partial discovery returned unexpected exit status: $partial_discovery_status"
fi
jq -e '
    .schema_version == 1
    and .status == "partial"
    and .payload.requested_root_count == 2
    and .payload.completed_root_count == 1
    and .payload.failed_root_count == 1
    and (.payload.failed_roots | length == 1)
    and (.payload.failed_roots[0].cause.kind | type == "string")
' "$scratch/discover-partial.json" >/dev/null \
    || fail 'Partial discovery protocol smoke check failed.'

run_bulls refresh --format json >"$scratch/refresh.json"
jq -e '
    .schema_version == 1
    and .status == "complete"
    and (.workspace_revision | type == "number")
    and .payload.coverage == "complete"
    and .payload.target_count == 2
    and .payload.succeeded_count == 2
    and .payload.failed_count == 0
    and .payload.cancelled_count == 0
    and (.payload.failures | length == 0)
' "$scratch/refresh.json" >/dev/null || fail 'Refresh JSON smoke check failed.'

run_bulls inspect "$repository_id" --format json >"$scratch/inspect-by-id.json"
run_bulls inspect "$repository" --format json >"$scratch/inspect-by-path.json"
(
    cd "$repository"
    run_bulls inspect . --format json >"$scratch/inspect-by-current-directory.json"
)

for inspection in \
    "$scratch/inspect-by-id.json" \
    "$scratch/inspect-by-path.json" \
    "$scratch/inspect-by-current-directory.json"
do
    jq -e --arg repository_id "$repository_id" '
        .schema_version == 1
        and .payload.repository.id == $repository_id
        and (.payload.advisories.advisories | type == "array")
    ' "$inspection" >/dev/null || fail "Repository selector smoke check failed: $inspection"
done

run_bulls overview --format json >"$scratch/overview-before-reset.json"
jq -e '
    .schema_version == 1
    and .payload.overview.repository_count == 1
    and (.payload.advisories.advisories | type == "array")
' "$scratch/overview-before-reset.json" >/dev/null || fail 'Overview JSON smoke check failed.'
revision_before_reset=$(jq -r '.workspace_revision' "$scratch/overview-before-reset.json")

run_bulls reset >"$scratch/reset.txt"
grep -Fq 'Status: complete' "$scratch/reset.txt" || fail 'Workspace reset did not complete.'
grep -Fq 'User configuration was preserved.' "$scratch/reset.txt" \
    || fail 'Workspace reset did not report configuration preservation.'
if ! cmp -s "$scratch/config-before-reset.toml" "$configuration_path"; then
    fail 'Workspace reset modified user configuration.'
fi
if ! git -C "$repository" rev-parse --git-dir >/dev/null 2>&1; then
    fail 'Workspace reset modified or removed the observed Git repository.'
fi

run_bulls overview --format json >"$scratch/overview-after-reset.json"
jq -e '
    .payload.overview.repository_count == 0
    and .payload.overview.worktree_count == 0
' "$scratch/overview-after-reset.json" >/dev/null || fail 'Workspace reset did not clear workspace knowledge.'
revision_after_reset=$(jq -r '.workspace_revision' "$scratch/overview-after-reset.json")
if [ "$revision_after_reset" -le "$revision_before_reset" ]; then
    fail 'Workspace reset did not advance the workspace revision.'
fi

run_bulls reset >"$scratch/reset-idempotent.txt"
run_bulls overview --format json >"$scratch/overview-after-idempotent-reset.json"
revision_after_idempotent_reset=$(jq -r '.workspace_revision' "$scratch/overview-after-idempotent-reset.json")
if [ "$revision_after_idempotent_reset" -ne "$revision_after_reset" ]; then
    fail 'Idempotent workspace reset changed the workspace revision.'
fi
if ! cmp -s "$scratch/config-before-reset.toml" "$configuration_path"; then
    fail 'Idempotent workspace reset modified user configuration.'
fi

./tools/check-foundation.sh

final_status=$(git status --short --untracked-files=all)
if [ "$final_status" != "$initial_status" ]; then
    printf '%s\n%s\n' 'Normal release execution changed the source tree:' "$final_status" >&2
    exit 1
fi

printf 'Release verification passed for certified native target %s.\n' "$host_target"
