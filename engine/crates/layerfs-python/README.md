# LayerFS Python SDK

Local package preparation for the Area P Python client. The distribution name
is `layerfs`; this tree does not publish it. Linux uses the public Rust API in
the calling process. macOS and Windows consume `LAYERFS_WORKER_ENDPOINT` with
the existing authenticated length-prefixed JSON worker protocol. Python
3.10–3.13 is the initial wheel range, built through the ABI3 subset.
