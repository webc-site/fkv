#!/usr/bin/env bash

set -e
DIR=$(realpath $0) && DIR=${DIR%/*}
cd $DIR
./test.sh
set -x
bun x jscpd . --ignore "**/tests/**" --pattern "**/*.js" -r console-full
bun x minify_size stream.js
