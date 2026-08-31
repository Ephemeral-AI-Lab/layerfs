use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FuseWriteMetrics {
    pub max_write_bytes: u64,
    pub kernel_write_requests: u64,
    pub kernel_write_bytes: u64,
    pub kernel_write_le_4k: u64,
    pub kernel_write_le_64k: u64,
    pub kernel_write_le_256k: u64,
    pub kernel_write_le_1m: u64,
    pub kernel_write_gt_1m: u64,
    pub client_request_copy_bytes: u64,
    pub frame_payload_copy_bytes: u64,
    pub client_frame_bytes: u64,
    pub encode_ns: u64,
    pub socket_write_ns: u64,
    pub host_frame_bytes: u64,
    pub socket_read_ns: u64,
    pub decode_ns: u64,
    pub host_decode_copy_bytes: u64,
    pub host_dispatch_ns: u64,
}

impl FuseWriteMetrics {
    const FIELD_COUNT: usize = 18;

    pub(crate) fn merge(&mut self, other: Self) {
        self.max_write_bytes = self.max_write_bytes.max(other.max_write_bytes);
        let left = self.fields_mut();
        let right = other.fields();
        for (index, value) in right.into_iter().enumerate().skip(1) {
            *left[index] = left[index].saturating_add(value);
        }
    }

    pub(crate) fn write_to(self, output: &mut impl Write) -> std::io::Result<()> {
        for value in self.fields() {
            output.write_all(&value.to_be_bytes())?;
        }
        Ok(())
    }

    pub(crate) fn read_from(input: &mut impl Read) -> std::io::Result<Self> {
        let mut fields = [0_u64; Self::FIELD_COUNT];
        for field in &mut fields {
            let mut bytes = [0; 8];
            input.read_exact(&mut bytes)?;
            *field = u64::from_be_bytes(bytes);
        }
        Ok(Self::from_fields(fields))
    }

    fn fields(self) -> [u64; Self::FIELD_COUNT] {
        [
            self.max_write_bytes,
            self.kernel_write_requests,
            self.kernel_write_bytes,
            self.kernel_write_le_4k,
            self.kernel_write_le_64k,
            self.kernel_write_le_256k,
            self.kernel_write_le_1m,
            self.kernel_write_gt_1m,
            self.client_request_copy_bytes,
            self.frame_payload_copy_bytes,
            self.client_frame_bytes,
            self.encode_ns,
            self.socket_write_ns,
            self.host_frame_bytes,
            self.socket_read_ns,
            self.decode_ns,
            self.host_decode_copy_bytes,
            self.host_dispatch_ns,
        ]
    }

    fn fields_mut(&mut self) -> [&mut u64; Self::FIELD_COUNT] {
        [
            &mut self.max_write_bytes,
            &mut self.kernel_write_requests,
            &mut self.kernel_write_bytes,
            &mut self.kernel_write_le_4k,
            &mut self.kernel_write_le_64k,
            &mut self.kernel_write_le_256k,
            &mut self.kernel_write_le_1m,
            &mut self.kernel_write_gt_1m,
            &mut self.client_request_copy_bytes,
            &mut self.frame_payload_copy_bytes,
            &mut self.client_frame_bytes,
            &mut self.encode_ns,
            &mut self.socket_write_ns,
            &mut self.host_frame_bytes,
            &mut self.socket_read_ns,
            &mut self.decode_ns,
            &mut self.host_decode_copy_bytes,
            &mut self.host_dispatch_ns,
        ]
    }

    fn from_fields(fields: [u64; Self::FIELD_COUNT]) -> Self {
        Self {
            max_write_bytes: fields[0],
            kernel_write_requests: fields[1],
            kernel_write_bytes: fields[2],
            kernel_write_le_4k: fields[3],
            kernel_write_le_64k: fields[4],
            kernel_write_le_256k: fields[5],
            kernel_write_le_1m: fields[6],
            kernel_write_gt_1m: fields[7],
            client_request_copy_bytes: fields[8],
            frame_payload_copy_bytes: fields[9],
            client_frame_bytes: fields[10],
            encode_ns: fields[11],
            socket_write_ns: fields[12],
            host_frame_bytes: fields[13],
            socket_read_ns: fields[14],
            decode_ns: fields[15],
            host_decode_copy_bytes: fields[16],
            host_dispatch_ns: fields[17],
        }
    }
}

#[derive(Default)]
pub(crate) struct AtomicFuseWriteMetrics {
    max_write_bytes: AtomicU64,
    kernel_write_requests: AtomicU64,
    kernel_write_bytes: AtomicU64,
    kernel_write_le_4k: AtomicU64,
    kernel_write_le_64k: AtomicU64,
    kernel_write_le_256k: AtomicU64,
    kernel_write_le_1m: AtomicU64,
    kernel_write_gt_1m: AtomicU64,
    client_request_copy_bytes: AtomicU64,
    frame_payload_copy_bytes: AtomicU64,
    client_frame_bytes: AtomicU64,
    encode_ns: AtomicU64,
    socket_write_ns: AtomicU64,
    host_frame_bytes: AtomicU64,
    socket_read_ns: AtomicU64,
    decode_ns: AtomicU64,
    host_decode_copy_bytes: AtomicU64,
    host_dispatch_ns: AtomicU64,
}

impl AtomicFuseWriteMetrics {
    pub(crate) fn note_max_write(&self, bytes: u64) {
        self.max_write_bytes.fetch_max(bytes, Ordering::Relaxed);
    }

    pub(crate) fn note_kernel_write(&self, bytes: u64) {
        self.kernel_write_requests.fetch_add(1, Ordering::Relaxed);
        self.kernel_write_bytes.fetch_add(bytes, Ordering::Relaxed);
        let bucket = if bytes <= 4 * 1024 {
            &self.kernel_write_le_4k
        } else if bytes <= 64 * 1024 {
            &self.kernel_write_le_64k
        } else if bytes <= 256 * 1024 {
            &self.kernel_write_le_256k
        } else if bytes <= 1024 * 1024 {
            &self.kernel_write_le_1m
        } else {
            &self.kernel_write_gt_1m
        };
        bucket.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_client_copy(&self, bytes: u64) {
        self.client_request_copy_bytes
            .fetch_add(bytes, Ordering::Relaxed);
    }

    pub(crate) fn note_client_frame(
        &self,
        frame_bytes: u64,
        payload_copy_bytes: u64,
        encode_ns: u64,
        socket_write_ns: u64,
    ) {
        self.client_frame_bytes
            .fetch_add(frame_bytes, Ordering::Relaxed);
        self.frame_payload_copy_bytes
            .fetch_add(payload_copy_bytes, Ordering::Relaxed);
        self.encode_ns.fetch_add(encode_ns, Ordering::Relaxed);
        self.socket_write_ns
            .fetch_add(socket_write_ns, Ordering::Relaxed);
    }

    pub(crate) fn note_host_frame(
        &self,
        frame_bytes: u64,
        payload_copy_bytes: u64,
        socket_read_ns: u64,
        decode_ns: u64,
    ) {
        self.host_frame_bytes
            .fetch_add(frame_bytes, Ordering::Relaxed);
        self.host_decode_copy_bytes
            .fetch_add(payload_copy_bytes, Ordering::Relaxed);
        self.socket_read_ns
            .fetch_add(socket_read_ns, Ordering::Relaxed);
        self.decode_ns.fetch_add(decode_ns, Ordering::Relaxed);
    }

    pub(crate) fn note_host_dispatch(&self, elapsed_ns: u64) {
        self.host_dispatch_ns
            .fetch_add(elapsed_ns, Ordering::Relaxed);
    }

    pub(crate) fn take(&self) -> FuseWriteMetrics {
        FuseWriteMetrics {
            max_write_bytes: self.max_write_bytes.load(Ordering::Relaxed),
            kernel_write_requests: self.kernel_write_requests.swap(0, Ordering::Relaxed),
            kernel_write_bytes: self.kernel_write_bytes.swap(0, Ordering::Relaxed),
            kernel_write_le_4k: self.kernel_write_le_4k.swap(0, Ordering::Relaxed),
            kernel_write_le_64k: self.kernel_write_le_64k.swap(0, Ordering::Relaxed),
            kernel_write_le_256k: self.kernel_write_le_256k.swap(0, Ordering::Relaxed),
            kernel_write_le_1m: self.kernel_write_le_1m.swap(0, Ordering::Relaxed),
            kernel_write_gt_1m: self.kernel_write_gt_1m.swap(0, Ordering::Relaxed),
            client_request_copy_bytes: self.client_request_copy_bytes.swap(0, Ordering::Relaxed),
            frame_payload_copy_bytes: self.frame_payload_copy_bytes.swap(0, Ordering::Relaxed),
            client_frame_bytes: self.client_frame_bytes.swap(0, Ordering::Relaxed),
            encode_ns: self.encode_ns.swap(0, Ordering::Relaxed),
            socket_write_ns: self.socket_write_ns.swap(0, Ordering::Relaxed),
            host_frame_bytes: self.host_frame_bytes.swap(0, Ordering::Relaxed),
            socket_read_ns: self.socket_read_ns.swap(0, Ordering::Relaxed),
            decode_ns: self.decode_ns.swap(0, Ordering::Relaxed),
            host_decode_copy_bytes: self.host_decode_copy_bytes.swap(0, Ordering::Relaxed),
            host_dispatch_ns: self.host_dispatch_ns.swap(0, Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FuseReadMetrics {
    pub max_readahead_bytes: u64,
    pub init_capabilities: u64,
    pub kernel_read_requests: u64,
    pub kernel_read_bytes: u64,
    pub kernel_read_le_4k: u64,
    pub kernel_read_le_64k: u64,
    pub kernel_read_le_256k: u64,
    pub kernel_read_le_1m: u64,
    pub kernel_read_gt_1m: u64,
    pub read_ahead_hits: u64,
    pub read_ahead_misses: u64,
    pub read_ahead_fetches: u64,
    pub read_ahead_requested_bytes: u64,
    pub read_ahead_fetched_bytes: u64,
    pub read_ahead_served_bytes: u64,
    pub read_ahead_unused_bytes: u64,
    pub read_ahead_cache_copy_bytes: u64,
    pub host_response_frames: u64,
    pub host_response_bytes: u64,
    pub host_response_copy_bytes: u64,
    pub host_encode_ns: u64,
    pub host_socket_write_ns: u64,
    pub client_response_frames: u64,
    pub client_response_bytes: u64,
    pub client_socket_read_ns: u64,
    pub client_decode_ns: u64,
    pub client_decode_copy_bytes: u64,
    pub host_dispatch_ns: u64,
}

impl FuseReadMetrics {
    const FIELD_COUNT: usize = 28;

    pub(crate) fn merge(&mut self, other: Self) {
        self.max_readahead_bytes = self.max_readahead_bytes.max(other.max_readahead_bytes);
        self.init_capabilities |= other.init_capabilities;
        let left = self.fields_mut();
        for (index, value) in other.fields().into_iter().enumerate().skip(2) {
            *left[index] = left[index].saturating_add(value);
        }
    }

    pub(crate) fn write_to(self, output: &mut impl Write) -> std::io::Result<()> {
        for value in self.fields() {
            output.write_all(&value.to_be_bytes())?;
        }
        Ok(())
    }

    pub(crate) fn read_from(input: &mut impl Read) -> std::io::Result<Self> {
        let mut fields = [0_u64; Self::FIELD_COUNT];
        for field in &mut fields {
            let mut bytes = [0; 8];
            input.read_exact(&mut bytes)?;
            *field = u64::from_be_bytes(bytes);
        }
        Ok(Self::from_fields(fields))
    }

    fn fields(self) -> [u64; Self::FIELD_COUNT] {
        [
            self.max_readahead_bytes,
            self.init_capabilities,
            self.kernel_read_requests,
            self.kernel_read_bytes,
            self.kernel_read_le_4k,
            self.kernel_read_le_64k,
            self.kernel_read_le_256k,
            self.kernel_read_le_1m,
            self.kernel_read_gt_1m,
            self.read_ahead_hits,
            self.read_ahead_misses,
            self.read_ahead_fetches,
            self.read_ahead_requested_bytes,
            self.read_ahead_fetched_bytes,
            self.read_ahead_served_bytes,
            self.read_ahead_unused_bytes,
            self.read_ahead_cache_copy_bytes,
            self.host_response_frames,
            self.host_response_bytes,
            self.host_response_copy_bytes,
            self.host_encode_ns,
            self.host_socket_write_ns,
            self.client_response_frames,
            self.client_response_bytes,
            self.client_socket_read_ns,
            self.client_decode_ns,
            self.client_decode_copy_bytes,
            self.host_dispatch_ns,
        ]
    }

    fn fields_mut(&mut self) -> [&mut u64; Self::FIELD_COUNT] {
        [
            &mut self.max_readahead_bytes,
            &mut self.init_capabilities,
            &mut self.kernel_read_requests,
            &mut self.kernel_read_bytes,
            &mut self.kernel_read_le_4k,
            &mut self.kernel_read_le_64k,
            &mut self.kernel_read_le_256k,
            &mut self.kernel_read_le_1m,
            &mut self.kernel_read_gt_1m,
            &mut self.read_ahead_hits,
            &mut self.read_ahead_misses,
            &mut self.read_ahead_fetches,
            &mut self.read_ahead_requested_bytes,
            &mut self.read_ahead_fetched_bytes,
            &mut self.read_ahead_served_bytes,
            &mut self.read_ahead_unused_bytes,
            &mut self.read_ahead_cache_copy_bytes,
            &mut self.host_response_frames,
            &mut self.host_response_bytes,
            &mut self.host_response_copy_bytes,
            &mut self.host_encode_ns,
            &mut self.host_socket_write_ns,
            &mut self.client_response_frames,
            &mut self.client_response_bytes,
            &mut self.client_socket_read_ns,
            &mut self.client_decode_ns,
            &mut self.client_decode_copy_bytes,
            &mut self.host_dispatch_ns,
        ]
    }

    fn from_fields(fields: [u64; Self::FIELD_COUNT]) -> Self {
        let mut metrics = Self::default();
        for (target, value) in metrics.fields_mut().into_iter().zip(fields) {
            *target = value;
        }
        metrics
    }
}

#[derive(Default)]
pub(crate) struct AtomicFuseReadMetrics {
    max_readahead_bytes: AtomicU64,
    init_capabilities: AtomicU64,
    kernel_read_requests: AtomicU64,
    kernel_read_bytes: AtomicU64,
    kernel_read_le_4k: AtomicU64,
    kernel_read_le_64k: AtomicU64,
    kernel_read_le_256k: AtomicU64,
    kernel_read_le_1m: AtomicU64,
    kernel_read_gt_1m: AtomicU64,
    read_ahead_hits: AtomicU64,
    read_ahead_misses: AtomicU64,
    read_ahead_fetches: AtomicU64,
    read_ahead_requested_bytes: AtomicU64,
    read_ahead_fetched_bytes: AtomicU64,
    read_ahead_served_bytes: AtomicU64,
    read_ahead_unused_bytes: AtomicU64,
    read_ahead_cache_copy_bytes: AtomicU64,
    host_response_frames: AtomicU64,
    host_response_bytes: AtomicU64,
    host_response_copy_bytes: AtomicU64,
    host_encode_ns: AtomicU64,
    host_socket_write_ns: AtomicU64,
    client_response_frames: AtomicU64,
    client_response_bytes: AtomicU64,
    client_socket_read_ns: AtomicU64,
    client_decode_ns: AtomicU64,
    client_decode_copy_bytes: AtomicU64,
    host_dispatch_ns: AtomicU64,
}

impl AtomicFuseReadMetrics {
    pub(crate) fn note_config(&self, max_readahead: u64, capabilities: u64) {
        self.max_readahead_bytes
            .fetch_max(max_readahead, Ordering::Relaxed);
        self.init_capabilities
            .fetch_or(capabilities, Ordering::Relaxed);
    }

    pub(crate) fn note_kernel_read(&self, bytes: u64) {
        self.kernel_read_requests.fetch_add(1, Ordering::Relaxed);
        self.kernel_read_bytes.fetch_add(bytes, Ordering::Relaxed);
        let bucket = if bytes <= 4 * 1024 {
            &self.kernel_read_le_4k
        } else if bytes <= 64 * 1024 {
            &self.kernel_read_le_64k
        } else if bytes <= 256 * 1024 {
            &self.kernel_read_le_256k
        } else if bytes <= 1024 * 1024 {
            &self.kernel_read_le_1m
        } else {
            &self.kernel_read_gt_1m
        };
        bucket.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_read_ahead_hit(&self, served: u64) {
        self.read_ahead_hits.fetch_add(1, Ordering::Relaxed);
        self.read_ahead_served_bytes
            .fetch_add(served, Ordering::Relaxed);
        self.read_ahead_cache_copy_bytes
            .fetch_add(served, Ordering::Relaxed);
    }

    pub(crate) fn note_read_ahead_miss(&self, requested: u64, fetched: u64, served: u64) {
        self.read_ahead_misses.fetch_add(1, Ordering::Relaxed);
        self.read_ahead_fetches.fetch_add(1, Ordering::Relaxed);
        self.read_ahead_requested_bytes
            .fetch_add(requested, Ordering::Relaxed);
        self.read_ahead_fetched_bytes
            .fetch_add(fetched, Ordering::Relaxed);
        self.read_ahead_served_bytes
            .fetch_add(served, Ordering::Relaxed);
        self.read_ahead_cache_copy_bytes
            .fetch_add(served, Ordering::Relaxed);
    }

    pub(crate) fn note_unused(&self, bytes: u64) {
        self.read_ahead_unused_bytes
            .fetch_add(bytes, Ordering::Relaxed);
    }

    pub(crate) fn note_host_response(
        &self,
        frame_bytes: u64,
        _logical_bytes: u64,
        payload_copy_bytes: u64,
        encode_ns: u64,
        socket_ns: u64,
    ) {
        self.host_response_frames.fetch_add(1, Ordering::Relaxed);
        self.host_response_bytes
            .fetch_add(frame_bytes, Ordering::Relaxed);
        self.host_response_copy_bytes
            .fetch_add(payload_copy_bytes, Ordering::Relaxed);
        self.host_encode_ns.fetch_add(encode_ns, Ordering::Relaxed);
        self.host_socket_write_ns
            .fetch_add(socket_ns, Ordering::Relaxed);
    }

    pub(crate) fn note_client_response(
        &self,
        frame_bytes: u64,
        payload_copy_bytes: u64,
        socket_ns: u64,
        decode_ns: u64,
    ) {
        self.client_response_frames.fetch_add(1, Ordering::Relaxed);
        self.client_response_bytes
            .fetch_add(frame_bytes, Ordering::Relaxed);
        self.client_socket_read_ns
            .fetch_add(socket_ns, Ordering::Relaxed);
        self.client_decode_ns
            .fetch_add(decode_ns, Ordering::Relaxed);
        self.client_decode_copy_bytes
            .fetch_add(payload_copy_bytes, Ordering::Relaxed);
    }

    pub(crate) fn note_host_dispatch(&self, elapsed_ns: u64) {
        self.host_dispatch_ns
            .fetch_add(elapsed_ns, Ordering::Relaxed);
    }

    pub(crate) fn take(&self) -> FuseReadMetrics {
        let mut metrics = FuseReadMetrics::default();
        for (target, source) in metrics.fields_mut().into_iter().zip([
            &self.max_readahead_bytes,
            &self.init_capabilities,
            &self.kernel_read_requests,
            &self.kernel_read_bytes,
            &self.kernel_read_le_4k,
            &self.kernel_read_le_64k,
            &self.kernel_read_le_256k,
            &self.kernel_read_le_1m,
            &self.kernel_read_gt_1m,
            &self.read_ahead_hits,
            &self.read_ahead_misses,
            &self.read_ahead_fetches,
            &self.read_ahead_requested_bytes,
            &self.read_ahead_fetched_bytes,
            &self.read_ahead_served_bytes,
            &self.read_ahead_unused_bytes,
            &self.read_ahead_cache_copy_bytes,
            &self.host_response_frames,
            &self.host_response_bytes,
            &self.host_response_copy_bytes,
            &self.host_encode_ns,
            &self.host_socket_write_ns,
            &self.client_response_frames,
            &self.client_response_bytes,
            &self.client_socket_read_ns,
            &self.client_decode_ns,
            &self.client_decode_copy_bytes,
            &self.host_dispatch_ns,
        ]) {
            *target = source.swap(0, Ordering::Relaxed);
        }
        metrics
    }
}
