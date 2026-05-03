#!/usr/bin/env bash

set -euo pipefail

# Build the Rust release binaries and package them into a versioned archive.

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"

workspace_version="$(awk -F'"' '/^version = "/{print $2; exit}' "${repo_root}/Cargo.toml")"
pom_version="$(sed -n 's:.*<version>\(.*\)</version>.*:\1:p' "${repo_root}/pom.xml" | head -n 1)"

if [[ -z "${workspace_version}" || -z "${pom_version}" ]]; then
    echo "failed to detect project version" >&2
    exit 1
fi

if [[ "${workspace_version}" != "${pom_version}" ]]; then
    echo "workspace version (${workspace_version}) does not match pom version (${pom_version})" >&2
    exit 1
fi

version="${workspace_version}"
tag="v${version}"

os_name="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch_name="$(uname -m)"
case "${arch_name}" in
    aarch64|arm64)
        arch_name="arm64"
        ;;
    x86_64|amd64)
        arch_name="amd64"
        ;;
esac

package_name="tinylog-${tag}-${os_name}-${arch_name}"
package_root="${repo_root}/dist/${tag}"
package_dir="${package_root}/${package_name}"
archive_path="${package_root}/${package_name}.tar.gz"

rm -rf "${package_dir}"
mkdir -p "${package_dir}/bin"

cargo build --release -q -p tinylog-converter -p tinylog-viewer

cp "${repo_root}/target/release/tinylog-converter" "${package_dir}/bin/"
cp "${repo_root}/target/release/tinylog-viewer" "${package_dir}/bin/"
cp "${repo_root}/LICENSE" "${package_dir}/"
cp "${repo_root}/README.md" "${package_dir}/"

rm -f "${archive_path}"
tar -C "${package_root}" -czf "${archive_path}" "${package_name}"

printf 'packaged %s\n' "${archive_path}"
