#!/usr/bin/env bash
# 行数门禁（struct-22 口径，struct-24 补齐内联测试段，struct-25 修正起点）：
# 1. 生产 `src` 文件 ≤1000 行。生产行数取首个「内联测试模块」之前的行数，
#    并剔除测试专用文件（`*_tests.rs`、`test_support.rs`、`src/tests.rs`、
#    `src/tests/**`）。内联测试模块指 `#[cfg(test)] mod <name> { ... }`
#    （允许属性在上一行，中间可隔其他属性 / 注释 / 空行）；
#    `#[cfg(test)]` 修饰 use / static / fn，或 `mod <name>;`（模块体在别的
#    文件）都不是内联测试段，仍按生产行计。
# 2. 生产文件的内联测试段（内联测试模块起点至文件末）≤1500 行，
#    且生产文件原始总行数（生产段 + 内联测试段）≤1500 行；
#    超出即把内联测试拆到 `src/<name>/tests.rs` 或 `tests/`。
# 3. 测试专用文件不参与生产门禁，但单文件 ≤1500 行；超过即按语义域拆分。
#    集成测试 `tests/**` 只做上限检查，不进入生产清单。
#
# 发现越线时打印文件清单并以非零退出。
set -euo pipefail
DIR=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
exec python3 - "$DIR" <<'PY'
import re
import sys
from pathlib import Path

ROOT = Path(sys.argv[1])
PRODUCTION_LIMIT = 1000
TEST_LIMIT = 1500
INLINE_WATCH_LIMIT = 200


def is_test_only(rel: Path) -> bool:
    parts = rel.parts
    name = rel.name
    if "tests" in parts[:-1]:
        return True
    if name.endswith("_tests.rs") or name == "test_support.rs":
        return True
    # src/tests.rs 或 src/tests/** 是 bin/lib 的测试模块，不是生产代码。
    if name == "tests.rs" and rel.parent.name == "src":
        return True
    for i, part in enumerate(parts[:-1]):
        if part == "src" and i + 1 < len(parts) and parts[i + 1] == "tests":
            return True
    return False


MOD_DECL_RE = r"(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+\w+\s*\{"
# 同一行：`#[cfg(test)] mod tests {`（中间可隔其他属性）。
CFG_TEST_INLINE_MOD_RE = re.compile(
    r"^\s*#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*" + MOD_DECL_RE
)
MOD_DECL_START_RE = re.compile(r"^\s*" + MOD_DECL_RE)


def is_inline_test_mod(lines: list[str], idx: int) -> bool:
    """`lines[idx]` 是 `#[cfg(test)]` 行时，判断其后是否紧跟内联测试模块。"""
    if CFG_TEST_INLINE_MOD_RE.match(lines[idx]):
        return True
    # 属性在上一行：跳过后续属性 / 注释 / 空行后应紧跟 `mod <name> {`。
    j = idx + 1
    while j < len(lines):
        stripped = lines[j].strip()
        if not stripped or stripped.startswith("#[") or stripped.startswith("//"):
            j += 1
            continue
        return bool(MOD_DECL_START_RE.match(lines[j]))
    return False


def production_lines(lines: list[str]) -> int:
    for lineno, line in enumerate(lines):
        if line.strip().startswith("#[cfg(test)]") and is_inline_test_mod(lines, lineno):
            return lineno
    return len(lines)


production = []
test_only = []
paths = list((ROOT / "crate").glob("*/*/src/**/*.rs"))
paths += list((ROOT / "crate").glob("*/*/tests/**/*.rs"))
for path in sorted(paths):
    rel = path.relative_to(ROOT)
    lines = path.read_text(encoding="utf-8").splitlines()
    raw = len(lines)
    if is_test_only(rel):
        test_only.append((raw, rel))
    else:
        prod = production_lines(lines)
        production.append((raw, prod, raw - prod, rel))

violations = []

prod_max = max(production, key=lambda item: item[1], default=(0, 0, 0, Path("-")))
print(
    f"production src: {len(production)} files checked, limit {PRODUCTION_LIMIT}, "
    f"max {prod_max[1]} ({prod_max[3].as_posix()})"
)
watch = sorted((prod, rel) for _, prod, _, rel in production if prod >= 800)
for n, p in watch:
    print(f"  {n:5} {p.as_posix()}")

inline_max = max(production, key=lambda item: item[2], default=(0, 0, 0, Path("-")))
inline_total = sum(item[2] for item in production)
inline_watch = sorted(
    (
        (inline, raw, prod, rel)
        for raw, prod, inline, rel in production
        if inline >= INLINE_WATCH_LIMIT
    ),
    key=lambda item: (-item[0], item[3].as_posix()),
)
print(
    f"inline test in production files: limit {TEST_LIMIT}, "
    f"max {inline_max[2]} ({inline_max[3].as_posix()}), "
    f"{len(inline_watch)} files >= {INLINE_WATCH_LIMIT}, "
    f"{inline_total} inline lines total (inline #[cfg(test)] mod bodies only)"
)
for inline, raw, prod, rel in inline_watch:
    print(
        f"  {inline:5} inline (raw {raw:5} / production {prod:5}) "
        f"{rel.as_posix()}"
    )

test_max = max(test_only, key=lambda item: item[0], default=(0, Path("-")))
print(
    f"test-only files: {len(test_only)} checked, limit {TEST_LIMIT} "
    f"(不在生产行数门禁内), max {test_max[0]} ({test_max[1].as_posix()})"
)
for n, p in sorted(test_only, reverse=True):
    if n > PRODUCTION_LIMIT:
        print(f"  {n:5} {p.as_posix()}")

for raw, prod, inline, p in production:
    if prod > PRODUCTION_LIMIT:
        violations.append(
            ("production", prod, p, f"raw {raw} / inline {inline}")
        )
    if inline > TEST_LIMIT:
        violations.append(
            ("inline-test", inline, p, f"raw {raw} / production {prod}")
        )
    elif raw > TEST_LIMIT:
        violations.append(
            ("total", raw, p, f"production {prod} / inline {inline}")
        )
for n, p in test_only:
    if n > TEST_LIMIT:
        violations.append(("test-only", n, p, "测试专用文件"))

if violations:
    print("error: line-count gate failed:")
    for kind, n, p, detail in violations:
        limit = {
            "production": PRODUCTION_LIMIT,
            "inline-test": TEST_LIMIT,
            "total": TEST_LIMIT,
            "test-only": TEST_LIMIT,
        }[kind]
        print(f"  {p.as_posix()}: {n} > {limit} ({kind}; {detail})")
    sys.exit(1)

print("line-count OK")
PY
