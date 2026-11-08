#!/usr/bin/env bash
# 幂等安装 Linux 原生构建 / 测试依赖。
#
# 用法：
#   bash scripts/ci/install.sh          # CI 里直接执行
#   source scripts/ci/install.sh        # 本地 source 后调用 install_deps
#   bash scripts/ci/install.sh --check  # 只检查命令，不安装
#
# 环境变量：
#   CI=true       使用 sudo -n，不进行任何交互（GitHub Actions 默认）
#   FKV_APT_MIRROR  可选，替换 apt 源（例如内网镜像）
#
# 非 Linux：打印 skip 并 exit 0。幂等：已安装的包不会重复触发下载。

PKG_LI=(
  clang
  libclang-dev
  cmake
  ninja-build
  pkg-config
  liburing-dev
  libzstd-dev
  libboost-all-dev
  libjsoncpp-dev
  libssl-dev
  libaio-dev
  rsync
  curl
  unzip
)

# 包名 -> 需要存在的可执行文件（同名的直接复用包名）。
CMD_LI=(clang cmake ninja pkg-config rsync curl unzip)

_say() { printf '%s\n' "$*"; }
_err() { printf '错误: %s\n' "$*" >&2; }

_need_sudo() {
  if [ "$(id -u)" -eq 0 ]; then
    echo ""
  elif [ "${CI:-}" = "true" ]; then
    echo "sudo -n"
  else
    echo "sudo"
  fi
}

_check_commands() {
  local missing=0 cmd
  for cmd in "${CMD_LI[@]}"; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
      _err "缺少命令: $cmd"
      missing=1
    fi
  done
  return "$missing"
}

_check_only() {
  if _check_commands; then
    _say "依赖检查通过: ${CMD_LI[*]}"
    return 0
  fi
  return 1
}

install_deps() {
  local uname_s
  uname_s=$(uname -s)
  if [ "$uname_s" != "Linux" ]; then
    _say "skip: 非 Linux ($uname_s)，不安装 apt 依赖"
    return 0
  fi

  if [ "${1:-}" = "--check" ]; then
    _check_only
    return $?
  fi

  if ! command -v apt-get >/dev/null 2>&1; then
    _err "当前 Linux 没有 apt-get，无法自动安装: ${PKG_LI[*]}"
    return 1
  fi

  local -a sudo_cmd=()
  local sudo_str
  sudo_str=$(_need_sudo)
  if [ -n "$sudo_str" ]; then
    read -r -a sudo_cmd <<<"$sudo_str"
    if ! "${sudo_cmd[@]}" true >/dev/null 2>&1; then
      _err "需要 root 或免密 sudo 才能安装系统包"
      return 1
    fi
  fi

  # 先算缺哪些包，全部已装则跳过 apt-get update，保证幂等且快。
  local -a missing_li=()
  local pkg status
  for pkg in "${PKG_LI[@]}"; do
    status=$(dpkg-query -W -f='${Status}' "$pkg" 2>/dev/null || true)
    if [ "$status" != "install ok installed" ]; then
      missing_li+=("$pkg")
    fi
  done

  if [ "${#missing_li[@]}" -eq 0 ]; then
    _say "apt 依赖已齐: ${PKG_LI[*]}"
  else
    _say "安装 apt 包: ${missing_li[*]}"
    export DEBIAN_FRONTEND=noninteractive
    if [ -n "${FKV_APT_MIRROR:-}" ]; then
      _say "使用 apt 镜像: $FKV_APT_MIRROR"
      # 只替换官方 archive/security 主机名，保留原有路径与发行版代号。
      "${sudo_cmd[@]}" sed -ri "s#https?://(archive|security)\.ubuntu\.com#${FKV_APT_MIRROR}#g" \
        /etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources 2>/dev/null || true
    fi
    # 托管 runner 的 apt 源偶发卡死；update 失败不阻断：镜像自带包索引仍可
    # 安装，install 本身再加硬超时，避免 job 在 install.sh 里挂 10+ 分钟。
    if ! timeout 180 "${sudo_cmd[@]}" apt-get update -y \
      -o Acquire::http::Timeout=15 -o Acquire::Retries=2; then
      _say "apt-get update 超时/失败，继续用现有索引安装"
    fi
    timeout 300 "${sudo_cmd[@]}" apt-get install -y --no-install-recommends \
      -o Dpkg::Options::=--force-confold -o Acquire::http::Timeout=15 \
      -o Acquire::Retries=2 "${missing_li[@]}"
  fi

  _check_commands
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  set -euo pipefail
  install_deps "$@"
fi
