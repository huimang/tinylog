#!/usr/bin/env bash

set -euo pipefail

# Convert a plaintext log into .tog and immediately open it in the viewer.

usage() {
    cat <<'EOF'
Usage:
  scripts/tinylog-open.sh <input.log> [output.tog] [algorithmId] [trunkSizeKb]

Examples:
  scripts/tinylog-open.sh logs/normal.log
  scripts/tinylog-open.sh logs/normal.log logs/normal.tog
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
fi

if [[ $# -lt 1 || $# -gt 4 ]]; then
    usage >&2
    exit 1
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
convert_script="${script_dir}/tinylog-convert.sh"
view_script="${script_dir}/tinylog-view.sh"

input_path="$1"
output_path="${2:-}"

if [[ -z "${output_path}" ]]; then
    if [[ "${input_path}" == *.* ]]; then
        output_path="${input_path%.*}.tog"
    else
        output_path="${input_path}.tog"
    fi
fi

"${convert_script}" "$@"
exec "${view_script}" "${output_path}"
