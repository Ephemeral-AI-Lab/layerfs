# LayerFS official release assets

> **Status:** Current general guide.

This folder holds the LayerFS intro video, the recording of the website's page 3 demo, and
renders of the six core-idea scenes. This guide gives the video's script and explains how
each asset was made.

## Files

| File | What it is |
| --- | --- |
| `layerfs-intro.mp4` | The intro video: 1920×1080, 30 fps, H.264 (CRF 18), 57.9 s, silent. |
| `page3-demo.mp4` | The website's page 3 demo filmed in max mode (10×): 1440×900, 30 fps, 75 s (plus a short hold on the last frame). |
| `page3-demo.gif` | The same film at 3× playback: 960×600, 10 fps, about 26 s. `README.md` shows it. |
| `idea-0N-*.gif` | Each core-idea scene, one full loop: 1200×440, 10 fps. |
| `idea-0N-*.png` | One full-content frame of each scene: 1500×550. |

The look matches the README art: IBM Plex Sans and IBM Plex Mono, warm off-white `#FDFCFA`,
ink `#1E2533`, brand blue `#2F5BB5` and light blue `#93B1E0`. An 8 px blue progress bar runs
along the bottom of the video.

## Intro video script

| Time (s) | Section | On screen |
| --- | --- | --- |
| 0.0–2.5 | Opening | The LayerFS mark scales in, then "LayerFS", "Ephemeral Workspaces. **Durable Shared History.**" and "The substrate under your agents." |
| 2.5–16.3 | Carousel A: core ideas 1–3 | Header "WHY LAYERFS · Ten ideas, one filesystem." Each card holds 4.6 s and the next slides in over 0.5 s, with neighbours peeking and dots below. |
| 16.3–18.3 | Transition | "THREE MORE CORE IDEAS" / **"More than that."** |
| 18.3–35.5 | Carousel B: core ideas 4–6, then supporting | Ideas 4–6 at 4.6 s each, then a 3.4 s "Four supporting ideas" card (07–10). |
| 35.5–55.25 | Demo: "See it run" | Film seconds 0–37.5 at 2× playback, with five captions. Tag: "simulated · max mode (10×) · 2× playback". |
| 55.25–57.85 | Close | Dark ground: "COMING / Oct 21", the LayerFS × DSH lockup, `github.com/Ephemeral-AI-Lab/layerfs`. |

### Core idea cards

Each card shows an icon, title, number and claim (from the website's `ideas.ts`). Below
them, the animated scene sits on the left and the "HOW IT WORKS" bullets on the right. The
bullets come from the website's docs Core ideas page (`IdeaProse.astro`): the bold leads are
verbatim and the sentences are shortened. Each bullet fades in 0.45 s after the one before,
starting 0.35 s after the card arrives. Each scene's loop is compressed into the card's stay,
at up to 4× speed, and ends on its fullest frame.

1. **Ephemeral Computing**: "Compute exists only while a command runs."
   - **File calls need no computer.** fs calls run in your own process, straight against the Store. A write is one Commit.
   - **Bash calls get a fresh Workspace.** Mount the Branch, run the command, commit what changed, unmount.
   - **Nothing happens on its own.** LayerFS never commits, times out or unmounts by itself.
2. **Decentralized Agents**: "Agents on different computers share one serverless Store."
   - **One Sandbox per agent.** Each agent's container runs its own LayerFS daemon.
   - **The SDK opens it too.** fs, branch and project calls run in your process against the same file.
   - **One writer at a time, briefly.** Writers take turns through a short publish step. Reads never wait.
3. **Multiagent Coordination**: "One project, many branches. One branch, many agents."
   - **Different files** land on top of the newer head as Committed.
   - **The same file** is refused as Conflict. Nothing is committed.
   - **In a Workspace,** a conflict writes marker blocks into the files.
   - **Blame and diffs** name the Commit, and so the tool call, behind every line.
4. **Recursive Filesystem**: "Fork recursively any commit, at any depth."
   - **A fork is one record.** Its files are the start's files until it changes them.
   - **Changes are stored once.** A losing fork costs only the bytes it changed.
   - **Keep the winner.** branch.merge folds it back; branch.publish makes it main's next Layer.
5. **Ultra Storage Optimization**: "10 copies compress into 0.31."
   - **Content addressing.** Every object is named by the hash of its bytes, so identical content is stored once.
   - **Chunking.** Large files are cut by content into chunks, so an edit changes only nearby chunks.
   - **Deltas and compression.** Deltas against the last version, packed and compressed with zstd.
   - **Measured per Store.** store.get --with stats reports both ratios: dedupe and compression.
6. **Instant Bootstrap**: "Dependencies come with every mount. No wasting time for re-installation."
   - **Init takes everything, once.** Source, .git, dependencies, caches and every ignored file.
   - **A mount binds the whole tree.** No scan, copy or install.
   - **Content loads on demand.** Files are read from the Store as they're opened.
   - **Installs are just changes.** Commit one, and every later mount has it.

The supporting card (07–10) lists Auto Checkpoints, Append-only Objects, Conflict Rules
(`.lww`) and Big File, Small Edit, each with its one-line claim from the README.

### Demo captions

Captions are timed to the film. Each fades over 0.3 s, on a 50% ink background with a text
shadow so the demo shows through.

| Film (s) | Video (s) | Caption |
| --- | --- | --- |
| 0–6.5 | 35.9–39.2 | The orchestrator forks Branches from a read-only main: cart, search, auth. |
| 6.5–13.5 | 39.2–42.7 | Each Bash call mounts its own Workspace in the Branch's Sandbox. fs calls commit straight to the Store. |
| 13.5–21 | 42.7–46.4 | Every tool call becomes a Commit, a leaf on its Branch. Forks of forks go deeper: cart.1, cart.1.1. |
| 21–29 | 46.4–50.4 | The best Branch is published to main as a new Layer. Weak ones stop; nested ones merge back. |
| 29–37.5 | 50.4–55.25 | History keeps growing: main reaches L16 with over 170 Commits. |

## Sources

| Asset | Source |
| --- | --- |
| LayerFS mark | `docs/assets/readme/layerfs-mark.svg` (inlined) |
| Idea icons | The pixel paths in the website's `ideas.ts`, the same as `docs/assets/readme/icon-*.svg` |
| Idea scenes | `docs/assets/readme/idea-0N-*.svg`: SMIL animations identical to the website's page 3 scenes |
| Card copy | Website (`layerfs-website` repository) `src/components/main/page3/ideas.ts` and `src/components/docs/IdeaProse.astro` |
| Demo film | Page 3 (`#page-3`, `OrchestrationDemo`) of the `layerfs-website` build |
| Fonts | Google Fonts: IBM Plex Sans 400–700, IBM Plex Mono 400/500 |

The page 3 demo is the website's own simulation (`src/components/demo/sim.js`). Its
Commits, Layers and store figures are illustrative, not measurements, which is why the video
labels it "simulated".

## How the assets were made

All rendering ran in headless Google Chrome, driven over the Chrome DevTools Protocol from
Node 24. ffmpeg did the encoding.

1. **The page 3 film.** The website's `dist/` build was served locally and opened at
   1440×900. The script scrolled to `#page-3`, clicked the 10× ("max mode") button and
   recorded Chrome's screencast for 75 s: about 58 fps with real frame timestamps, from an
   empty project to main at L32 with 360 Commits. ffmpeg's concat demuxer turned the
   frames, at their real durations, into a 30 fps MP4. The GIF is the same film at 3×
   playback with a 96-colour palette.
2. **The idea scenes.** Each SVG was inlined in a page, its SMIL clock paused and stepped
   with `setCurrentTime()`, and every frame captured. A static rasterizer would drop the
   animated content.
3. **The intro video.** One 1920×1080 page holds every section and exposes `seek(t)`, which
   sets the whole frame for time `t`:
   - section fades and entrance moves;
   - the carousel's position, card scaling and dimming, bullet reveals and dots;
   - each scene's SMIL time;
   - the demo video's `currentTime`, loaded as a blob URL so it can seek on a plain static server;
   - caption opacity and the progress bar.

   A CDP script stepped `seek(i / 30)` for every frame and saved a screenshot; four
   workers rendered in parallel. ffmpeg encoded the frames:

   ```bash
   ffmpeg -framerate 30 -i intro-frames/f%04d.jpg -c:v libx264 -preset slow -crf 18 -pix_fmt yuv420p -movflags +faststart layerfs-intro.mp4
   ```

The timing constants are the section lengths (2.5 s opening, 4.6 s per core card, 0.5 s
slides, 2.0 s transition, 3.4 s supporting card, 2.6 s close) and the demo playback
(film 0–37.5 s at 2×, with a 0.4 s lead-in and a 0.6 s hold). They and the caption list
are the levers for any re-cut. The build scripts are not in this repository.
