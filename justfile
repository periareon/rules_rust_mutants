set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    just --list

ci:
    bazel test --config=strict //...
    buildifier -lint=warn -mode=check -warnings=all -r .

check:
    bazel test --config=strict //...

buildifier-check:
    buildifier -lint=warn -mode=check -warnings=all -r .

buildifier-fix:
    buildifier -mode=fix -warnings=all -r .

rustfmt-check:
    bazel build --config=rustfmt //...

rustfmt-fix:
    files=(); while IFS= read -r -d '' file; do files+=("$file"); done < <(find . -type f -name '*.rs' -not -path './.git/*' -not -path './bazel-*/*' -not -path './external/*' -not -path './target/*' -print0); if (( ${#files[@]} == 0 )); then exit 0; fi; bazel run @rules_rust//tools/upstream_wrapper:rustfmt -- "${files[@]}"

fmt: buildifier-fix rustfmt-fix
