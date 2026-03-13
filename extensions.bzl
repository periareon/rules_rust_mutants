"""Bzlmod module extensions for rules_rust_mutation"""

load("@bazel_features//:features.bzl", "bazel_features")
load("//:repositories.bzl", "rust_mutation_dependencies")

def _rust_ext_impl(module_ctx):
    direct_deps = []
    direct_deps.extend(rust_mutation_dependencies())

    metadata_kwargs = {
        "root_module_direct_deps": [repo.repo for repo in direct_deps],
        "root_module_direct_dev_deps": [],
    }

    if bazel_features.external_deps.extension_metadata_has_reproducible:
        metadata_kwargs["reproducible"] = True

    return module_ctx.extension_metadata(**metadata_kwargs)

rust_ext = module_extension(
    doc = "Dependencies for the rules_rust_mutation extension.",
    implementation = _rust_ext_impl,
)
