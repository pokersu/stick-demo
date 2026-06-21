#!/usr/bin/env bash
set -euo pipefail

PROFILE="${1:-release}"
CHIP="esp32s3"
ELF_DIR="target/xtensa-esp32s3-espidf/${PROFILE}"
ELF="${ELF_DIR}/stick-demo"
BIN="${ELF_DIR}/stick-demo.bin"

case "$PROFILE" in
    release) CARGO_CMD="cargo build --release" ;;
    debug)   CARGO_CMD="cargo build" ;;
    *)       echo "Usage: $0 [release|debug]"; exit 1 ;;
esac

IMAGE="espressif/idf-rust:esp32s3_1.93.0.0"

# 缓存目录
REGISTRY_CACHE="./registry-cache"
ESPRESSIF_DIR="./.espressif"
mkdir -p "${REGISTRY_CACHE}" "${ESPRESSIF_DIR}" ./git-cache

echo "=> Image: ${IMAGE}"
echo "=> Profile: ${PROFILE}"
docker pull "${IMAGE}" 2>&1 | tail -1

echo "=> Building..."

docker run --rm \
    -v "$(pwd)":/project \
    -v "$(pwd)/target":/project/target \
    -v "${HOME}/.espressif":/root/.espressif \
    -v "$(pwd)/registry-cache":/home/esp/.cargo/registry \
    -v "$(pwd)/git-cache":/home/esp/.cargo/git \
    -w /project \
    -e IDF_PATH=/project/.embuild/espressif/esp-idf/v5.2.3 \
    "${IMAGE}" \
    sh -c "${CARGO_CMD} && espflash save-image --chip ${CHIP} /project/${ELF} /project/${BIN}"

echo ""
echo "============================================"
echo "Build complete."
echo "Firmware: ${BIN}"
echo "ELF:      ${ELF}"
echo ""
echo "Flash:  espflash flash --baud=921600 ${ELF}"
echo "============================================"
