set -e
test -d .r7-binary-proof/.git
test -d .r7-binary-proof/node_modules
test -d .r7-binary-proof/output
cmp .r7-binary-proof/.gitignore <(printf 'ignored.bin\nnode_modules/\n')
cmp .r7-binary-proof/.git/index <(printf 'complete index\000\377')
cmp .r7-binary-proof/file <(printf 'unregistered external Bash bytes\n')
cmp .r7-binary-proof/alias <(printf 'unregistered external Bash bytes\n')
cmp .r7-binary-proof/ignored.bin <(printf '\000\377ignored content')
cmp .r7-binary-proof/node_modules/index.js <(printf 'module.exports = 42;\n')
cmp .r7-binary-proof/output/renamed <(printf 'captured output\n')
test ! -e .r7-binary-proof/output/result
test "$(readlink .r7-binary-proof/dependency)" = node_modules
test "$(stat -c '%i' .r7-binary-proof/file)" = "$(stat -c '%i' .r7-binary-proof/alias)"
test "$(stat -c '%a %h %u:%g' .r7-binary-proof/file)" = "640 2 $(id -u):$(id -g)"
test "$(find .r7-binary-proof -mindepth 1 | wc -l)" -eq 11
printf 'R7_BINARY_SURVIVAL_ORACLE paths=12 regular_files=7 full_regular_bytes=162 hardlinks=true symlinks=true scoped=true\n'
