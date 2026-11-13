# Page templates

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

| Template | Copy to | Use for | Size |
| --- | --- | --- | --- |
| [topic.md](topic.md) | `<layer>/<topic>/README.md` | a topic's one main page | about 300 lines at most |
| [page.md](page.md) | `<layer>/<topic>/<page>.md` | an extra page for one code directory, crate or backend | about 250 lines at most |

Links in the templates resolve once the file sits at its target path. The rules
these templates follow are in [How these pages work](../README.md#how-these-pages-work).
