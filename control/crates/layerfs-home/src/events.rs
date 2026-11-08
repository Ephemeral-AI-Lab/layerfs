//! A running call's one event stream (O22): progress, a worker's events and
//! a command's output. The entities emit through it; the call surface
//! (`layerfs-api`) gives each call one (layerfs-project#107).
use layerfs_bridge::control::SdkValue;

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
