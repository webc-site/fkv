#!/usr/bin/env bash
# 依赖方向门禁：只允许从高层 crate 依赖低层 crate（同层依赖允许）。
#
# 层序与 doc/architecture/01-overview.md「分层与依赖方向」保持一致，
# 文档改层时必须同步下面的 LAYERS。脚本只检查生产 path 依赖
# （dependencies / build-dependencies 及其 target 变体），
# dev-dependencies 与 crate/test/* 不参与生产分层。
#
# 发现倒挂时打印 `crate -> dep (Lx -> Ly)` 并以非零退出。
set -euo pipefail
DIR=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
python3 - "$DIR" <<'PY'
import sys

try:
    import tomllib
except ModuleNotFoundError:
    sys.exit("check_layers: python3.11+ is required (tomllib)")

from pathlib import Path

ROOT = Path(sys.argv[1])

# doc/architecture/01-overview.md 的层表。索引越小越底层。
LAYERS = {
    "L0": [
        "fkv_error", "fkv_types", "fkv_codec", "fkv_config",
        "fkv_metrics", "fkv_failpoint",
    ],
    "L1": ["fkv_page", "fkv_compress", "fkv_manifest"],
    "L2": ["fkv_buf", "fkv_mapping", "fkv_btree"],
    "L3": [
        "fkv_io", "fkv_io_mem", "fkv_io_compio", "fkv_io_sim",
        "fkv_object", "fkv_object_s3", "fkv_standby", "fkv_large",
    ],
    "L4": [
        "fkv_api", "fkv_task_read", "fkv_task_scan", "fkv_task_write",
        "fkv_task_reopen", "fkv_task_compact", "fkv_task_gc",
        "fkv_task_prewarm", "fkv_recovery", "fkv_gc", "fkv_cloud",
        "fkv_store", "fkv_shard", "fkv_runtime",
    ],
    "L5": ["fkv", "fkv_sdk", "fkv_cli", "fkv_testkit", "fkv_bench"],
}
LEVEL = {name: i for i, names in enumerate(LAYERS.values()) for name in names}
# 只放隔离测试、不参与生产分层的目录。
SKIP_PREFIXES = ("crate/test/",)


def load(manifest):
    with manifest.open("rb") as f:
        return tomllib.load(f)


def production_path_deps(data):
    tables = []
    for key in ("dependencies", "build-dependencies"):
        tables.append(data.get(key, {}))
    for target in data.get("target", {}).values():
        for key in ("dependencies", "build-dependencies"):
            tables.append(target.get(key, {}))
    for table in tables:
        for name, spec in table.items():
            if isinstance(spec, dict) and "path" in spec:
                yield spec.get("package", name), spec["path"]


members = {}
for manifest in sorted((ROOT / "crate").glob("*/*/Cargo.toml")):
    data = load(manifest)
    name = data["package"]["name"]
    rel = manifest.relative_to(ROOT).as_posix()
    members[manifest.parent.resolve()] = name
    if rel.startswith(SKIP_PREFIXES):
        continue
    if name not in LEVEL:
        sys.exit(
            f"error: workspace crate `{name}` ({rel}) is not in "
            f"doc/architecture/01-overview.md layer table"
        )

violations = []
edges = 0
for manifest in sorted((ROOT / "crate").glob("*/*/Cargo.toml")):
    rel = manifest.relative_to(ROOT).as_posix()
    if rel.startswith(SKIP_PREFIXES):
        continue
    crate = members[manifest.parent.resolve()]
    for dep, path in production_path_deps(load(manifest)):
        dep_dir = (manifest.parent / path).resolve()
        dep_name = members.get(dep_dir)
        if dep_name is None:
            # 工作区外的 path 依赖（例如 fuzz）不参与分层。
            continue
        edges += 1
        src_level = LEVEL[crate]
        dst_level = LEVEL[dep_name]
        if src_level < dst_level:
            violations.append((crate, dep_name, src_level, dst_level))

if violations:
    print("error: crate layering violations (higher layers must not be depended on by lower layers):")
    for crate, dep, src, dst in violations:
        print(f"  {crate} -> {dep} (L{src} -> L{dst})")
    sys.exit(1)

print(f"crate layering OK: {len(LEVEL)} crates, {edges} production path edges")
PY
# 行数门禁（struct-22）：生产 src ≤1000，测试专用文件 ≤1500。
bash "$(dirname -- "$0")/check_line_count.sh"
