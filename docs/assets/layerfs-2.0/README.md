# LayerFS 2.0 pictures

> **Status:** Proposal; target LayerFS 2.0; not a released contract.

These three pictures illustrate the LayerFS 2.0 tracking issue. They show
planned work: the providers, sandboxes and integrations drawn here are proposed
unless the current release documents them.

| File | What it shows |
| --- | --- |
| `layerfs-2.0-header.svg` | The logo, "LayerFS 2.0" and the tagline: agents' tool call side effects in LayerFS, Git for task checkpoints. 1300×300. |
| `layerfs-2.0-architecture.svg` | pi agents with an API plugin (Node and Python SDKs, CLI, MCP). File calls go straight to the Store; Bash calls run in each agent's sandbox, one Workspace per call. The SQLite Store pushes and pulls to remotes (Google Drive, Cloudflare R2, MinIO, AWS S3), PostgreSQL is a second backend, and finished tasks publish to Git and GitHub. 1300×1030, animated. |
| `layerfs-2.0-commit-history.svg` | A Git-style history that grows over time: agent Branches fork from main and from each other, one Commit per tool call, and finished work merges back as Layers L14 to L17. 1300×640, animated. |

All three are self-contained SVG with no scripts or external files, so they
display as images on GitHub. Text is drawn as outlines of the Geist and Geist
Mono fonts (SIL Open Font License), so the pictures look the same without the
fonts installed. The animations use SVG animation elements; viewers who ask for
reduced motion see the finished picture. Icons come from Lucide (ISC license)
and Simple Icons (CC0).
