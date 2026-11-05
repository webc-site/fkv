#!/usr/bin/env bash
# 下载并以原生进程启动 RustFS（S3 兼容），不依赖 Docker。
#
# 用法：
#   bash scripts/ci/s3.sh start     # 下载/启动/建桶，并导出 EKV_S3_*
#   bash scripts/ci/s3.sh status    # 检查进程、端口与桶
#   bash scripts/ci/s3.sh stop      # 停进程并清理数据目录
#
#   source scripts/ci/s3.sh         # 本地 source 后调用 s3_start / s3_stop / s3_status
#
# 环境变量：
#   FKV_CI_CACHE     缓存目录，默认 ${RUNNER_TEMP:-/tmp}/fkv-ci
#   FKV_RUSTFS_TAG   RustFS 版本，默认 1.0.4（tag 不带 v，资产名带 v）
#   GITHUB_ENV       存在时把 EKV_S3_* 追加进去（GitHub Actions 自动设置）
#
# 目录布局：
#   $FKV_CI_CACHE/rustfs-<asset>.zip   下载缓存
#   $FKV_CI_CACHE/rustfs-dist/         解压目录
#   $FKV_CI_CACHE/rustfs-data/         数据目录（stop 时删除）
#   $FKV_CI_CACHE/rustfs.pid  rustfs.log  s3.env

RUSTFS_TAG="${FKV_RUSTFS_TAG:-1.0.4}"
RUSTFS_TAG="${RUSTFS_TAG#v}"
RUSTFS_VERSION="v${RUSTFS_TAG}"
S3_ADDR="127.0.0.1:9900"
S3_ENDPOINT="http://${S3_ADDR}"
S3_BUCKET="fkv-test"
S3_ACCESS_KEY="minioadmin"
S3_SECRET_KEY="minioadmin"
S3_REGION="us-east-1"

_s3_say() { printf '%s\n' "$*"; }
_s3_note() { printf '%s\n' "$*" >&2; }
_s3_err() { printf 'rustfs 错误: %s\n' "$*" >&2; }

_s3_cache() {
  local cache="${FKV_CI_CACHE:-${RUNNER_TEMP:-/tmp}/fkv-ci}"
  printf '%s\n' "$cache"
}

_s3_asset() {
  local os arch
  os=$(uname -s)
  arch=$(uname -m)
  case "$arch" in
    x86_64|amd64) arch=x86_64 ;;
    arm64|aarch64) arch=aarch64 ;;
    *) _s3_err "不支持的架构: $arch"; return 1 ;;
  esac
  case "$os" in
    Linux)
      # Linux 只有 gnu / musl；CI 的 Ubuntu runner 用 gnu。
      printf 'rustfs-linux-%s-gnu-%s.zip\n' "$arch" "$RUSTFS_VERSION"
      ;;
    Darwin)
      if [ "$arch" != "aarch64" ]; then
        _s3_err "RustFS ${RUSTFS_VERSION} 没有 macOS ${arch} 资产（仅 aarch64）"
        return 1
      fi
      printf 'rustfs-macos-%s-%s.zip\n' "$arch" "$RUSTFS_VERSION"
      ;;
    *)
      _s3_err "不支持的系统: $os"
      return 1
      ;;
  esac
}

_s3_log_tail() {
  local cache log
  cache=$(_s3_cache)
  log="$cache/rustfs.log"
  if [ -f "$log" ]; then
    _s3_err "rustfs.log 尾部:"
    tail -n 50 "$log" >&2 || true
  fi
}

_s3_binary() {
  local cache dist
  cache=$(_s3_cache)
  dist="$cache/rustfs-dist"
  if [ -x "$dist/rustfs" ]; then
    printf '%s\n' "$dist/rustfs"
    return 0
  fi
  local found
  found=$(find "$dist" -type f -name rustfs -perm -u+x 2>/dev/null | head -n 1)
  if [ -z "$found" ]; then
    found=$(find "$dist" -type f -name rustfs 2>/dev/null | head -n 1)
  fi
  [ -n "$found" ] || return 1
  chmod +x "$found"
  printf '%s\n' "$found"
}

_s3_download() {
  local cache asset url zip tmp fallback
  cache=$(_s3_cache)
  mkdir -p "$cache"
  asset=$(_s3_asset) || return 1
  zip="$cache/$asset"

  if [ -f "$zip" ] && [ -s "$zip" ]; then
    _s3_note "已缓存: $zip"
    printf '%s\n' "$zip"
    return 0
  fi

  url="https://github.com/rustfs/rustfs/releases/download/${RUSTFS_TAG}/${asset}"
  fallback="https://github.com/rustfs/rustfs/releases/download/${RUSTFS_VERSION}/${asset}"
  tmp="$zip.part"
  rm -f "$tmp"

  _s3_note "下载 $url"
  if ! curl -fL --retry 3 --retry-delay 2 --connect-timeout 20 \
      -o "$tmp" "$url"; then
    _s3_note "主地址失败，尝试 $fallback"
    if ! curl -fL --retry 3 --retry-delay 2 --connect-timeout 20 \
        -o "$tmp" "$fallback"; then
      rm -f "$tmp"
      _s3_err "下载失败: $asset"
      return 1
    fi
  fi
  mv "$tmp" "$zip"
  printf '%s\n' "$zip"
}

_s3_extract() {
  local zip cache dist
  zip=$1
  cache=$(_s3_cache)
  dist="$cache/rustfs-dist"
  mkdir -p "$dist"
  if ! unzip -o -q "$zip" -d "$dist"; then
    _s3_err "解压失败: $zip"
    return 1
  fi
  if ! _s3_binary >/dev/null; then
    _s3_err "解压后找不到 rustfs 可执行文件: $dist"
    find "$dist" -maxdepth 3 -type f >&2 || true
    return 1
  fi
}

_s3_pid() {
  local cache pidfile
  cache=$(_s3_cache)
  pidfile="$cache/rustfs.pid"
  [ -f "$pidfile" ] || return 1
  cat "$pidfile"
}

_s3_alive() {
  local pid
  pid=$(_s3_pid) || return 1
  [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null
}

_s3_ready() {
  local code
  code=$(curl -s -o /dev/null -m 2 -w '%{http_code}' "$S3_ENDPOINT/" 2>/dev/null || true)
  [ -n "$code" ] && [ "$code" != "000" ]
}

_s3_create_bucket() {
  local body code i
  body=$(mktemp)
  # 根端点先响应、storage quorum 仍在初始化时会返回 503；最多等 60s。
  for i in $(seq 1 120); do
    code=$(curl -s -o "$body" -w '%{http_code}' -m 10 \
      --aws-sigv4 "aws:amz:${S3_REGION}:s3" \
      --user "${S3_ACCESS_KEY}:${S3_SECRET_KEY}" \
      -X PUT "${S3_ENDPOINT}/${S3_BUCKET}") || code="000"
    # 200/204 新建成功；409 表示已存在（BucketAlreadyOwnedByYou）。
    if [ "$code" = "200" ] || [ "$code" = "204" ] || [ "$code" = "409" ]; then
      _s3_say "桶就绪: $S3_BUCKET (HTTP $code)"
      rm -f "$body"
      return 0
    fi
    if [ "$code" != "500" ] && [ "$code" != "503" ] && [ "$code" != "000" ]; then
      break
    fi
    sleep 0.5
  done
  _s3_err "建桶失败: HTTP $code"
  if [ -s "$body" ]; then
    _s3_err "响应: $(cat "$body")"
  fi
  rm -f "$body"
  return 1
}

_s3_bucket_ok() {
  local code
  # HEAD 是只读检查，status 不应依赖会重试 60s 的建桶流程。
  code=$(curl -s -o /dev/null -w '%{http_code}' -I -m 10 \
    --aws-sigv4 "aws:amz:${S3_REGION}:s3" \
    --user "${S3_ACCESS_KEY}:${S3_SECRET_KEY}" \
    "${S3_ENDPOINT}/${S3_BUCKET}") || code="000"
  [ "$code" = "200" ]
}

_s3_write_env() {
  local cache envfile
  cache=$(_s3_cache)
  envfile="$cache/s3.env"
  cat >"$envfile" <<EOF
EKV_S3_ENDPOINT=${S3_ENDPOINT}
EKV_S3_BUCKET=${S3_BUCKET}
EKV_S3_ACCESS_KEY=${S3_ACCESS_KEY}
EKV_S3_SECRET_KEY=${S3_SECRET_KEY}
EKV_S3_REGION=${S3_REGION}
EOF
  _s3_say "已写入 $envfile"

  if [ -n "${GITHUB_ENV:-}" ] && [ -w "${GITHUB_ENV}" ]; then
    {
      echo "EKV_S3_ENDPOINT=${S3_ENDPOINT}"
      echo "EKV_S3_BUCKET=${S3_BUCKET}"
      echo "EKV_S3_ACCESS_KEY=${S3_ACCESS_KEY}"
      echo "EKV_S3_SECRET_KEY=${S3_SECRET_KEY}"
      echo "EKV_S3_REGION=${S3_REGION}"
    } >>"$GITHUB_ENV"
    _s3_say "已追加到 \$GITHUB_ENV"
  fi

  # source 使用时也能直接拿到变量。
  export EKV_S3_ENDPOINT="$S3_ENDPOINT"
  export EKV_S3_BUCKET="$S3_BUCKET"
  export EKV_S3_ACCESS_KEY="$S3_ACCESS_KEY"
  export EKV_S3_SECRET_KEY="$S3_SECRET_KEY"
  export EKV_S3_REGION="$S3_REGION"
}

s3_start() {
  local cache zip bin data pidfile log pid i
  cache=$(_s3_cache)
  mkdir -p "$cache"
  data="$cache/rustfs-data"
  pidfile="$cache/rustfs.pid"
  log="$cache/rustfs.log"

  if _s3_alive; then
    pid=$(_s3_pid)
    _s3_say "RustFS 已在运行 (pid $pid)"
    if ! _s3_bucket_ok && ! _s3_create_bucket; then
      _s3_log_tail
      return 1
    fi
    _s3_write_env
    return 0
  fi
  rm -f "$pidfile"

  zip=$(_s3_download) || return 1
  _s3_extract "$zip" || return 1
  bin=$(_s3_binary) || { _s3_err "找不到 rustfs 可执行文件"; return 1; }
  _s3_say "二进制: $bin"

  rm -rf "$data"
  mkdir -p "$data"

  _s3_say "启动 RustFS: $S3_ADDR (data=$data)"
  RUSTFS_ADDRESS="$S3_ADDR" \
  RUSTFS_ACCESS_KEY="$S3_ACCESS_KEY" \
  RUSTFS_SECRET_KEY="$S3_SECRET_KEY" \
  RUSTFS_CONSOLE_ENABLE=false \
    nohup "$bin" server "$data" >"$log" 2>&1 &
  pid=$!
  echo "$pid" >"$pidfile"

  for i in $(seq 1 120); do
    if ! kill -0 "$pid" 2>/dev/null; then
      _s3_err "RustFS 进程提前退出 (pid $pid)"
      _s3_log_tail
      rm -f "$pidfile"
      return 1
    fi
    if _s3_ready; then
      _s3_say "RustFS 就绪 (pid $pid)"
      break
    fi
    sleep 0.5
  done

  if ! _s3_ready; then
    _s3_err "等待 RustFS 就绪超时"
    _s3_log_tail
    return 1
  fi

  if ! _s3_create_bucket; then
    _s3_log_tail
    # 建桶失败时不留半启动状态，避免下一次 start 误判为已在运行。
    kill "$pid" 2>/dev/null || true
    rm -f "$pidfile"
    return 1
  fi

  _s3_write_env
  return 0
}

s3_stop() {
  local cache pidfile pid i
  cache=$(_s3_cache)
  pidfile="$cache/rustfs.pid"

  if [ -f "$pidfile" ]; then
    pid=$(cat "$pidfile" 2>/dev/null || true)
  else
    pid=""
  fi

  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
    _s3_say "停止 RustFS (pid $pid)"
    kill "$pid" 2>/dev/null || true
    for i in $(seq 1 40); do
      kill -0 "$pid" 2>/dev/null || break
      sleep 0.25
    done
    if kill -0 "$pid" 2>/dev/null; then
      _s3_err "进程未退出，发送 SIGKILL"
      kill -9 "$pid" 2>/dev/null || true
    fi
  else
    _s3_say "RustFS 未运行"
  fi

  rm -f "$pidfile"
  rm -rf "$cache/rustfs-data"
  _s3_say "已清理数据目录: $cache/rustfs-data"
  return 0
}

s3_status() {
  local cache pidfile pid running=0 rc=0
  cache=$(_s3_cache)
  pidfile="$cache/rustfs.pid"

  if [ -f "$pidfile" ] && pid=$(cat "$pidfile" 2>/dev/null) && kill -0 "$pid" 2>/dev/null; then
    _s3_say "进程: 运行中 (pid $pid)"
    running=1
  else
    _s3_say "进程: 未运行"
    rc=1
  fi

  if [ "$running" -eq 1 ]; then
    if _s3_ready; then
      _s3_say "端口: $S3_ADDR 可访问"
    else
      _s3_say "端口: $S3_ADDR 不可访问"
      rc=1
    fi
  elif _s3_ready; then
    # 常见于 Docker/OrbStack 仍在转发 9900；不能误报为本脚本的 RustFS。
    _s3_say "端口: $S3_ADDR 被其他进程占用"
  else
    _s3_say "端口: $S3_ADDR 不可访问"
  fi

  if [ "$rc" -eq 0 ]; then
    if _s3_bucket_ok; then
      _s3_say "桶: $S3_BUCKET 可用"
    else
      _s3_say "桶: $S3_BUCKET 不可用"
      rc=1
    fi
  fi

  _s3_say "日志: $cache/rustfs.log"
  _s3_say "环境: $cache/s3.env"
  return "$rc"
}

_s3_usage() {
  cat <<'EOF'
用法: scripts/ci/s3.sh <start|stop|status>

  start   下载并后台启动 RustFS，创建 fkv-test 桶，导出 EKV_S3_*
  stop    停止进程并清理数据目录
  status  检查进程、端口与桶

环境变量: FKV_CI_CACHE, FKV_RUSTFS_TAG, GITHUB_ENV
EOF
}

_s3_main() {
  case "${1:-}" in
    start) s3_start ;;
    stop) s3_stop ;;
    status) s3_status ;;
    -h|--help|help) _s3_usage ;;
    *) _s3_usage >&2; return 2 ;;
  esac
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  set -euo pipefail
  _s3_main "$@"
fi
