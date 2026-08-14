# M8 closeout exit

- M8 status: passed
- Candidate commit: `b8eb6bb623ebfa0448ba96636864b6c33e9052d6`
- Computer candidate: `9a82e2699ec8ac50e4a1652eca08f56babe82196`
- Candidate parent: `fdc76b9afd3eabc53a4c6ed9ac150cee70f72cec`
- Commands: `pnpm check:api`, `pnpm test:m8`, `pnpm test:quick`,
  `npm.cmd test --workspace @cloudflare/computer-rpc`,
  `npm.cmd test --workspace @cloudflare/computerd`,
  `wsl.exe -- bash -lc set -e; printf 'uname=%s\n' "$(uname -srmo)"; test -c /dev/fuse; stat -c 'fuse=%F mode=%a device=%t:%T' /dev/fuse; fusermount3 --version | head -1; node --version`
- FS M8: 40/40; FS quick: 231/231; Computer RPC: 70/70; computerd: 144 passed, 1
  Docker-only skipped.
- FUSE topology: PowerShell -> wsl.exe -> Linux Node/computerd -> /dev/fuse.

Evidence is candidate-bound, log-hashed, and ready for the direct-child evidence commit.
