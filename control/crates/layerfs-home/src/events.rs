//! A running call's one event stream (O22): progress, a worker's events and
//! a command's output. The entities emit through it; the call surface
//! (`layerfs-api`) gives each call one (layerfs-project#107).
use layerfs_protocol::sdk::SdkValue;

/// A call's one event stream.
pub struct Events<'a> {
    sink: &'a mut dyn FnMut(SdkValue),
}

impl<'a> Events<'a> {
    /// The stream that sends each event to `sink`.
    pub fn new(sink: &'a mut dyn FnMut(SdkValue)) -> Self {
        Events { sink }
    }
}

impl Events<'_> {
    /// Sends one progress update, `{ progress: value }`.
    pub fn progress(&mut self, value: SdkValue) {
        (self.sink)(SdkValue::map([("progress", value)]));
    }

    /// Sends one progress event of the public shape, `{ phase, done, total }`
    /// (call surface 2.0), by the same route as [`Self::progress`]: `done`
    /// and `total` are null while they are not known, and `extra` keys go
    /// beside the three (a doctor's `node`).
    pub fn phase(
        &mut self,
        phase: &str,
        done: Option<i64>,
        total: Option<i64>,
        extra: Vec<(&str, SdkValue)>,
    ) {
        let count = |n: Option<i64>| n.map_or(SdkValue::Null, SdkValue::Int);
        let mut fields = vec![
            ("phase".to_owned(), SdkValue::text(phase)),
            ("done".to_owned(), count(done)),
            ("total".to_owned(), count(total)),
        ];
        fields.extend(
            extra
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value)),
        );
        self.progress(SdkValue::Map(fields));
    }

    /// Passes on one event a worker sent, as it came.
    pub fn event(&mut self, event: SdkValue) {
        (self.sink)(event);
    }

    /// Sends one piece of a command's output, `{ stream, bytes }`.
    pub fn output(&mut self, stream: &str, bytes: &[u8]) {
        (self.sink)(SdkValue::map([
            ("stream", SdkValue::text(stream)),
            ("bytes", SdkValue::Bytes(bytes.to_vec())),
        ]));
    }
}
