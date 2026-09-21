//! Pinned Zstandard codec with caller-owned bounded workspaces.
//!
//! **This module is the crate's audited `unsafe` boundary.** `unsafe` is denied
//! everywhere else in `layerfs-storage` (and rejected again by the product
//! boundary guard); it is allowed here and only here, because the pinned codec
//! is the zstd C API. The complete FFI inventory: `ZSTD_isError` and
//! `ZSTD_getErrorCode` (numeric return interpretation), `ZSTD_compressBound`
//! and `ZSTD_estimateCCtxSize_usingCParams` (size arithmetic before any
//! allocation), `ZSTD_initStaticCCtx` and `ZSTD_initStaticDCtx` (static contexts
//! in caller-owned aligned workspaces), `ZSTD_CCtx_reset`/`ZSTD_CCtx_setParameter`
//! /`ZSTD_CCtx_setCParams`/`ZSTD_CCtx_setFParams`/`ZSTD_CCtx_refPrefix`/
//! `ZSTD_compress2` (encoding), `ZSTD_DCtx_reset`/`ZSTD_DCtx_setParameter`/
//! `ZSTD_DCtx_refPrefix`/`ZSTD_decompressDCtx` (decoding), `ZSTD_getCParams`
//! (group parameters), `ZSTD_getFrameHeader` and `ZSTD_findFrameCompressedSize`
//! (frame validation). Every block carries its own SAFETY argument; sizes are
//! read from the frame header and checked against declared limits before any
//! decompression, and decompression is exact-size into a validated destination.
//!
//! The parameter sequences, workspace sizes and frame policy are the measured
//! ones of the retained-history profile: payload frames use level 3, a
//! role-specific window log, a content-size field, a checksum, no dictionary id
//! and no workers; group bodies use level 1 with the window log capped at
//! sixteen. Contexts live in a caller-owned aligned region, so a codec call
//! cannot grow an allocator-backed context and every allocation is charged
//! before the call.
//!
//! Both FULL (unprefixed) and PREFIX frames are produced here. A prefix frame
//! borrows caller-supplied base bytes as a raw Zstandard prefix for exactly one
//! call; the reference is cleared on success and on failure, so no prefix
//! outlives the call that supplied it. A failed call never selects another codec.

use crate::policy::StorageCapacities;

use std::ffi::c_void;
use std::ptr;

use zstd_sys::{
    ZSTD_CCtx, ZSTD_CCtx_refPrefix, ZSTD_CCtx_reset, ZSTD_CCtx_setCParams, ZSTD_CCtx_setFParams,
    ZSTD_CCtx_setParameter, ZSTD_DCtx, ZSTD_DCtx_refPrefix, ZSTD_DCtx_reset,
    ZSTD_DCtx_setParameter, ZSTD_ErrorCode, ZSTD_FrameType_e, ZSTD_ResetDirective, ZSTD_cParameter,
    ZSTD_compress2, ZSTD_compressBound, ZSTD_dParameter, ZSTD_decompressDCtx,
    ZSTD_estimateCCtxSize_usingCParams, ZSTD_findFrameCompressedSize, ZSTD_frameParameters,
    ZSTD_getCParams, ZSTD_getErrorCode, ZSTD_getFrameHeader, ZSTD_initStaticCCtx,
    ZSTD_initStaticDCtx, ZSTD_isError,
};

use crate::error::{StorageError, StorageResult};

/// One aligned encode workspace shared by every role of one save.
///
/// The retained 16 MiB bound was sized for the earlier payload-level-9 and
/// group-level-19 profiles. Those historical estimates are conservative for
/// the current profiles. The group-level change does not resize this arena:
/// one save allocates it once and shares it across all codec calls. Every
/// static-context request must still fit; failure is returned to the caller.
pub const ENCODE_WORKSPACE_BYTES: usize = 16 * 1024 * 1024;
/// One aligned decode workspace shared by every role of one read.
pub const DECODE_WORKSPACE_BYTES: usize = 1024 * 1024;
/// Largest accepted group body before compression is attempted.
pub const GROUP_LIMIT: usize = 65_536;
/// Largest accepted group body frame.
pub const GROUP_FRAME_LIMIT: usize = GROUP_LIMIT + 1024;
/// Compression level of whole-file and chunk payload records.
const PAYLOAD_LEVEL: i32 = 3;
/// Compression level shared by ordinary and pooled value-group bodies.
///
/// This profile trades a larger encoded representation for less group-encoding
/// work. Payload settings, frame integrity checks and workspace bounds are
/// independent of this level and remain unchanged.
const GROUP_LEVEL: i32 = 1;
/// Largest window log of the ordinary group body codec.
const GROUP_WINDOW_LOG_MAX: u32 = 16;

/// Bytes of a payload the stored-frame probe compresses.
///
/// A payload at or below this width is not probed: the sample would be the whole
/// payload, so the probe would pay for the codec call twice to learn nothing the
/// frame itself does not already say. `encode_representation` compresses such a
/// payload once and compares the frame with the payload.
pub const STORED_PROBE_BYTES: usize = 1024;
/// Saving a probe must find, as a fraction of the sample, before the payload is
/// compressed for real.
///
/// `STORED_PROBE_SAVING_NUM / STORED_PROBE_SAVING_DEN` is `1/32`, and the
/// threshold is deliberately *below* the codec's own frame overhead: a frame
/// carries a four-byte magic, a header and a checksum, which on a 1024-byte
/// sample is already about 1.3 %. A sample the pinned profile cannot shrink by
/// 3 % is one the profile cannot shrink, and storing it verbatim costs at most
/// the 3 % the probe did not find.
pub const STORED_PROBE_SAVING_NUM: usize = 1;
/// Denominator of [`STORED_PROBE_SAVING_NUM`].
pub const STORED_PROBE_SAVING_DEN: usize = 32;

/// Pinned payload codec profile: raw bound, frame bound and window log.
///
/// The chunk profile is fixed by the frozen CDC grammar. The whole-file profile
/// is derived from the accepted Store policy, so a larger construction cutoff
/// gets the window and frame bound its payload actually needs while the default
/// cutoff keeps its frozen parameters exactly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodecProfile {
    raw_limit: usize,
    frame_limit: usize,
    window_log: i32,
}

impl CodecProfile {
    /// Chunk payloads: 32 KiB raw limit, window log 20.
    pub const fn native() -> Self {
        Self {
            raw_limit: CHUNK_RAW_LIMIT,
            frame_limit: CHUNK_FRAME_LIMIT,
            window_log: 20,
        }
    }

    /// Whole-file payloads under `capacities`.
    pub const fn whole_file(capacities: &StorageCapacities) -> Self {
        Self {
            raw_limit: capacities.whole_file_canonical_limit - WHOLE_FILE_CANONICAL_OVERHEAD,
            frame_limit: capacities.whole_file_frame_limit,
            window_log: capacities.whole_file_window_log,
        }
    }

    /// Largest raw payload accepted.
    pub const fn raw_limit(self) -> usize {
        self.raw_limit
    }

    /// Largest accepted frame.
    pub const fn frame_limit(self) -> usize {
        self.frame_limit
    }

    /// Fixed window log.
    pub const fn window_log(self) -> i32 {
        self.window_log
    }
}

/// Frozen chunk payload limits.
const CHUNK_RAW_LIMIT: usize = 32_768;
/// Frozen chunk frame limit.
const CHUNK_FRAME_LIMIT: usize = 33_024;
/// Canonical bytes a whole-file object adds over its raw payload: the 13-byte
/// bytes-role envelope and the 10-byte whole-file value header.
const WHOLE_FILE_CANONICAL_OVERHEAD: usize = 23;
fn resource() -> StorageError {
    StorageError::Integrity("bounded Zstandard workspace unavailable")
}

fn codec_failure() -> StorageError {
    StorageError::Integrity("Zstandard codec failure")
}

fn checked(code: usize) -> StorageResult<usize> {
    // SAFETY: this API only interprets the numeric return value.
    if unsafe { ZSTD_isError(code) } != 0 {
        return Err(codec_failure());
    }
    Ok(code)
}

fn encode_checked(code: usize) -> StorageResult<usize> {
    // SAFETY: this API only interprets the numeric return value.
    if unsafe { ZSTD_getErrorCode(code) } == ZSTD_ErrorCode::ZSTD_error_memory_allocation {
        return Err(resource());
    }
    checked(code)
}

fn workspace(size: usize) -> StorageResult<Vec<u64>> {
    if size == 0 {
        return Err(resource());
    }
    let mut words = Vec::new();
    words
        .try_reserve_exact(size.div_ceil(8))
        .map_err(|_| resource())?;
    words.resize(size.div_ceil(8), 0_u64);
    Ok(words)
}

fn output(size: usize, limit: usize) -> StorageResult<Vec<u8>> {
    if size == 0 || size > limit {
        return Err(resource());
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(size).map_err(|_| resource())?;
    bytes.resize(size, 0);
    Ok(bytes)
}

fn workspace_bytes(words: &[u64]) -> usize {
    std::mem::size_of_val(words)
}

/// Owned encode workspace plus the static context built inside it.
pub struct CompressionWorkspace {
    memory: Vec<u64>,
    context: *mut ZSTD_CCtx,
}

impl CompressionWorkspace {
    /// Allocates the bounded encode workspace and its static context.
    pub fn new() -> StorageResult<Self> {
        let mut memory = workspace(ENCODE_WORKSPACE_BYTES)?;
        // SAFETY: the region is live, eight-byte aligned and owned by `memory`;
        // the static context neither allocates nor needs a C free.
        let context = unsafe {
            ZSTD_initStaticCCtx(
                memory.as_mut_ptr().cast::<c_void>(),
                workspace_bytes(&memory),
            )
        };
        if context.is_null() {
            return Err(resource());
        }
        Ok(Self { memory, context })
    }

    /// Bytes charged by the encode workspace.
    pub fn workspace_bytes(&self) -> usize {
        workspace_bytes(&self.memory)
    }

    /// Compresses `raw` under `profile`, returning the exact payload frame.
    pub fn compress(&mut self, profile: CodecProfile, raw: &[u8]) -> StorageResult<Vec<u8>> {
        if raw.is_empty() || raw.len() > profile.raw_limit() {
            return Err(StorageError::CapacityExceeded {
                what: "codec.raw_payload",
                limit: profile.raw_limit() as u64,
                actual: raw.len() as u64,
            });
        }
        let context = self.context;
        // SAFETY: `context` is a live static context inside `self.memory`, which
        // outlives the call and is exclusively borrowed. Input and output are
        // live, non-overlapping Rust allocations.
        unsafe {
            checked(ZSTD_CCtx_reset(
                context,
                ZSTD_ResetDirective::ZSTD_reset_session_and_parameters,
            ))?;
            for (parameter, value) in [
                (ZSTD_cParameter::ZSTD_c_compressionLevel, PAYLOAD_LEVEL),
                (ZSTD_cParameter::ZSTD_c_windowLog, profile.window_log()),
                (ZSTD_cParameter::ZSTD_c_contentSizeFlag, 1),
                (ZSTD_cParameter::ZSTD_c_checksumFlag, 1),
                (ZSTD_cParameter::ZSTD_c_dictIDFlag, 0),
                (ZSTD_cParameter::ZSTD_c_nbWorkers, 0),
            ] {
                checked(ZSTD_CCtx_setParameter(context, parameter, value))?;
            }
            let estimate = checked(ZSTD_estimateCCtxSize_usingCParams(ZSTD_getCParams(
                PAYLOAD_LEVEL,
                raw.len() as u64,
                raw.len(),
            )))?;
            let bound = checked(ZSTD_compressBound(raw.len()))?;
            if estimate > ENCODE_WORKSPACE_BYTES || bound > profile.frame_limit() {
                return Err(resource());
            }
            let mut frame = output(bound, profile.frame_limit())?;
            let length = encode_checked(ZSTD_compress2(
                context,
                frame.as_mut_ptr().cast::<c_void>(),
                frame.len(),
                raw.as_ptr().cast::<c_void>(),
                raw.len(),
            ))?;
            if length == 0 || length > frame.len() || length > profile.frame_limit() {
                return Err(codec_failure());
            }
            frame.truncate(length);
            checked(ZSTD_CCtx_reset(
                context,
                ZSTD_ResetDirective::ZSTD_reset_session_and_parameters,
            ))?;
            Ok(frame)
        }
    }

    /// Reports whether a bounded prefix of `raw` is one this profile cannot shrink.
    ///
    /// The sample is the payload's own first [`STORED_PROBE_BYTES`] bytes,
    /// compressed under the *same* profile the payload would be, so the answer is
    /// the codec's own and not a proxy statistic's: a sampled prefix that does not
    /// shrink by [`STORED_PROBE_SAVING_NUM`]/[`STORED_PROBE_SAVING_DEN`] is a
    /// prefix this profile cannot compress, and the caller stores the payload
    /// verbatim instead of paying for the whole scan.
    ///
    /// A payload at or below the probe width answers `false`: the caller compresses
    /// it once and compares the frame with the payload, which is the same decision
    /// for one codec call instead of two.
    ///
    /// The answer is an estimate by construction. A prefix is not the payload, and
    /// a payload whose first [`STORED_PROBE_BYTES`] bytes are unrepresentative is
    /// stored at its full width. That is a size consequence only: the stored form
    /// carries the payload itself, so no decision this makes can make a read wrong.
    pub fn payload_is_incompressible(
        &mut self,
        profile: CodecProfile,
        raw: &[u8],
    ) -> StorageResult<bool> {
        if raw.len() <= STORED_PROBE_BYTES {
            return Ok(false);
        }
        let sample = raw
            .get(..STORED_PROBE_BYTES)
            .ok_or(StorageError::Integrity("stored-frame probe sample"))?;
        let frame = self.compress(profile, sample)?;
        Ok(frame.len().saturating_mul(STORED_PROBE_SAVING_DEN)
            >= sample
                .len()
                .saturating_mul(STORED_PROBE_SAVING_DEN - STORED_PROBE_SAVING_NUM))
    }

    /// Compresses `raw` under `profile` against a caller-supplied raw prefix.
    ///
    /// The base bytes are borrowed as a raw Zstandard prefix (never parsed as a
    /// dictionary), for exactly one call. The reference is cleared on success and
    /// on failure, so the next call cannot observe a stale prefix and the base
    /// never outlives the borrow.
    pub fn compress_prefix(
        &mut self,
        profile: CodecProfile,
        raw: &[u8],
        prefix: &[u8],
    ) -> StorageResult<Vec<u8>> {
        if raw.is_empty() || raw.len() > profile.raw_limit() {
            return Err(StorageError::CapacityExceeded {
                what: "codec.raw_payload",
                limit: profile.raw_limit() as u64,
                actual: raw.len() as u64,
            });
        }
        if prefix.is_empty() || prefix.len() > profile.raw_limit() {
            return Err(StorageError::CapacityExceeded {
                what: "codec.prefix_payload",
                limit: profile.raw_limit() as u64,
                actual: prefix.len() as u64,
            });
        }
        let context = self.context;
        // SAFETY: `context` is a live static context inside `self.memory`, which
        // outlives the call and is exclusively borrowed. Input, prefix and output
        // are live, non-overlapping Rust allocations.
        unsafe {
            checked(ZSTD_CCtx_reset(
                context,
                ZSTD_ResetDirective::ZSTD_reset_session_and_parameters,
            ))?;
            for (parameter, value) in [
                (ZSTD_cParameter::ZSTD_c_compressionLevel, PAYLOAD_LEVEL),
                (ZSTD_cParameter::ZSTD_c_windowLog, profile.window_log()),
                (ZSTD_cParameter::ZSTD_c_contentSizeFlag, 1),
                (ZSTD_cParameter::ZSTD_c_checksumFlag, 1),
                (ZSTD_cParameter::ZSTD_c_dictIDFlag, 0),
                (ZSTD_cParameter::ZSTD_c_nbWorkers, 0),
            ] {
                checked(ZSTD_CCtx_setParameter(context, parameter, value))?;
            }
            let estimate = checked(ZSTD_estimateCCtxSize_usingCParams(ZSTD_getCParams(
                PAYLOAD_LEVEL,
                raw.len() as u64,
                raw.len(),
            )))?;
            let bound = checked(ZSTD_compressBound(raw.len()))?;
            if estimate > ENCODE_WORKSPACE_BYTES || bound > profile.frame_limit() {
                return Err(resource());
            }
            let framed = (|| {
                checked(ZSTD_CCtx_refPrefix(
                    context,
                    prefix.as_ptr().cast::<c_void>(),
                    prefix.len(),
                ))?;
                let mut frame = output(bound, profile.frame_limit())?;
                let length = encode_checked(ZSTD_compress2(
                    context,
                    frame.as_mut_ptr().cast::<c_void>(),
                    frame.len(),
                    raw.as_ptr().cast::<c_void>(),
                    raw.len(),
                ))?;
                if length == 0 || length > frame.len() || length > profile.frame_limit() {
                    return Err(codec_failure());
                }
                frame.truncate(length);
                Ok(frame)
            })();
            // The borrowed prefix is released on success and on failure alike.
            checked(ZSTD_CCtx_refPrefix(context, ptr::null(), 0))?;
            checked(ZSTD_CCtx_reset(
                context,
                ZSTD_ResetDirective::ZSTD_reset_session_and_parameters,
            ))?;
            framed
        }
    }

    /// Compresses one ordinary group body with the measured group parameters.
    ///
    /// The sequence is level [`GROUP_LEVEL`] with the window log capped at
    /// [`GROUP_WINDOW_LOG_MAX`], a content-size field, a checksum and no
    /// dictionary id. It is deliberately not the payload profile.
    pub fn compress_group(&mut self, raw: &[u8]) -> StorageResult<Vec<u8>> {
        if raw.is_empty() || raw.len() > GROUP_LIMIT {
            return Err(StorageError::Integrity("group body bounds"));
        }
        let context = self.context;
        // SAFETY: as in `compress`; the context stays inside `self.memory`.
        unsafe {
            let mut parameters = ZSTD_getCParams(GROUP_LEVEL, raw.len() as u64, 0_usize);
            parameters.windowLog = parameters.windowLog.min(GROUP_WINDOW_LOG_MAX);
            checked(ZSTD_CCtx_reset(
                context,
                ZSTD_ResetDirective::ZSTD_reset_session_and_parameters,
            ))?;
            checked(ZSTD_CCtx_setCParams(context, parameters))?;
            checked(ZSTD_CCtx_setFParams(
                context,
                ZSTD_frameParameters {
                    contentSizeFlag: 1,
                    checksumFlag: 1,
                    noDictIDFlag: 1,
                },
            ))?;
            let bound = checked(ZSTD_compressBound(raw.len()))?;
            if bound > GROUP_FRAME_LIMIT {
                return Err(resource());
            }
            let mut frame = output(bound, GROUP_FRAME_LIMIT)?;
            let length = checked(ZSTD_compress2(
                context,
                frame.as_mut_ptr().cast::<c_void>(),
                frame.len(),
                raw.as_ptr().cast::<c_void>(),
                raw.len(),
            ))?;
            if length == 0 || length > frame.len() {
                return Err(codec_failure());
            }
            frame.truncate(length);
            checked(ZSTD_CCtx_reset(
                context,
                ZSTD_ResetDirective::ZSTD_reset_session_and_parameters,
            ))?;
            Ok(frame)
        }
    }
}

impl Drop for CompressionWorkspace {
    fn drop(&mut self) {
        // A static context must never be released with the C free function; the
        // owned region is released by `memory` alone.
        self.context = ptr::null_mut();
    }
}

/// Owned decode workspace plus the static context built inside it.
pub struct DecompressionWorkspace {
    memory: Vec<u64>,
    context: *mut ZSTD_DCtx,
}

impl DecompressionWorkspace {
    /// A decode workspace whose bounded arena is allocated on first use.
    ///
    /// The arena is the read's own decode scratch, not a per-read constant: a
    /// read that decompresses nothing - a wave served entirely from uncompressed
    /// ordinary records - pays nothing for it, and the first frame that needs a
    /// decoder materialises it once for the rest of that read. One read call
    /// therefore allocates this arena at most once, and only when it has real
    /// decode work to do.
    pub fn new() -> StorageResult<Self> {
        Ok(Self {
            memory: Vec::new(),
            context: ptr::null_mut(),
        })
    }

    /// Materialises the decode arena and returns its static context.
    fn context(&mut self) -> StorageResult<*mut ZSTD_DCtx> {
        if self.context.is_null() {
            let mut memory = workspace(DECODE_WORKSPACE_BYTES)?;
            // SAFETY: the static decoder neither allocates nor needs a free.
            let context = unsafe {
                ZSTD_initStaticDCtx(
                    memory.as_mut_ptr().cast::<c_void>(),
                    workspace_bytes(&memory),
                )
            };
            if context.is_null() {
                return Err(resource());
            }
            self.memory = memory;
            self.context = context;
        }
        Ok(self.context)
    }

    /// Bytes charged by the decode workspace, zero until it is materialised.
    pub fn workspace_bytes(&self) -> usize {
        workspace_bytes(&self.memory)
    }

    /// Decompresses one unprefixed payload frame under `profile`.
    pub fn decompress(
        &mut self,
        profile: CodecProfile,
        frame: &[u8],
        raw_length: usize,
    ) -> StorageResult<Vec<u8>> {
        if frame.is_empty()
            || frame.len() > profile.frame_limit()
            || raw_length == 0
            || raw_length > profile.raw_limit()
        {
            return Err(StorageError::Integrity("Zstandard frame bounds"));
        }
        let context = self.context()?;
        // SAFETY: `context` is a live static decoder inside `self.memory`.
        // Header parsing only reads `frame`; the destination is exactly the
        // validated declared size, never a frame-derived grow.
        unsafe {
            let header = parse_frame_header(frame)?;
            if header.frameType != ZSTD_FrameType_e::ZSTD_frame
                || header.frameContentSize != raw_length as u64
                || header.windowSize > (1_u64 << profile.window_log())
                || header.dictID != 0
                || header.checksumFlag != 1
            {
                return Err(StorageError::Integrity("Zstandard frame fields"));
            }
            checked(ZSTD_DCtx_reset(
                context,
                ZSTD_ResetDirective::ZSTD_reset_session_and_parameters,
            ))?;
            checked(ZSTD_DCtx_setParameter(
                context,
                ZSTD_dParameter::ZSTD_d_windowLogMax,
                profile.window_log(),
            ))?;
            let mut raw = output(raw_length, profile.raw_limit())?;
            if checked(ZSTD_decompressDCtx(
                context,
                raw.as_mut_ptr().cast::<c_void>(),
                raw.len(),
                frame.as_ptr().cast::<c_void>(),
                frame.len(),
            ))? != raw_length
            {
                return Err(StorageError::Integrity("Zstandard frame length"));
            }
            Ok(raw)
        }
    }

    /// Decompresses one prefix-framed payload against its exact base bytes.
    ///
    /// The base is borrowed as a raw prefix for exactly one call and the
    /// reference is cleared afterwards, on success and on failure. The frame
    /// header is validated before the call, so the declared size and the window
    /// are checked against the profile rather than trusted.
    pub fn decompress_prefix(
        &mut self,
        profile: CodecProfile,
        frame: &[u8],
        raw_length: usize,
        prefix: &[u8],
    ) -> StorageResult<Vec<u8>> {
        if frame.is_empty()
            || frame.len() > profile.frame_limit()
            || raw_length == 0
            || raw_length > profile.raw_limit()
            || prefix.is_empty()
            || prefix.len() > profile.raw_limit()
        {
            return Err(StorageError::Integrity("Zstandard frame bounds"));
        }
        let context = self.context()?;
        // SAFETY: as in `decompress`; `prefix` is borrowed for this call only.
        unsafe {
            let header = parse_frame_header(frame)?;
            if header.frameType != ZSTD_FrameType_e::ZSTD_frame
                || header.frameContentSize != raw_length as u64
                || header.windowSize > (1_u64 << profile.window_log())
                || header.dictID != 0
                || header.checksumFlag != 1
            {
                return Err(StorageError::Integrity("Zstandard prefix frame fields"));
            }
            checked(ZSTD_DCtx_reset(
                context,
                ZSTD_ResetDirective::ZSTD_reset_session_and_parameters,
            ))?;
            checked(ZSTD_DCtx_setParameter(
                context,
                ZSTD_dParameter::ZSTD_d_windowLogMax,
                profile.window_log(),
            ))?;
            let decoded = (|| {
                checked(ZSTD_DCtx_refPrefix(
                    context,
                    prefix.as_ptr().cast::<c_void>(),
                    prefix.len(),
                ))?;
                let mut raw = output(raw_length, profile.raw_limit())?;
                if checked(ZSTD_decompressDCtx(
                    context,
                    raw.as_mut_ptr().cast::<c_void>(),
                    raw.len(),
                    frame.as_ptr().cast::<c_void>(),
                    frame.len(),
                ))? != raw_length
                {
                    return Err(StorageError::Integrity("Zstandard prefix frame length"));
                }
                Ok(raw)
            })();
            checked(ZSTD_DCtx_refPrefix(context, ptr::null(), 0))?;
            checked(ZSTD_DCtx_reset(
                context,
                ZSTD_ResetDirective::ZSTD_reset_session_and_parameters,
            ))?;
            decoded
        }
    }

    /// Decompresses one ordinary group body frame.
    pub fn decompress_group(&mut self, frame: &[u8], raw_length: usize) -> StorageResult<Vec<u8>> {
        if frame.is_empty()
            || raw_length == 0
            || raw_length > GROUP_LIMIT
            || frame.len() > GROUP_FRAME_LIMIT
        {
            return Err(StorageError::Integrity("group body frame bounds"));
        }
        let context = self.context()?;
        // SAFETY: as in `decompress`; only `frame` is read for header parsing.
        unsafe {
            let header = parse_frame_header(frame)?;
            if header.frameType != ZSTD_FrameType_e::ZSTD_frame
                || header.frameContentSize != raw_length as u64
                || header.windowSize > GROUP_LIMIT as u64
                || header.dictID != 0
                || header.checksumFlag != 1
            {
                return Err(StorageError::Integrity("group body frame fields"));
            }
            checked(ZSTD_DCtx_reset(
                context,
                ZSTD_ResetDirective::ZSTD_reset_session_and_parameters,
            ))?;
            let mut raw = output(raw_length, GROUP_LIMIT)?;
            if checked(ZSTD_decompressDCtx(
                context,
                raw.as_mut_ptr().cast::<c_void>(),
                raw.len(),
                frame.as_ptr().cast::<c_void>(),
                frame.len(),
            ))? != raw_length
            {
                return Err(StorageError::Integrity("group body frame length"));
            }
            Ok(raw)
        }
    }
}

impl Drop for DecompressionWorkspace {
    fn drop(&mut self) {
        self.context = ptr::null_mut();
    }
}

/// Validates the shared frame prefix, exact frame length and reserved bits.
///
/// SAFETY: the caller guarantees `frame` is a live slice; this only reads it.
unsafe fn parse_frame_header(frame: &[u8]) -> StorageResult<zstd_sys::ZSTD_FrameHeader> {
    if frame.get(..4) != Some(&[0x28, 0xb5, 0x2f, 0xfd])
        || frame.get(4).is_none_or(|descriptor| descriptor & 0x1b != 0)
    {
        return Err(StorageError::Integrity("Zstandard frame header"));
    }
    // SAFETY: header parsing only reads `frame`; a zero return initializes every
    // field used by the caller.
    unsafe {
        let mut header = std::mem::MaybeUninit::<zstd_sys::ZSTD_FrameHeader>::uninit();
        if checked(ZSTD_getFrameHeader(
            header.as_mut_ptr(),
            frame.as_ptr().cast::<c_void>(),
            frame.len(),
        ))? != 0
        {
            return Err(StorageError::Integrity("Zstandard frame header"));
        }
        if checked(ZSTD_findFrameCompressedSize(
            frame.as_ptr().cast::<c_void>(),
            frame.len(),
        ))? != frame.len()
        {
            return Err(StorageError::Integrity("Zstandard frame extent"));
        }
        Ok(header.assume_init())
    }
}

/// Compresses one ordinary group body, keeping it only when it is smaller.
///
/// The retained rule is the reference one: the compressed body plus its
/// sixteen-byte directory entry must fit inside the raw body.
pub fn compress_group_body(
    workspace: &mut CompressionWorkspace,
    raw: &[u8],
) -> StorageResult<Option<Vec<u8>>> {
    if raw.is_empty() || raw.len() > GROUP_LIMIT {
        return Err(StorageError::Integrity("group body bounds"));
    }
    let mut frame = workspace.compress_group(raw)?;
    if frame.len() + 16 <= raw.len() {
        frame.shrink_to_fit();
        Ok(Some(frame))
    } else {
        Ok(None)
    }
}
