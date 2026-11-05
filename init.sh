#!/usr/bin/env bash

DIR=$(realpath $0) && DIR=${DIR%/*}
cd "$DIR"
set -ex

# `sh` 是作者本机工具链（cargo_sh）的符号链接，不随仓库分发。
# 新 clone 会得到一个悬空链接；必须删掉重建，否则 `ln -s` 会因路径已存在失败。
if [ ! -d sh ]; then
  rm -f sh
  ln -s "$HOME/.local/share/cargo_sh" sh
fi

if [ -x ./sh/init.sh ]; then
  ./sh/init.sh
else
  echo "warning: $HOME/.local/share/cargo_sh 不存在，跳过 ./sh/init.sh；构建不依赖它" >&2
fi
