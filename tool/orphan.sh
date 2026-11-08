#!/usr/bin/env bash

# 孤儿 crate 守卫：生产依赖图（非 dev、非 build）里没有调用者的 workspace 成员必须显式登记。
# 判据：从顶层交付物（facade / bin）出发不可达的 crate 即孤儿。
# 直连孤儿（没有任何生产消费者）直接失败；传递孤儿（消费者本身也是孤儿）只提示，
# 避免整条未接线的运行时栈互相带出噪音。
# 白名单 tool/orphan_allow.tsv 每行 `<crate>\t<原因>`。
# 白名单项一旦重新被生产路径引用，同样报错，防止长期挂着。
# 退出码: 0 干净 / 2 有发现 / 1 环境错误

set -euo pipefail

DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$DIR"

ALLOW_FILE="tool/orphan_allow.tsv"
# 顶层交付物：无上游消费者也合法。
ROOT_LI=(fkv fkv_sdk fkv_testkit fkv_bench fkv_cli)

command -v jq >/dev/null 2>&1 || {
  echo "缺少 jq" >&2
  exit 1
}

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT

# 行格式: A<crate> / R<root> / E<from><to>，全部 \t 分隔
jq -r --arg roots "${ROOT_LI[*]}" '
  .packages as $p
  | [ $p[] | select(.source == null) | .name ] as $ws
  | [ $p[]
      | . as $pkg
      | $pkg.dependencies[]?
      | select(.kind == null)
      | .name as $n
      | select($ws | index($n))
      | "E\t\($pkg.name)\t\($n)" ] as $edges
  | ([ $ws[] | "A\t\(.)" ] + $edges + [ ($roots | split(" "))[] | "R\t\(.)" ])
  | .[]' <<<"$(cargo metadata --no-deps --format-version 1)" >"$tmp"

dead_all=$(awk -F'\t' '
  $1 == "A" { all[$2] = 1; next }
  $1 == "R" { root[$2] = 1; next }
  $1 == "E" { m++; frm[m] = $2; to[m] = $3; direct[$3]++; next }
  END {
    for (x in all) live[x] = 1
    # 反向叶剪枝：消费者的消费者都死了，自己也是孤儿。
    do {
      changed = 0
      for (x in all) {
        if (!live[x] || root[x]) continue
        alive = 0
        for (i = 1; i <= m; i++) {
          if (to[i] == x && live[frm[i]]) { alive = 1; break }
        }
        if (!alive) { live[x] = 0; dead[x] = 1; changed = 1 }
      }
    } while (changed)
    for (x in dead) print (direct[x] ? "T\t" : "L\t") x
  }' "$tmp" | sort)

# comm 要求两侧都按字节序排好；这里必须重新 sort。
dead_li=$(printf '%s\n' "$dead_all" | awk -F'\t' 'NF { print $2 }' | sed '/^$/d' | sort)
leaf_li=$(printf '%s\n' "$dead_all" | awk -F'\t' '$1 == "L" { print $2 }' | sed '/^$/d' | sort)
trans_li=$(printf '%s\n' "$dead_all" | awk -F'\t' '$1 == "T" { print $2 }' | sed '/^$/d' | sort)

allow_line_li=$(grep -v '^#' "$ALLOW_FILE" | awk -F'\t' 'NF' || true)
allow_li=$(printf '%s\n' "$allow_line_li" | awk -F'\t' 'NF { print $1 }' | sort)

new_li=$(comm -23 <(printf '%s\n' "$leaf_li") <(printf '%s\n' "$allow_li"))
stale_li=$(comm -13 <(printf '%s\n' "$dead_li") <(printf '%s\n' "$allow_li"))
trans_new_li=$(comm -23 <(printf '%s\n' "$trans_li") <(printf '%s\n' "$allow_li"))

code=0
if [ -n "$new_li" ]; then
  echo ">>> 未登记的孤儿 crate：无任何生产消费者（接线、删除，或加白名单并写明原因）" >&2
  while read -r crate; do
    [ -n "$crate" ] && echo "  $crate" >&2
  done <<<"$new_li"
  code=2
fi

if [ -n "$trans_new_li" ]; then
  echo ">>> 传递孤儿（消费者本身也是孤儿，随上游接线/删除一并消失）" >&2
  while read -r crate; do
    [ -n "$crate" ] && echo "  $crate" >&2
  done <<<"$trans_new_li"
fi

if [ -n "$stale_li" ]; then
  echo ">>> 白名单已过期：这些 crate 已有生产消费者，请从 $ALLOW_FILE 删除" >&2
  while read -r crate; do
    [ -n "$crate" ] && echo "  $crate" >&2
  done <<<"$stale_li"
  code=2
fi

if [ "$code" -eq 0 ]; then
  echo "孤儿 crate 守卫通过（白名单 $(printf '%s\n' "$allow_li" | sed '/^$/d' | wc -l | tr -d ' ') 项）"
fi
exit "$code"
