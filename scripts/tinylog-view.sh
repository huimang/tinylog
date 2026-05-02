#!/usr/bin/env bash

set -euo pipefail

# Open a .tog file through the Rust workspace viewer crate.

usage() {
    cat <<'EOF'
Usage:
  scripts/tinylog-view.sh <input.tog>

Example:
  scripts/tinylog-view.sh logs/normal.tog
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
fi

if [[ $# -ne 1 ]]; then
    usage >&2
    exit 1
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
manifest_path="${repo_root}/tinylog-viewer/Cargo.toml"
input_path="$1"

if [[ ! -f "${input_path}" ]]; then
    echo "input TinyLog file does not exist: ${input_path}" >&2
    exit 1
fi

exec cargo run --quiet --manifest-path "${manifest_path}" -- "${input_path}"
