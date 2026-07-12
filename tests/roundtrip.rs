//! End-to-end dedup round-trip from the user's perspective.
//!
//! This models what a real dedup store does, exactly as the README documents:
//! chunk -> store each segment's `dedup_key()` under its hash -> later, restore
//! purely from (store + manifest) by tiling the stored bytes to `len` -> assert
//! the bytes come back identical.
//!
//! This is the test that was missing: the in-crate unit tests reconstructed from
//! the segment's own fields, never from the *stored* `dedup_key()`, so they could
//! not catch a `dedup_key()` that isn't reconstruction-safe.

use std::collections::HashMap;
use std::io::{self, Cursor, Read};

use mothcdc::mincdc::{MinCdcHash4, ReadChunker, SliceChunker};
use mothcdc::{MothChunker, MothReadChunker};
use proptest::prelude::*;

fn fnv1a(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &x in b {
        h ^= x as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn xorshift(seed: u64, n: usize) -> Vec<u8> {
    let mut s = seed | 1;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 33) as u8
        })
        .collect()
}

fn build(data: &[u8], min: usize, max: usize) -> MothChunker<'_, MinCdcHash4> {
    MothChunker::new(data, min, max)
}

/// The full user round-trip: chunk, build a content-addressed store keyed by
/// `hash(dedup_key())` plus an ordered manifest, then restore from store +
/// manifest only and assert it equals the input. Also checks `reconstruct_into`.
fn assert_roundtrip(label: &str, data: &[u8], min: usize, max: usize) {
    let tag = format!("{label} (min={min} max={max})");

    // --- ingest ---
    let mut store: HashMap<u64, Vec<u8>> = HashMap::new();
    let mut manifest: Vec<(u64, u64)> = Vec::new(); // (key_hash, logical_len)
    let mut next_off = 0u64;
    for seg in build(data, min, max) {
        assert_eq!(seg.offset(), next_off, "{tag}: non-contiguous offset");
        assert!(!seg.is_empty(), "{tag}: empty segment");
        // Note: a coalesced segment's len() intentionally exceeds max (it stands
        // for a whole run of chunks), so max only bounds the underlying chunks.
        let key = seg.dedup_key();
        let h = fnv1a(key);
        store.entry(h).or_insert_with(|| key.to_vec());
        manifest.push((h, seg.len()));
        next_off += seg.len();
    }
    assert_eq!(next_off, data.len() as u64, "{tag}: coverage gap");

    // The caterpillar must represent exactly the same underlying chunks as plain
    // mincdc — it only groups them, never adds or drops any.
    let plain = SliceChunker::new(data, min, max, MinCdcHash4::new()).count();
    let expanded: u64 = build(data, min, max).map(|s| s.chunk_count()).sum();
    assert_eq!(
        expanded, plain as u64,
        "{tag}: chunk_count {expanded} != plain {plain}"
    );

    // --- restore from store + manifest only (the documented path) ---
    let mut restored = Vec::with_capacity(data.len());
    for (h, len) in &manifest {
        let bytes = store
            .get(h)
            .expect("manifest references a chunk not in the store");
        let mut w = 0u64;
        while w < *len {
            let take = (bytes.len() as u64).min(*len - w) as usize;
            restored.extend_from_slice(&bytes[..take]);
            w += take as u64;
        }
    }
    assert_eq!(restored, data, "{tag}: store round-trip corrupted the data");

    // --- reconstruct_into helper must agree too ---
    let mut via_helper = Vec::with_capacity(data.len());
    for seg in build(data, min, max) {
        seg.reconstruct_into(&mut via_helper);
    }
    assert_eq!(via_helper, data, "{tag}: reconstruct_into mismatch");
}

fn corpora() -> Vec<(&'static str, Vec<u8>)> {
    let mut periodic = Vec::new();
    let p = xorshift(9, 777);
    while periodic.len() < 1024 * 1024 {
        periodic.extend_from_slice(&p);
    }
    let mut holed = xorshift(2, 1024 * 1024);
    for b in holed[256 * 1024..768 * 1024].iter_mut() {
        *b = 0;
    }
    // A period break near the end of the stream: at the run's tail the decision
    // windows straddle the break, which is exactly where the caterpillar's
    // packed-scanning fast path must hand back to the argmin path.
    let mut broken = periodic.clone();
    let blen = broken.len();
    broken[blen - 10_000] ^= 0xFF;
    vec![
        ("random", xorshift(1, 1024 * 1024)),
        ("zeros", vec![0u8; 1024 * 1024]),
        ("const-0xAB", vec![0xABu8; 1024 * 1024]),
        ("periodic-777", periodic),
        ("periodic-777-break", broken),
        ("random+zero-hole", holed),
        ("tiny", xorshift(5, 100)),
        ("empty", Vec::new()),
    ]
}

#[test]
fn roundtrip_wide_and_narrow() {
    for (name, data) in corpora() {
        // Wide window (normal CDC) and a narrow window.
        assert_roundtrip(name, &data, 2048, 14336);
        assert_roundtrip(name, &data, 2048, 2200);
    }
}

/// A reader that returns at most `step` bytes per call, to exercise the streaming
/// chunker's buffer refill/shift logic.
struct ChokedReader<'a> {
    data: &'a [u8],
    pos: usize,
    step: usize,
}
impl std::io::Read for ChokedReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.step.min(buf.len()).min(self.data.len() - self.pos);
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

fn segment_layout<R: Read>(reader: R, min: usize, max: usize) -> Vec<(u64, u64, u64, Vec<u8>)> {
    let mut chunker = MothReadChunker::new(reader, min, max);
    let mut out = Vec::new();
    while let Some(segment) = chunker.next().unwrap() {
        out.push((
            segment.offset(),
            segment.len(),
            segment.chunk_count(),
            segment.dedup_key().to_vec(),
        ));
    }
    out
}

#[test]
fn streaming_grouping_is_independent_of_read_fragmentation() {
    let data = vec![0u8; 1024 * 1024];
    let cursor = segment_layout(Cursor::new(&data), 2048, 14336);
    let one_byte = segment_layout(
        ChokedReader {
            data: &data,
            pos: 0,
            step: 1,
        },
        2048,
        14336,
    );
    assert_eq!(one_byte, cursor);
    assert_eq!(cursor.len(), 1, "a zero run should be one record");
    assert!(cursor[0].2 >= 2);
}

struct InterruptOnce<R> {
    inner: R,
    interrupted: bool,
}

impl<R: Read> Read for InterruptOnce<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.interrupted {
            self.interrupted = true;
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.inner.read(buf)
    }
}

#[test]
fn moth_reader_retries_interrupted() {
    let data = xorshift(88, 8192);
    let reader = InterruptOnce {
        inner: Cursor::new(&data),
        interrupted: false,
    };
    assert_stream_roundtrip("interrupted", reader, &data, 64, 256);
}

struct ErrorOnceAfterData<'a> {
    data: &'a [u8],
    pos: usize,
    errored: bool,
}

impl Read for ErrorOnceAfterData<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= 257 && !self.errored {
            self.errored = true;
            return Err(io::Error::other("transient failure"));
        }
        let remaining_before_error = 257usize.saturating_sub(self.pos);
        let n = buf
            .len()
            .min(self.data.len() - self.pos)
            .min(remaining_before_error.max(1));
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

#[test]
fn moth_reader_can_resume_after_error_with_internal_progress() {
    let data = vec![0u8; 4096];
    let reader = ErrorOnceAfterData {
        data: &data,
        pos: 0,
        errored: false,
    };
    let mut chunker = MothReadChunker::new(reader, 64, 256);
    assert_eq!(chunker.next().unwrap_err().kind(), io::ErrorKind::Other);

    let mut rebuilt = Vec::new();
    while let Some(segment) = chunker.next().unwrap() {
        segment.reconstruct_into(&mut rebuilt);
    }
    assert_eq!(rebuilt, data);
}

#[test]
fn moth_reader_reports_permanent_errors() {
    let mut chunker = MothReadChunker::new(FailingReader, 64, 256);
    assert_eq!(chunker.next().unwrap_err().kind(), io::ErrorKind::Other);
}

#[test]
fn moth_fallible_constructor_rejects_overflowing_configuration() {
    let result = MothReadChunker::try_new(Cursor::new(Vec::<u8>::new()), 0, usize::MAX);
    match result {
        Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidInput),
        Ok(_) => panic!("overflowing configuration was accepted"),
    }
}

struct FailingReader;

impl Read for FailingReader {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("permanent failure"))
    }
}

/// Drives the streaming caterpillar over `reader`, asserting contiguity, lossless
/// reconstruction, exact coverage, and that it represents the same underlying
/// chunk count as plain mincdc.
fn assert_stream_roundtrip<R: std::io::Read>(
    tag: &str,
    reader: R,
    data: &[u8],
    min: usize,
    max: usize,
) {
    let cdc = MinCdcHash4::new();
    let plain = SliceChunker::new(data, min, max, cdc).count();
    let mut rc = MothReadChunker::with_cdc(reader, min, max, cdc);
    let (mut next_off, mut chunks) = (0u64, 0u64);
    let mut rebuilt = Vec::with_capacity(data.len());
    while let Some(s) = rc.next().unwrap() {
        assert_eq!(s.offset(), next_off, "{tag}: non-contiguous");
        assert!(!s.is_empty(), "{tag}: empty segment");
        chunks += s.chunk_count();
        next_off += s.len();
        s.reconstruct_into(&mut rebuilt);
    }
    assert_eq!(rebuilt, data, "{tag}: stream round-trip corrupted data");
    assert_eq!(next_off, data.len() as u64, "{tag}: coverage gap");
    assert_eq!(
        chunks, plain as u64,
        "{tag}: chunk_count {chunks} != plain {plain}"
    );
}

#[test]
fn streaming_chunk_boundaries_match_plain_mincdc() {
    // Proves the streaming caterpillar uses the SAME content-defined chunk
    // boundaries as plain ReadChunker — so cross-machine dedup is equivalent,
    // not merely lossless. This also guards against the boundary-decision logic
    // (replicated in chunk_len) silently diverging from the core chunkers.
    for (name, data) in corpora() {
        for (min, max) in [(2048usize, 14336usize), (2048, 2200), (64, 256)] {
            let cdc = MinCdcHash4::new();

            let mut plain = Vec::new();
            let mut rc = ReadChunker::new(Cursor::new(&data), min, max, cdc);
            while let Some(c) = rc.next().unwrap() {
                plain.push((c.offset(), c.len()));
            }

            // Expand the streaming caterpillar's segments back to per-chunk
            // boundaries; they must match plain mincdc exactly.
            let mut expanded = Vec::new();
            let mut sc = MothReadChunker::with_cdc(Cursor::new(&data), min, max, cdc);
            while let Some(s) = sc.next().unwrap() {
                let unit_len = s.dedup_key().len();
                for i in 0..s.chunk_count() {
                    expanded.push((s.offset() + i * unit_len as u64, unit_len));
                }
            }

            assert_eq!(
                plain, expanded,
                "{name} (min={min} max={max}): boundaries differ"
            );
        }
    }
}

#[test]
fn streaming_caterpillar_roundtrips() {
    for (name, data) in corpora() {
        for (min, max) in [(2048usize, 14336usize), (2048, 2200), (64, 256)] {
            // One big read (Cursor) and a hostile 1-byte reader — the run splits
            // land differently, but both must be lossless and fully cover.
            assert_stream_roundtrip(
                &format!("{name}/cursor"),
                Cursor::new(&data),
                &data,
                min,
                max,
            );
            let choked = ChokedReader {
                data: &data,
                pos: 0,
                step: 1,
            };
            assert_stream_roundtrip(&format!("{name}/choked"), choked, &data, min, max);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// Fuzz the full round-trip over random data and sizes. Shrinks any
    /// state-machine or reconstruction bug to a minimal case.
    #[test]
    fn prop_caterpillar_roundtrip(
        data in proptest::collection::vec(any::<u8>(), 0..8192),
        min in 16usize..512,
        extra in 0usize..512,
    ) {
        assert_roundtrip("prop", &data, min, min + extra);
    }

    /// Fuzz the streaming caterpillar over random data, sizes, and reader chunk
    /// sizes — asserts lossless reconstruction, full coverage, and same chunk
    /// count as plain mincdc regardless of how the reader fragments input.
    #[test]
    fn prop_streaming_caterpillar_roundtrip(
        data in proptest::collection::vec(any::<u8>(), 0..8192),
        min in 16usize..512,
        extra in 0usize..512,
        step in 1usize..4096,
    ) {
        let choked = ChokedReader { data: &data, pos: 0, step };
        assert_stream_roundtrip("prop-stream", choked, &data, min, min + extra);
    }
}
