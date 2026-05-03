#!/usr/bin/env bash

set -euo pipefail

# Convert a plaintext log into a .tog file through the Rust workspace converter crate.

usage() {
    cat <<'EOF'
Usage:
  scripts/tinylog-convert.sh <input.log> [output.tog] [algorithmId] [trunkSizeKb]
  scripts/tinylog-convert.sh --reverse <input.tog> [output.log]

Examples:
  scripts/tinylog-convert.sh logs/normal.log
  scripts/tinylog-convert.sh logs/normal.log logs/normal.tog
  scripts/tinylog-convert.sh logs/normal.log logs/normal.tog 1 512
  scripts/tinylog-convert.sh --reverse logs/normal.tog
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
fi

if [[ "${1:-}" == "--reverse" ]]; then
    if [[ $# -lt 2 || $# -gt 3 ]]; then
        usage >&2
        exit 1
    fi

    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    repo_root="$(cd "${script_dir}/.." && pwd)"
    manifest_path="${repo_root}/tinylog-converter/Cargo.toml"
    command=(
        cargo
        run
        --quiet
        --manifest-path
        "${manifest_path}"
        --
        --reverse
        "$2"
    )
    if [[ $# -eq 3 ]]; then
        command+=("$3")
    fi
    exec "${command[@]}"
fi

if [[ $# -lt 1 || $# -gt 4 ]]; then
    usage >&2
    exit 1
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
manifest_path="${repo_root}/tinylog-converter/Cargo.toml"

input_path="$1"
output_path="${2:-}"

if [[ ! -f "${input_path}" ]]; then
    echo "input log does not exist: ${input_path}" >&2
    exit 1
fi

if [[ -z "${output_path}" ]]; then
    if [[ "${input_path}" == *.* ]]; then
        output_path="${input_path%.*}.tog"
    else
        output_path="${input_path}.tog"
    fi
fi

command=(
    cargo
    run
    --quiet
    --manifest-path
    "${manifest_path}"
    --
    "${input_path}"
    "${output_path}"
)

if [[ $# -ge 3 ]]; then
    command+=("$3")
fi

if [[ $# -ge 4 ]]; then
    command+=("$4")
fi

exec "${command[@]}"
