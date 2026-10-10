# R8b-holder-private: original failure and diagnosis

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

One registered invocation, [042](042-holder-private/result.json), at
registration v3. Verdict **FAIL**, retained as run. It was not repeated: an
unchanged repeat would be a replay of a failed selection.

## What happened

| Step | Original outcome |
| --- | --- |
| Mount Workspace 1 and 2 | both acknowledged Ready |
| Ordinary command in Workspace 1 | exit 0; its descendant (pid 31) holds a working directory in Workspace 1 from a **private user and mount namespace** (`unshare` allowed; holder namespace `mnt:[4026533100]`, command namespace `mnt:[4026532962]`) |
| Observation while held | the daemon's mount table has both rows; `layerfs-mount-1`, `-2`, `layerfs-fence-1`, `-2` exist |
| Normal Unmount of the **sibling**, Workspace 2 | no reply inside the controller's 5 s event stop; the controller ended the runtime process (`SIGKILL`) and stopped the container explicitly |
| Normal Unmount of held Workspace 1 | not attempted (the run had already failed) |

## Diagnosis from the retained receipts

A private mount namespace is a copy of **every** mount visible when it is
made, so the holder's namespace holds a copy of Workspace 2's mount as well as
Workspace 1's, although the holder references nothing inside Workspace 2.

Exploratory receipt [010 private](010-namespace-holder-private/) shows what the
product does with such a copy, on the held Workspace: plain `umount2` in the
daemon's namespace sees no user of its own row and detaches it, the connection
cannot drain because the copy is still mounted, and after 5.0 s the reply is
`Retained(TeardownCustody { stage: Join, detached: true, … })`. Here the same
path was taken by the sibling's Unmount, and the controller's stop for that
event is also 5 s, so the stop preceded the reply. The reply the product would
have sent to this Unmount is therefore **not observed** in this run; `Retained`
at Join is an inference from receipt 010, not a fact of 042.

## What this establishes

- FP-22-FS fails at the actual topology: an ordinary unprivileged command can
  create a mount namespace in the Sandbox, and a normal Unmount is then neither
  the reversible `Busy` nor a complete `Unmounted`.
- New in this run: the effect is not limited to the Workspace the command uses.
  One such command keeps the normal Unmount of a sibling Workspace of the same
  daemon from completing. The exploratory runs had not exercised the sibling.
- Force is unavailable in this topology (no abort control is bound), so there
  is no product exit short of stopping the container.

Not repaired: the candidates (a shared parent mount, denying user-namespace
creation in Sandbox setup, binding the abort control) are topology decisions the
specification forbids taking by assumption. Carried as `PENDING OWNER`; see
[013](013-runtime-confinement-and-holders.md) for the candidates and limits.

Custody: container `71e27756ad4f` is retained, exited, as the evidence of this
failure. The volume was not written by this selection and served the next
registered selection (043) unchanged.
