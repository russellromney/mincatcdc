# Changelog

## 0.6.0 — 2026-07-10

Packed-scanning caterpillar fast path (VectorCDC-style SIMD).

- Inside a repetitive run, the caterpillar no longer pays a full argmin
  boundary search plus a memcmp per chunk. One packed equality scan
  (`byte_run_len` broadcast compare for constant bytes, `common_prefix_len`
  self-shifted compare for longer periods) proves the run periodic, and every
  chunk whose decision window stays inside the periodic region is emitted
  directly — the boundary search provably returns the same split, so it is
  skipped. See `packed_repeats` in `src/caterpillar.rs` for the proof.
- New SIMD primitives in all backends: NEON (`vceqq_u8` + `vshrn` mask),
  x86_64 (SSE2 baseline, AVX2, AVX-512BW via runtime dispatch), and a
  word-at-a-time scalar fallback.
- Applies to both `CaterpillarChunker` and `CaterpillarReadChunker`. Output is
  bit-identical to 0.5.0 (same segments, same grouping, same bytes).
- Bench results (`cargo bench`, min=2048 max=14336), caterpillar 0.5 → 0.6.
  Real data first — synthetic pure-zeros is the ceiling, not the claim:
  - Raw Debian 12 VM image (uncompressed, first 768 MiB): 5.0 → 13.0 GiB/s
    (2.6x) on AMD EPYC/AVX2; 3.0 → 11.2 GiB/s (3.8x) on Apple M-series/NEON.
  - Mostly-empty 200 MiB disk image: 1.8 → 30.9 GiB/s (17x, NEON).
  - Zero-padded build artifact: 5.0 → 8.1 GiB/s (1.6x, NEON).
  - FAST'25 DEB dataset (.ova appliances — streamOptimized/compressed VMDKs,
    so no byte-identical runs exist): no change, ~12 GiB/s all variants on
    EPYC/AVX2. Compressed or run-free data (SQLite, logs, random) is a no-op
    within ~2%.
  - Synthetic ceiling: zeros 2.4 → 74.3 GiB/s on Intel Xeon/AVX-512BW,
    1.8 → 30.2 GiB/s on NEON.
- Context vs UWASL dedup-bench (VectorCDC, FAST'25) on the same raw VM image,
  same machine (EPYC/AVX2), chunking-only, ~8.5 KiB avg chunks: FastCDC
  1.8 GiB/s, AE-Min 1.3 GiB/s, VectorCDC-AE-Min 14.1 GiB/s, VectorCDC-RAM
  25.2 GiB/s, mincatcdc packed 13.0 GiB/s — comparable to VectorCDC-AE-Min
  while additionally emitting caterpillar-coalesced metadata. On the DEB .ova
  files plain mincdc (12 GiB/s) is ~2.4x VectorCDC-AE-Min (4.9 GiB/s).
- Tests: a `packed_repeats` soundness test against the real boundary search as
  oracle (break position swept over every byte at every alignment), a
  segment-stream differential against the pre-SIMD caterpillar (adversarial
  corpus + proptest), per-width SIMD agreement tests, and a streaming corpus
  entry with a period break inside the final decision window.

## 0.5.0 and earlier

See git history (`git log --oneline`).
