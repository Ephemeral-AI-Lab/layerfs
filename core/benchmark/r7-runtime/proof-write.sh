set -e
umask 022
mkdir .r7-binary-proof
mkdir .r7-binary-proof/.git
mkdir .r7-binary-proof/node_modules
mkdir .r7-binary-proof/output
printf 'ignored.bin\nnode_modules/\n' > .r7-binary-proof/.gitignore
printf 'complete index\000\377' > .r7-binary-proof/.git/index
printf 'unregistered external Bash bytes\n' > .r7-binary-proof/file
chmod 640 .r7-binary-proof/file
ln .r7-binary-proof/file .r7-binary-proof/alias
printf '\000\377ignored content' > .r7-binary-proof/ignored.bin
printf 'module.exports = 42;\n' > .r7-binary-proof/node_modules/index.js
printf 'captured output\n' > .r7-binary-proof/output/result
ln -s node_modules .r7-binary-proof/dependency
mv .r7-binary-proof/output/result .r7-binary-proof/output/renamed
printf 'R7_BINARY_MUTATION_COMPLETE\n'
