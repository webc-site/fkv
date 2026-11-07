#!/usr/bin/env bash
# 用 zigbuild 交叉编译 Linux 测试二进制，然后放进最小 Debian 容器运行。
# 缺少 zig / python3 / docker / cargo-zigbuild 时自动用 mise / brew / cargo 安装。
#
# 用法：
#   ./docker-test.sh                 # 全部测试
#   ./docker-test.sh -p fkv_store    # 只跑某个 crate
#
# 可用环境变量：
#   FKV_LINUX_TARGET  默认按宿主架构推 aarch64/x86_64-unknown-linux-gnu
#   FKV_TEST_IMAGE   默认 fkv-test-runner:local
#   FKV_CROSS_TARGET_DIR  交叉编译产物目录，默认 /tmp/fkv-target-<target>
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cd "$ROOT"

ensure_cargo_zigbuild() {
  if command -v cargo-zigbuild >/dev/null 2>&1; then
    return
  fi
  echo "==> cargo install cargo-zigbuild"
  cargo install cargo-zigbuild --locked
}

ensure_zig() {
  if command -v zig >/dev/null 2>&1; then
    return
  fi
  if command -v mise >/dev/null 2>&1; then
    echo "==> mise use -g zig@latest"
    mise use -g zig@latest
    mise reshim
  elif command -v brew >/dev/null 2>&1; then
    echo "==> brew install zig"
    brew install zig
  else
    echo "无法安装 zig：需要 mise 或 brew" >&2
    exit 1
  fi
}

ensure_python() {
  if command -v python3 >/dev/null 2>&1; then
    return
  fi
  if command -v brew >/dev/null 2>&1; then
    echo "==> brew install python"
    brew install python
  elif command -v mise >/dev/null 2>&1; then
    echo "==> mise use -g python@latest"
    mise use -g python@latest
    mise reshim
  else
    echo "无法安装 python3：需要 brew 或 mise" >&2
    exit 1
  fi
}

ensure_docker() {
  if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
    return
  fi
  if command -v orb >/dev/null 2>&1; then
    echo "==> orb start"
    orb start
  elif command -v brew >/dev/null 2>&1; then
    echo "==> brew install --cask orbstack"
    brew install --cask orbstack
    if command -v orb >/dev/null 2>&1; then
      orb start
    else
      open -a OrbStack
    fi
    if ! command -v docker >/dev/null 2>&1; then
      echo "==> brew install docker"
      brew install docker
    fi
  else
    echo "无法启动 Docker：需要 OrbStack 或 brew" >&2
    exit 1
  fi
  echo "==> wait docker daemon"
  for _ in $(seq 1 60); do
    if docker info >/dev/null 2>&1; then return; fi
    sleep 1
  done
  echo "Docker daemon 未就绪" >&2
  exit 1
}

ensure_cargo_zigbuild
ensure_zig
ensure_python
ensure_docker

case "$(uname -m)" in
  arm64|aarch64) HOST_ARCH=aarch64 ;;
  x86_64|amd64) HOST_ARCH=x86_64 ;;
  *) echo "不支持的宿主架构: $(uname -m)" >&2; exit 1 ;;
esac
TARGET=${FKV_LINUX_TARGET:-${HOST_ARCH}-unknown-linux-gnu}
IMAGE=${FKV_TEST_IMAGE:-fkv-test-runner:local}
# 交叉编译产物不放仓库：避免 3GB+ target/<triple> 污染工作区 / 同步目录。
export CARGO_TARGET_DIR=${FKV_CROSS_TARGET_DIR:-/tmp/fkv-target-$TARGET}
mkdir -p "$CARGO_TARGET_DIR"

case "$TARGET" in
  aarch64-*) PLATFORM=linux/arm64 ;;
  x86_64-*) PLATFORM=linux/amd64 ;;
  *) echo "不支持的 target: $TARGET" >&2; exit 1 ;;
esac

if ! rustup target list --installed | grep -qx "$TARGET"; then
  echo "==> rustup target add $TARGET"
  rustup target add "$TARGET"
fi

ARTIFACTS=$(mktemp)
trap 'rm -f "$ARTIFACTS"' EXIT

if [ "$#" -gt 0 ]; then
  SCOPE_ARGS=("$@")
else
  SCOPE_ARGS=(--workspace)
fi

echo "==> cargo zigbuild --tests target=$TARGET ${SCOPE_ARGS[*]}"
cargo zigbuild --tests \
  "${SCOPE_ARGS[@]}" --all-features \
  --target "$TARGET" --message-format=json >"$ARTIFACTS"

echo "==> build test image"
docker build --platform "$PLATFORM" -t "$IMAGE" -f docker/Dockerfile.test-run docker >/dev/null

LIST=$(mktemp)
trap 'rm -f "$ARTIFACTS" "$LIST"' EXIT
python3 - "$CARGO_TARGET_DIR" "$ARTIFACTS" >"$LIST" <<'PY'
import json
import os
import sys

target_dir, path = sys.argv[1], sys.argv[2]
seen = set()
with open(path, encoding="utf-8") as fh:
    for line in fh:
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if msg.get("reason") != "compiler-artifact":
            continue
        if not msg.get("profile", {}).get("test"):
            continue
        exe = msg.get("executable")
        if not exe or exe in seen:
            continue
        seen.add(exe)
        print(os.path.relpath(exe, target_dir))
PY

if [ ! -s "$LIST" ]; then
  echo "没有找到测试二进制" >&2
  exit 1
fi

status=0
while IFS= read -r exe; do
  echo "==> $exe"
  docker run --rm \
    --platform "$PLATFORM" \
    --security-opt seccomp=unconfined \
    --ulimit memlock=-1 \
    -v "$ROOT:/work" -w /work \
    -v "$CARGO_TARGET_DIR:/target" \
    -v "$CARGO_TARGET_DIR:$CARGO_TARGET_DIR:ro" \
    -e "LD_LIBRARY_PATH=/target/$TARGET/debug/deps" \
    -e RUST_BACKTRACE=short \
    "$IMAGE" "/target/$exe" --test-threads=1 || status=$?
done <"$LIST"

exit "$status"
