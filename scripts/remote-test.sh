#!/usr/bin/env bash
# 同步源码到远端 Linux 主机并在那里跑测试。
#
# 开发回路用这个；线上固化见 .github/workflows/ci.yml。
#
# 用法：
#   ./scripts/remote-test.sh                 # 默认 c1（x86_64 / 8 核）
#   ./scripts/remote-test.sh n1              # aarch64（6 核 / 7G）
#   ./scripts/remote-test.sh c1 -- -p fkv_store
#   ./scripts/remote-test.sh c1 -- --all-features
#   ./scripts/remote-test.sh c1 --probe      # 只打印远端环境，不跑测试
#
# 环境变量：
#   FKV_REMOTE_DIR         远端源码目录，默认 /tmp/fkv-src
#   FKV_REMOTE_TARGET_DIR  远端 target 目录，默认 /tmp/fkv-target-<host>
#   FKV_REMOTE_JOBS        远端并发 job 数，默认取远端 nproc
#   FKV_REMOTE_CARGO_HOME  cargo home，默认 /opt/rust
#
# 说明：
# - target 目录常驻远端，rsync 不覆盖，增量编译有效；
# - 远端若缺 cargo-nextest，脚本会尝试下载官方预编译二进制到 $CARGO_HOME/bin。
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

HOST=c1
if [ "$#" -gt 0 ] && [ "${1#--}" = "$1" ]; then
  HOST=$1
  shift
fi
PROBE=0
if [ "${1:-}" = "--probe" ]; then
  PROBE=1
  shift
fi
if [ "${1:-}" = "--" ]; then
  shift
fi

DEST=${FKV_REMOTE_DIR:-/tmp/fkv-src}
TARGET_DIR=${FKV_REMOTE_TARGET_DIR:-/tmp/fkv-target-$HOST}
CARGO_HOME_DIR=${FKV_REMOTE_CARGO_HOME:-/opt/rust}

probe() {
  ssh "$HOST" bash -s -- "$DEST" "$TARGET_DIR" "$CARGO_HOME_DIR" <<'REMOTE'
set -euo pipefail
DEST=$1; TARGET_DIR=$2; CARGO_HOME_DIR=$3
export PATH="$CARGO_HOME_DIR/bin:$PATH"
# n1 的 rustup 装在 /opt/rust，但登录 shell 没导 RUSTUP_HOME；
# 不设它 rustup shim 会去找 ~/.rustup 并报「no default configured」。
if [ -d "$CARGO_HOME_DIR/toolchains" ]; then
  export RUSTUP_HOME="$CARGO_HOME_DIR"
fi
export LC_ALL=C
echo "host      : $(hostname)"
echo "arch      : $(uname -m)"
echo "pagesize  : $(getconf PAGESIZE)"
echo "nproc     : $(nproc)"
echo "mem       : $(awk '/MemTotal/{printf "%.0f GiB", $2/1024/1024}' /proc/meminfo)"
echo "rustc     : $(rustc -V 2>/dev/null || echo MISSING)"
echo "cargo     : $(cargo -V 2>/dev/null || echo MISSING)"
echo "nextest   : $(cargo-nextest --version 2>/dev/null | head -1 || echo MISSING)"
echo "src dir   : $DEST"
echo "target dir: $TARGET_DIR"
REMOTE
}

if [ "$PROBE" = 1 ]; then
  probe
  exit 0
fi

echo "==> rsync $ROOT -> $HOST:$DEST"
rsync -a --delete \
  --exclude target/ \
  --exclude .git/ \
  --exclude .data/ \
  --exclude node_modules/ \
  --exclude .codegraph/ \
  --exclude cpp/build/ \
  --exclude web/dist/ \
  "$ROOT/" "$HOST:$DEST/"

echo "==> nextest on $HOST"
ssh "$HOST" bash -s -- "$DEST" "$TARGET_DIR" "$CARGO_HOME_DIR" "${FKV_REMOTE_JOBS:-0}" "$@" <<'REMOTE'
set -euo pipefail
DEST=$1; TARGET_DIR=$2; CARGO_HOME_DIR=$3; JOBS=$4
shift 4

export PATH="$CARGO_HOME_DIR/bin:$PATH"
if [ -d "$CARGO_HOME_DIR/toolchains" ]; then
  export RUSTUP_HOME="$CARGO_HOME_DIR"
fi
export CARGO_TARGET_DIR="$TARGET_DIR"
export RUST_BACKTRACE=${RUST_BACKTRACE:-short}

if ! command -v cargo-nextest >/dev/null 2>&1; then
  echo "==> install cargo-nextest"
  case "$(uname -m)" in
    aarch64|arm64) TRIPLE=linux-arm ;;
    x86_64) TRIPLE=linux ;;
    *) echo "unsupported arch: $(uname -m)" >&2; exit 1 ;;
  esac
  curl -LsSf "https://get.nexte.st/latest/$TRIPLE" | tar zxf - -C "$CARGO_HOME_DIR/bin"
fi

mkdir -p "$TARGET_DIR"
cd "$DEST"

# rust-toolchain.toml 写的是 channel = "nightly"，rustup 会据此联网同步；
# 远端网络受限时 `rustup show` 会挂住甚至把已有 toolchain 更新坏。
# 这里直接从已安装目录挑一个，完全不联网。
if [ -z "${RUSTUP_TOOLCHAIN:-}" ]; then
  for tc in $(ls "$RUSTUP_HOME/toolchains" 2>/dev/null); do
    case "$tc" in
      nightly-*|stable-*|beta-*) RUSTUP_TOOLCHAIN=$tc; break ;;
    esac
  done
  if [ -n "${RUSTUP_TOOLCHAIN:-}" ]; then
    export RUSTUP_TOOLCHAIN
  fi
fi

if [ "$JOBS" = 0 ]; then
  JOBS=$(nproc)
fi

echo "toolchain: ${RUSTUP_TOOLCHAIN:-<toolchain file>}"
echo "jobs     : $JOBS"

exec cargo nextest run --jobs "$JOBS" --no-tests=pass "$@"
REMOTE
