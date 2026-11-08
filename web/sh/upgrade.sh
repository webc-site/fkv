#!/usr/bin/env bash

set -e
DIR=$(realpath $0) && DIR=${DIR%/*}
cd "$DIR/.."

bun i

upgrade() {
  (
    local dir=$1
    set -x
    cd "$dir"
    bun x ncu -u --dep prod,dev,peer
    rm -f bun.lock
    bun i
  )
}

for file in $(git ls-files | grep 'package.json$'); do
  dir=$(dirname "$file")
  upgrade "$dir"
done
