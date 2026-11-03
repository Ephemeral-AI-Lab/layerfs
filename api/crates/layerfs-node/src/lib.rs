//! The Node front end of the `layerfs` package (#533 step 14): one entry,
//! [`call`], that makes exactly the library's `calls::call` on a native
//! worker thread; [`table`], the call table the package builds its nine
//! families from; and [`doctor`], the top-level `layerfs.doctor()`.
//! Arguments, results and events cross as JSON text, bytes as
//! `{ "base64": … }`. The Promise always resolves, to `{ events, answer }`
//! with `answer` `{ value }` or `{ refused: { code, message, detail } }`.
//! Nothing is retried here.
mod call;

pub use call::{call, doctor, table, Call, Doctor};
