#!/usr/bin/env bash
set -euo pipefail

readonly VERSION="b11429"
readonly COMMIT="d81235049384534c167caea52b85a694f6103d14"
readonly BASE_URL="https://github.com/ggml-org/llama.cpp/releases/download/${VERSION}"
readonly CPU_ARCHIVE="llama-${VERSION}-bin-ubuntu-x64.tar.gz"
readonly VULKAN_ARCHIVE="llama-${VERSION}-bin-ubuntu-vulkan-x64.tar.gz"
readonly CPU_SHA="f6d25dde8f51133143d1453da4fd5f73b145127177612a283bf7995957af3392"
readonly VULKAN_SHA="632c4e98feba2b94407a2130e3133e0c3aefb0ea1ab41337e926d8bfafdd0b74"
readonly ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly DEST="${ROOT}/src-tauri/binaries/runtime/linux/x86_64"
readonly CACHE="${TMPDIR:-/tmp}/gravityforge-llama-${VERSION}"

mkdir -p "${CACHE}" "${DEST}"

fetch() {
  local archive="$1" expected="$2"
  if [[ ! -f "${CACHE}/${archive}" ]]; then
    curl --fail --location --retry 3 --output "${CACHE}/${archive}" "${BASE_URL}/${archive}"
  fi
  printf '%s  %s\n' "${expected}" "${CACHE}/${archive}" | sha256sum --check --status
}

install_profile() {
  local backend="$1" archive="$2"
  local unpack="${CACHE}/${backend}"
  local target="${DEST}/${backend}"
  mkdir -p "${unpack}" "${target}"
  tar -xzf "${CACHE}/${archive}" -C "${unpack}"
  find "${target}" -mindepth 1 -maxdepth 1 -delete
  cp -a "${unpack}/llama-${VERSION}/llama-server" "${target}/"
  cp -a "${unpack}/llama-${VERSION}/LICENSE" "${target}/LLAMA_CPP_LICENSE"
  cp -a "${unpack}/llama-${VERSION}"/libggml*.so* "${target}/"
  cp -a "${unpack}/llama-${VERSION}"/libllama.so* "${target}/"
  cp -a "${unpack}/llama-${VERSION}"/libllama-common.so* "${target}/"
  cp -a "${unpack}/llama-${VERSION}"/libllama-server-impl.so "${target}/"
  cp -a "${unpack}/llama-${VERSION}"/libmtmd.so* "${target}/"
  chmod 0755 "${target}/llama-server"
}

fetch "${CPU_ARCHIVE}" "${CPU_SHA}"
fetch "${VULKAN_ARCHIVE}" "${VULKAN_SHA}"
install_profile cpu "${CPU_ARCHIVE}"
install_profile vulkan "${VULKAN_ARCHIVE}"

printf 'Prepared llama.cpp %s (%s) CPU and Vulkan runtimes.\n' "${VERSION}" "${COMMIT}"
