#!/usr/bin/env bash

set -euo pipefail

docker run -it --rm --env CHAINDASH_ASCII_COUNTRIES=1 \
    registry.cn-shenzhen.aliyuncs.com/platon-dev/platone:chaindash \
    chaindash --url "$1"
