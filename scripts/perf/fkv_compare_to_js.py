#!/usr/bin/env python3
"""把 fkv_compare 的 JSON 报告回填到 web/src/conf/perf.js。

首页矩阵的每个数字都必须能从一次可复现的运行追溯，手工抄写既容易抄错也难以
复核。本脚本是唯一回填入口：

    python3 scripts/perf/fkv_compare_to_js.py \
      --json target/perf/fkv-compare.json \
      --perf-js web/src/conf/perf.js \
      --machine "macOS arm64 (M3 Pro, 12 核) / APFS" \
      --source "本地运行 · HEAD 9a60678"

规则：
- 单元格只写操作数/秒（不逐格带单位），单位与倍率含义在表下一行说明。
- ratio = fkv / 对照引擎，> 1 表示 fkv 更快；倍率行保留 3 位小数。
- 吞吐行 ratio < 0.5 记 tone "bad"（体积行不判 tone）。
- 体积用 MiB（>= 1024 MiB 切 GiB）。
"""

from __future__ import annotations

import argparse
import json
import re
from datetime import date as date_cls
from pathlib import Path

BAD_RATIO_BELOW = 0.5

# 阶段 -> 中文行名（顺序与首页表一致：写三类 -> len() -> 各并发点读 -> 范围读 -> 删除）
ZH_LABELS = {
    "bulk_load": "批量加载",
    "individual_writes": "单条写入",
    "batch_writes": "批量写",
    "len": "len()",
    "range_reads": "随机范围读",
    "removals": "删除",
}
ORDER = ["bulk_load", "individual_writes", "batch_writes", "len"]


def zh_label(phase: str) -> str:
    if phase in ZH_LABELS:
        return ZH_LABELS[phase]
    if phase.startswith("random_reads_"):
        return f"随机读（{phase.rsplit('_', 1)[1]} 线程）"
    return phase


def order_key(phase: str) -> tuple[int, int]:
    if phase.startswith("random_reads_"):
        return (3, int(phase.rsplit("_", 1)[1]))
    if phase in ORDER:
        return (ORDER.index(phase), 0)
    if phase == "range_reads":
        return (4, 0)
    if phase == "removals":
        return (5, 0)
    return (99, 0)


def throughput(cell: dict | None) -> float | None:
    """该阶段的操作数/秒；`ops == 0`（阶段未启用）视作没有数据。"""
    if not cell or not cell.get("ops") or not cell.get("secs"):
        return None
    return cell["ops"] / cell["secs"]


def fmt_ops(cell: dict | None) -> str:
    value = throughput(cell)
    return "—" if value is None else f"{value:.0f}"


def fmt_bytes(value: int | None) -> str:
    if value is None:
        return "—"
    mib = value / (1024 * 1024)
    if mib >= 1024:
        return f"{mib / 1024:.2f} GiB"
    return f"{mib:.2f} MiB"


def fmt_ratio(value: float | None) -> str:
    """倍率保留 3 位小数；小到 0.01 以下时多给两位，避免出现 `0.000x` 这种假零。"""
    if value is None:
        return "null"
    return f'"{value:.3f}x"' if value >= 0.01 else f'"{value:.5f}x"'


def ratio_of(row: dict, other: str) -> float | None:
    fkv = throughput(row.get("fkv"))
    base = throughput(row.get(other))
    if fkv is None or base is None or base <= 0:
        return None
    return fkv / base


def build_rows(report: dict) -> list[str]:
    """行格式：[阶段, fkv, RocksDB, fjall, fkv/RocksDB, fkv/fjall, tone?]。"""
    rows: list[str] = []
    phases = sorted(report["phases"], key=lambda r: order_key(r["phase"]))
    for row in phases:
        # 未启用的阶段（individual_writes / batch_writes 为 0）整行不出现，
        # 免得页面上出现一排 0。
        if throughput(row.get("fkv")) is None and not any(
            throughput(row.get(k)) is not None for k in ("rocksdb", "fjall")
        ):
            continue
        label = zh_label(row["phase"])
        r_rocks = ratio_of(row, "rocksdb")
        r_fjall = ratio_of(row, "fjall")
        worst = [v for v in (r_rocks, r_fjall) if v is not None]
        tone = ""
        if worst and all(v < BAD_RATIO_BELOW for v in worst):
            tone = ', "bad"'
        rows.append(
            f'  ["{label}", "{fmt_ops(row.get("fkv"))}", '
            f'"{fmt_ops(row.get("rocksdb"))}", "{fmt_ops(row.get("fjall"))}", '
            f"{fmt_ratio(r_rocks)}, {fmt_ratio(r_fjall)}{tone}]"
        )

    sizes = report.get("sizes") or {}
    size_cell_li = [fmt_bytes(sizes.get(k)) for k in ("fkv", "rocksdb", "fjall")]
    rows.append(
        f'  ["盘面体积", "{size_cell_li[0]}", "{size_cell_li[1]}", "{size_cell_li[2]}", '
        "null, null]"
    )
    return rows


def build_config(report: dict, machine: str, source: str, date: str) -> str:
    cfg = report["config"]
    rows = cfg["rows"]
    data_mib = rows * cfg["kv_size"] / (1024 * 1024)
    cache_mib = cfg["cache_bytes"] / (1024 * 1024)
    ratio = data_mib / max(cache_mib, 1.0)
    readers = "/".join(str(r) for r in cfg["readers"])
    has_rocksdb = any(row.get("rocksdb") for row in report["phases"])
    blocks = [
        ("机器", [
            ["机器", machine],
            ["页大小", "4 KiB（fkv data page = RocksDB block_size）"],
            ["日期", date],
            ["来源", source],
            ["口径", "三个引擎同一台机器、同一轮次、同一份数据形状"],
            ["并发口径", "readers 档位三方都是真并发：fkv 每读者一个 OS 线程 + 独立 Store（页缓存平分），RocksDB / fjall 等量 std::thread；fkv 在 16/32 档位受 4 vCPU 超订与缓存平分影响，见 doc/perf-compare.md"],
        ]),
        ("测试负载", [
            ["数据量", f"{rows:,} 行 × {cfg['kv_size']} B ≈ {data_mib:.0f} MiB"],
            ["缓存/引擎", f"{cache_mib:.0f} MiB（数据 ≈ {ratio:.1f}× 缓存）"],
            ["key / value", f"{cfg['key_size']} B / {cfg['kv_size'] - cfg['key_size']} B（db_bench 默认）"],
            ["点读", f"{cfg['reads']:,} 次（并发 {readers}）"],
            ["范围读", f"{cfg['scans']:,} 次 × {cfg['scan_len']} 条"],
            ["删除", f"{cfg['removals']:,} 条"],
            ["批量", f"每批 {cfg['batch_size']} 条"],
        ]),
        ("持久化 / fsync", [
            ["fkv", "每批 1×数据页 + 1×manifest fdatasync（共 2 次，无关闭开关）"],
            ["RocksDB", "默认配置：WAL 开、WriteOptions.sync=false（不逐次 fsync）"],
            ["fjall", "默认配置：PersistMode::Buffer（不逐次 fsync）"],
            ["影响", "写阶段的差距来自这里：fkv 每批付两次同步，另外两列用默认配置不付；读阶段不受影响"],
        ]),
        ("RocksDB 列", ([
            ["版本", "系统包 librocksdb（dlopen 同一 C ABI）"],
            ["block cache", f"{cache_mib:.0f} MiB LRU（与两侧缓存同额）"],
            ["block size", "4 KiB"],
            ["index/filter", "cache_index_and_filter_blocks=true（同池）"],
            ["压缩", "kNoCompression（三侧都不压缩）"],
            ["memtable", "write_buffer_size 64 MiB（不属于页缓存）"],
            ["注意", "RocksDB 用默认持久化配置（不逐次 fsync），不为了对齐修改它的默认值；memtable 不属于页缓存"],
        ] if has_rocksdb else [["状态", "本次未启用（--rocksdb-lib 未加载）"]])),
    ]
    body = "{\n"
    for title, item_li in blocks:
        body += f"  {json.dumps(title, ensure_ascii=False)}: [\n"
        for name, value in item_li:
            body += (
                f"    [{json.dumps(name, ensure_ascii=False)}, "
                f"{json.dumps(value, ensure_ascii=False)}],\n"
            )
        body += "  ],\n"
    body += "}"
    return body


def build_meta(report: dict, machine: str, date: str) -> str:
    cfg = report["config"]
    readers = "/".join(str(r) for r in cfg["readers"])
    rows = cfg["rows"]
    data_mib = rows * cfg["kv_size"] / (1024 * 1024)
    cache_mib = cfg["cache_bytes"] / (1024 * 1024)
    ratio = data_mib / max(cache_mib, 1.0)
    return (
        "Rust fkv vs RocksDB vs fjall · "
        f"{date} · {machine} · "
        f"{rows:,} 行 × {cfg['kv_size']} B ≈ {data_mib:.0f} MiB 数据 / "
        f"{cache_mib:.0f} MiB 缓存（≈{ratio:.0f}:1，对齐 RocksDB 官方 97 GiB / 6 GiB）· "
        f"readers {readers} · 每阶段 1 次采样"
    )


def replace_block(text: str, name: str, body: str) -> str:
    pattern = re.compile(
        r"(export const "
        + re.escape(name)
        + r" =\s*)(?:\[[\s\S]*?\];|\{[\s\S]*?\};|\"(?:[^\"\\]|\\.)*\";|true;|false;)",
        re.MULTILINE,
    )
    if not pattern.search(text):
        raise SystemExit(f"{name}: 在 perf.js 里找不到可替换的块")
    return pattern.sub(lambda m: m.group(1) + body, text, count=1)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", required=True)
    parser.add_argument("--perf-js", required=True)
    parser.add_argument("--machine", default="GitHub Actions ubuntu-latest (x86_64)")
    parser.add_argument("--source", default="CI 运行")
    parser.add_argument("--date", default=date_cls.today().isoformat())
    args = parser.parse_args()

    report = json.loads(Path(args.json).read_text())
    text = Path(args.perf_js).read_text()

    text = re.sub(
        r"// 数据源：[^\n]*",
        f"// 数据源：{args.source}（fkv-compare.json）。",
        text,
        count=1,
    )
    text = replace_block(text, "PERF_META", f'"{build_meta(report, args.machine, args.date)}";')
    rows = "[\n" + ",\n".join(build_rows(report)) + "\n];"
    text = replace_block(text, "PERF_MATRIX_ROWS", rows)
    text = replace_block(
        text,
        "PERF_CONFIG",
        build_config(report, args.machine, args.source, args.date) + ";",
    )
    Path(args.perf_js).write_text(text)
    print(f"updated {args.perf_js} from {args.json}")


if __name__ == "__main__":
    main()
