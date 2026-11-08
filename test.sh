#!/usr/bin/env bash

set -e
DIR=$(realpath $0) && DIR=${DIR%/*}
cd $DIR
set -x
# 依赖方向门禁：层表见 doc/architecture/01-overview.md。
bash scripts/ci/check_layers.sh
# --all-features
cargo nextest run --failure-output immediate --status-level slow --final-status-level slow --show-progress=none
