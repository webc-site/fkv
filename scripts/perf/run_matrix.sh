#!/usr/bin/env bash
# 本地跑一轮「对齐口径」的三引擎矩阵，用于迭代期间的快速对比。
#
# 口径与 CI `perf.yml` 一致：25M 行 × 116 B、每引擎 256 MiB 缓存、readers 1/4/8/16/32、
# 三个引擎都逐批 fsync（fkv 本来如此；RocksDB 由 FKV_BENCH_SYNC=1 打开 sync=true；
# fjall 用 PersistMode::SyncAll）。
#
# 用法：
#   bash scripts/perf/run_matrix.sh [输出目录]
#
# 环境变量：
#   ROWS / CACHE_BYTES / READERS  覆盖默认规模（调试时用小的，例如 ROWS=2000000）
#   REPEAT                        采样轮数（默认 3；每轮独立目录，脚本取中位数）
#   FKV_DRIVER                    store（默认）/ runtime
#   ROCKSDB_LIB                   默认 target/perf/librocksdb_bench_ffi.*
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
out_dir=${1:-"$root/target/perf/local"}
rows=${ROWS:-25000000}
cache=${CACHE_BYTES:-268435456}
readers=${READERS:-1,4,8,16,32}
repeat=${REPEAT:-3}
driver=${FKV_DRIVER:-store}

mkdir -p "$out_dir"
bin="$root/target/release/fkv_compare"
if [ ! -x "$bin" ]; then
  echo "先构建： cargo build --release -p fkv_compare" >&2
  exit 1
fi

# RocksDB 适配器：有就带上，没有就只跑 fkv / fjall。
rocks=${ROCKSDB_LIB:-}
if [ -z "$rocks" ]; then
  for cand in "$root"/target/perf/librocksdb_bench_ffi.*; do
    [ -e "$cand" ] && rocks=$cand && break
  done
fi

run_once() {
  local idx=$1
  local json="$out_dir/run-$idx.json"
  local args=(
    --rows "$rows" --bulk-rows "$rows"
    --cache-bytes "$cache" --readers "$readers"
    --fkv-driver "$driver"
    --workdir "$out_dir/work-$idx"
    --json "$json"
  )
  if [ -n "$rocks" ]; then
    args+=(--rocksdb-lib "$rocks")
  fi
  echo "[matrix] run $idx/$repeat (driver=$driver rocksdb=${rocks:-none})"
  "$bin" "${args[@]}"
}

for i in $(seq 1 "$repeat"); do
  run_once "$i"
done

python3 - "$out_dir" <<'PY'
import json, statistics, sys
from pathlib import Path

out = Path(sys.argv[1])
run_li = sorted(out.glob("run-*.json"))
reports = [json.loads(p.read_text()) for p in run_li]

def ops(cell):
    if not cell or not cell.get("ops") or not cell.get("secs"):
        return None
    return cell["ops"] / cell["secs"]

merged = []
for phase in reports[0]["phases"]:
    row = {"phase": phase["phase"]}
    for engine in ("fkv", "rocksdb", "fjall"):
        samples = [ops(r_phase.get(engine)) for r in reports for r_phase in r["phases"] if r_phase["phase"] == phase["phase"]]
        samples = [s for s in samples if s]
        row[engine] = statistics.median(samples) if samples else None
    merged.append(row)

print(f"{'阶段':<18}{'fkv':>12}{'RocksDB':>12}{'fjall':>12}{'fkv/RocksDB':>13}{'fkv/fjall':>12}")
for row in merged:
    def ratio(other):
        if row["fkv"] and row.get(other):
            return f"{row['fkv'] / row[other]:.3f}x"
        return "—"
    print(
        f"{row['phase']:<18}"
        + f"{row['fkv'] or 0:>12,.0f}"
        + f"{row['rocksdb'] or 0:>12,.0f}"
        + f"{row['fjall'] or 0:>12,.0f}"
        + f"{ratio('rocksdb'):>13}{ratio('fjall'):>12}"
    )

sizes = reports[0].get("sizes") or {}
print()
for key, value in sizes.items():
    print(f"{key}: {'—' if value is None else f'{value / 2**20:.1f} MiB'}")

merged_path = out / "median.json"
merged_path.write_text(json.dumps({"phases": merged, "sizes": sizes, "runs": len(reports)}, indent=2))
print(f"\n中位数写入 {merged_path}")
PY
