# doctor calls

> **Status:** Current general guide for LayerFS 2.0 (#533). Part of the
> `layerfs-js-sdk` skill. `doctor` is the report function, outside the table;
> `doctor.fix` is the family's one call. A folder scope refuses it.

```js
doctor.fix(id)
```

`layerfs doctor` reports what is wrong in the home as
`{ ok, findings: [{ severity, entity: { kind, name }, message, fix }] }`, each
finding naming the call that fixes it. It changes nothing and starts no
machine. `doctor.fix(id)` removes the one leftover `id` names.

## `doctor.fix`

`doctor.fix(id)`: `id` is a leftover's `<machine>:<kind>:<name>` from the
report; it never holds a path. One attempt: nothing is retried, and nothing
but the one entry is removed. The kinds are `run-folder`, `empty-root`,
`init-copy`, `backup-staging`, `image-tmp` and `image-part`.

```json
{"id":"native:run-folder:web","machine":"native","kind":"run-folder",
 "path":"/Users/me/.layerfs/machine/native/run/web","removed":true,"bytes":11833344}
```

`bytes` is what the removed files held, `null` for a folder copy and an image
temporary. Errors: `NotFound` (no leftover has that id now), `InUse` (the
kind's guard does not hold; nothing removed), `InvalidInput` (a kind that is
only reported, a missing `id`, a folder scope), `Unavailable` (the machine is
stopped), `Unknown` (the worker's answer was lost), `Failed`.
