//! Pinned Zstandard codec with caller-owned bounded workspaces.
//!
//! The parameter sequences, workspace sizes and frame policy are the frozen ones
//! of the reference profile: payload frames use level 3, a role-specific window
//! log, a content-size field, a checksum, no dictionary id and no workers; group
//! bodies use level 1 with the window log capped at sixteen. Contexts live in a
//! caller-owned aligned region, so a codec call cannot grow an allocator-backed
//! context and every allocation is charged before the call.
//!
//! Only FULL (unprefixed) frames are produced and accepted here; prefix/DELTA
//! encoding is later scope, and a failed call never selects another codec.

use std::ffi::c_void;
use std::ptr;

use zstd_sys::{
    ZSTD_CCtx, ZSTD_CCtx_reset, ZSTD_CCtx_setCParams, ZSTD_CCtx_setFParams, ZSTD_CCtx_setParameter,
    ZSTD_DCtx, ZSTD_DCtx_reset, ZSTD_DCtx_setParameter, ZSTD_ErrorCode, ZSTD_FrameType_e,
    ZSTD_ResetDirective, ZSTD_cParameter, ZSTD_compress2, ZSTD_compressBound, ZSTD_dParameter,
    ZSTD_decompressDCtx, ZSTD_estimateCCtxSize_usingCParams, ZSTD_findFrameCompressedSize,
    ZSTD_frameParameters, ZSTD_getCParams, ZSTD_getErrorCode, ZSTD_getFrameHeader,
    ZSTD_initStaticCCtx, ZSTD_initStaticDCtx, ZSTD_isError,
};

use crate::error::{StorageError, StorageResult};

/// One aligned encode workspace shared by every role of one save.
pub const ENCODE_WORKSPACE_BYTES: usize = 2 * 1024 * 1024;
/// One aligned decode workspace shared by every role of one read.
pub const DECODE_WORKSPACE_BYTES: usize = 1024 * 1024;
/// Largest accepted group body before compression is attempted.
pub const GROUP_LIMIT: usize = 65_536;
/// Compression level of the ordinary group body codec.
const GROUP_LEVEL: i32 = 1;
/// Largest window log of the ordinary group body codec.
const GROUP_WINDOW_LOG_MAX: u32 = 16;

/// Role-specific payload codec profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecProfile {
    /// Chunk payloads: 32 KiB raw limit, window log 20.
    Native,
    /// Whole-file payloads: 128 KiB raw limit, window log 18.
    Small,
}

impl CodecProfile {
    /// Largest raw payload accepted.
    pub const fn raw_limit(self) -> usize {
        match self {
            Self::Native => 32_768,
            Self::Small => 131_071,
        }
    }

    /// Largest accepted frame.
    pub const fn frame_limit(self) -> usize {
        match self {
            Self::Native => 33_024,
            Self::Small => 135_168,
        }
    }

    /// Fixed window log.
    pub const fn window_log(self) -> i32 {
        match self {
            Self::Small => 18,
            Self::Native => 20,
        }
    }

    /// Bytes reserved for the codec's own scratch inside the encode workspace.
    const fn estimate_limit(self) -> usize {
        match self {
            Self::Small => 2 * 1024 * 1024,
            Self::Native => 1024 * 1024,
        }
    }
}

const _: () = assert!(CodecProfile::Small.estimate_limit() <= ENCODE_WORKSPACE_BYTES);

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
                (ZSTD_cParameter::ZSTD_c_compressionLevel, 3),
                (ZSTD_cParameter::ZSTD_c_windowLog, profile.window_log()),
                (ZSTD_cParameter::ZSTD_c_contentSizeFlag, 1),
                (ZSTD_cParameter::ZSTD_c_checksumFlag, 1),
                (ZSTD_cParameter::ZSTD_c_dictIDFlag, 0),
                (ZSTD_cParameter::ZSTD_c_nbWorkers, 0),
            ] {
                checked(ZSTD_CCtx_setParameter(context, parameter, value))?;
            }
            let estimate = checked(ZSTD_estimateCCtxSize_usingCParams(ZSTD_getCParams(
                3,
                raw.len() as u64,
                raw.len(),
            )))?;
            let bound = checked(ZSTD_compressBound(raw.len()))?;
            if estimate > profile.estimate_limit() || bound > profile.frame_limit() {
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

    /// Compresses one ordinary group body with the frozen group parameters.
    ///
    /// The sequence is level 1 with the window log capped at
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
            if bound > GROUP_LIMIT + 1024 {
                return Err(resource());
            }
            let mut frame = output(bound, GROUP_LIMIT + 1024)?;
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
    /// Allocates the bounded decode workspace and its static context.
    pub fn new() -> StorageResult<Self> {
        let mut memory = workspace(DECODE_WORKSPACE_BYTES)?;
        // SAFETY: as above; the static decoder neither allocates nor needs a free.
        let context = unsafe {
            ZSTD_initStaticDCtx(
                memory.as_mut_ptr().cast::<c_void>(),
                workspace_bytes(&memory),
            )
        };
        if context.is_null() {
            return Err(resource());
        }
        Ok(Self { memory, context })
    }

    /// Bytes charged by the decode workspace.
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
        let context = self.context;
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

    /// Decompresses one ordinary group body frame.
    pub fn decompress_group(&mut self, frame: &[u8], raw_length: usize) -> StorageResult<Vec<u8>> {
        if frame.is_empty()
            || raw_length == 0
            || raw_length > GROUP_LIMIT
            || frame.len() > GROUP_LIMIT + 1024
        {
            return Err(StorageError::Integrity("group body frame bounds"));
        }
        let context = self.context;
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

/// Level and window cap used by the ordinary group body codec.
pub const fn group_body_parameters() -> (i32, u32) {
    (GROUP_LEVEL, GROUP_WINDOW_LOG_MAX)
}
