"""Dependencies for the Rust mutation testing rules"""

load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")
load("//3rdparty/crates:crates.bzl", "crate_repositories")

# buildifier: disable=unnamed-macro
def rust_mutation_dependencies():
    """Declare dependencies needed for mutation testing.

    Returns:
        list[struct(repo=str, is_dev_dep=bool)]: A list of the repositories
        defined by this macro.
    """
    http_archive(
        build_file_content = """\
package(default_visibility = ["//visibility:public"])

exports_files(["Cargo.toml"])

filegroup(
    name = "srcs",
    srcs = glob(["src/**/*.rs"]),
)
""",
        name = "cargo_mutants_src",
        sha256 = "0a505ab4fe1621d778810d025f8843d57e552c6160875618e8a339fb5c2b2326",
        strip_prefix = "cargo-mutants-f9ac4de4abaeec486b2b55d5ed30859ecd9d12b1",
        type = "tar.gz",
        urls = ["https://codeload.github.com/sourcefrog/cargo-mutants/tar.gz/f9ac4de4abaeec486b2b55d5ed30859ecd9d12b1"],
    )

    direct_deps = []
    direct_deps.append(struct(repo = "cargo_mutants_src", is_dev_dep = False))
    direct_deps.extend(crate_repositories())
    return direct_deps
