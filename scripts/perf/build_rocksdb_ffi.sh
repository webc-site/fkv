#!/usr/bin/env bash
# 编译 RocksDB 侧的 C ABI 适配器（crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp）。
#
# 产物是一个共享库，由 fkv_compare 通过 `--rocksdb-lib <path>` dlopen 加载，
# 因此 Rust 构建不需要链接 RocksDB。找不到 RocksDB 时脚本非 0 退出，调用方
# 可以选择跳过 RocksDB 列（矩阵仍是 fkv / fjall 两列）。
#
# 用法：
#   bash scripts/perf/build_rocksdb_ffi.sh [输出文件]
#
# 环境变量：
#   ROCKSDB_PREFIX  RocksDB 安装前缀（含 include/ 与 lib/）；默认按
#                   pkg-config -> /usr -> /usr/local -> Homebrew 顺序探测。
#   CXX             编译器，默认 c++。
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
src_dir="$root/crate/perf/fkv_compare/cpp"
cxx=${CXX:-c++}

os=$(uname -s)
case "$os" in
  Darwin) lib_name="librocksdb_bench_ffi.dylib" ;;
  *) lib_name="librocksdb_bench_ffi.so" ;;
esac
out=${1:-"$root/target/perf/$lib_name"}

say() { printf '[rocksdb-ffi] %s\n' "$*"; }

# 1) 定位 include / lib 目录。
include_dir=""
lib_dir=""
if [ -n "${ROCKSDB_PREFIX:-}" ]; then
  include_dir="$ROCKSDB_PREFIX/include"
  lib_dir="$ROCKSDB_PREFIX/lib"
elif command -v pkg-config >/dev/null 2>&1 && pkg-config --exists rocksdb; then
  include_dir=$(pkg-config --variable=includedir rocksdb)
  lib_dir=$(pkg-config --variable=libdir rocksdb)
else
  for prefix in /usr /usr/local /opt/homebrew/opt/rocksdb /opt/rocksdb; do
    if [ -f "$prefix/include/rocksdb/db.h" ]; then
      include_dir="$prefix/include"
      lib_dir="$prefix/lib"
      break
    fi
  done
fi

if [ -z "$include_dir" ] || [ ! -f "$include_dir/rocksdb/db.h" ]; then
  say "未找到 rocksdb/db.h（可用 ROCKSDB_PREFIX 指定前缀）" >&2
  exit 1
fi
if [ -z "$lib_dir" ] || [ ! -d "$lib_dir" ]; then
  say "未找到 RocksDB 库目录（可用 ROCKSDB_PREFIX 指定前缀）" >&2
  exit 1
fi

mkdir -p "$(dirname "$out")"
say "编译 $out"
say "  include: $include_dir"
say "  lib:     $lib_dir"

"$cxx" -O2 -std=c++20 -fPIC -shared \
  -o "$out" \
  "$src_dir/rocksdb_ffi.cpp" \
  -I"$src_dir" -I"$include_dir" -L"$lib_dir" -lrocksdb

say "完成: $out"
