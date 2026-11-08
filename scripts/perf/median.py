#!/usr/bin/env python3
"""把多次 fkv_compare 运行合并成一份中位数报告。

单次采样在共享 CI runner 上抖动很大（同一份代码两轮删除阶段能差 2.8 倍），
矩阵页面又必须给可复核的数字，所以由 CI 跑 3 轮、这里取每阶段中位数。

用法：

    python3 scripts/perf/median.py --out median.json run-1.json run-2.json run-3.json

输出结构与 fkv_compare 的 --json 一致（`phases[].{fkv,rocksdb,fjall}` 为
`{ops, secs}`），且 `ops/secs` 等于各轮吞吐的中位数；`config` / `sizes` 取第一轮。
"""

from __future__ import annotations

import argparse
import json
import statistics
from pathlib import Path


def throughput(cell: dict | None) -> float | None:
    if not cell or not cell.get("ops") or not cell.get("secs"):
        return None
    return cell["ops"] / cell["secs"]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", required=True)
    parser.add_argument("runs", nargs="+")
    args = parser.parse_args()

    report_li = [json.loads(Path(path).read_text()) for path in args.runs]
    if not report_li:
        raise SystemExit("没有输入报告")

    base = report_li[0]
    phases = []
    for phase in base["phases"]:
        row = {"phase": phase["phase"]}
        for engine in ("fkv", "rocksdb", "fjall"):
            samples = [
                value
                for report in report_li
                for p in report["phases"]
                if p["phase"] == phase["phase"]
                and (value := throughput(p.get(engine))) is not None
            ]
            value = statistics.median(samples) if samples else None
            row[engine] = None if value is None else {"ops": value, "secs": 1.0}
        phases.append(row)

    merged = {
        "config": base["config"],
        "phases": phases,
        "sizes": base.get("sizes") or {},
        "runs": len(report_li),
    }
    Path(args.out).write_text(json.dumps(merged, indent=2))
    print(f"median of {len(report_li)} runs -> {args.out}")


if __name__ == "__main__":
    main()
