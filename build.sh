#!/usr/bin/env bash

set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
output_dir="${project_dir}/target/docker"

mkdir -p "${output_dir}"

docker buildx build \
    --output "type=local,dest=${output_dir}" \
    "${project_dir}"

echo "编译完成：${output_dir}/chaindash"
