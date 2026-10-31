# SDD ledger — plan: docs/superpowers/plans/2026-10-28-area-p-foundation.md

Pre-flight: Task 1 produces registered resources/images consumed by Tasks 2 and 4; no interface conflict found.
Pre-flight: Task 2 produces allowlisted build operations used optionally by Task 4; CLI remains the authoritative fallback.
Pre-flight: Task 3 preserves sealed bundle interfaces consumed by Task 4; Node behavior must remain compatible.
Ruling: use the fresh dedicated clone on area-p/p1-p15 instead of creating a nested linked worktree — the checkout has no other work and the user approved this branch and immediate execution — cost if wrong: another agent editing this exact clone could collide, so no concurrent editor is allowed.
Ruling: Ubuntu WSL2 is required for the existing launcher because worktree_run.py uses os.uname() — use a dedicated distribution rather than porting the launcher to Windows — cost if wrong: one additional local Linux distribution to maintain.
Task 1: Docker Desktop WSL integration failed; an isolated Ubuntu docker.io install then hit host C: at 0 MB and VHD I/O errors. Moving Ubuntu to E: repeatedly failed ERROR_SHARING_VIOLATION. Do not shut down shared Docker/WSL state without user approval.
Task 1: Ruling: proceed with Task 2's independent MCP unit/stdio tests while the host storage blocker awaits user action — this is still build infrastructure, not Area P product code — cost if wrong: MCP integration must be revalidated after Task 1 resources exist, which is already required by Task 4.
Task 2: command adapter RED→GREEN (6/6); stdio test failed first on the missing server, then exposed unresolved SDK v2 initialization. Task 2 is not complete. User requested an immediate archival push of all plans/history before further debugging.
