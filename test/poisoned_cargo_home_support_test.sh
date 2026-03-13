#!/usr/bin/env bash

set -euo pipefail

cd "${TEST_SRCDIR}/${TEST_WORKSPACE}"

poison_dir="${TEST_TMPDIR}/poisoned-cargo-home"
mkdir -p "${poison_dir}"
cat > "${poison_dir}/config.toml" <<'EOF'
[broken
EOF

export CARGO_HOME="${poison_dir}"

set +e
output="$(./test/example_mutation_test 2>&1)"
status=$?
set -e

if [[ "${status}" -ne 0 ]]; then
  echo "Expected mutation target to ignore poisoned ambient CARGO_HOME, got status ${status}."
  echo "${output}"
  exit 1
fi

if ! grep -q "Mutation Testing Summary" <<<"${output}"; then
  echo "Expected mutation summary output."
  echo "${output}"
  exit 1
fi
