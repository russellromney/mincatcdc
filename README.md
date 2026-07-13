# MothCDC

> A fork of [MinCDC](https://github.com/orlp/mincdc) that stores repeated data
> with far fewer records. The algorithm described below is MinCDC; see
> [This fork](#this-fork-mothcdc) for what MothCDC adds and why.

MinCDC splits data into chunks whose boundaries depend on the data itself.
When the same content appears in several files, it usually gets the same chunk
boundaries. That makes duplicate content easy to find and store only once.

To start using `mothcdc` add the following to your `Cargo.toml`:

    [dependencies]
    mothcdc = "0.7"

See [the documentation](https://docs.rs/mothcdc) for the full API and more
examples.


## This fork: MothCDC

MinCDC and its optimized implementations are Orson Peters' work. This fork adds
a *caterpillar* layer, an idea from the
[Chonkers algorithm](https://arxiv.org/abs/2509.11121) (Berger, 2025), with
speedups inspired by VectorCDC (Udayashankar et al., FAST '25).

When data contains a long repeated region — zeros, padding, or the same block
over and over — MinCDC can produce a large number of small chunks. The content
itself takes almost no space after deduplication, but each chunk still needs a
record. A mostly empty 200 MiB disk image, for example, produces about 182,000
records.

The caterpillar represents identical chunks that appear next to each other as
one record plus a repeat count. On that disk image, it reduces **182,701 records
to 7,798 (−96%)** without changing the stored content. MothCDC recognizes the
repeated region once and skips work inside it, making such data several times
faster to chunk than with plain MinCDC. Data without repeated regions is about
1% slower. A repeat can also continue from one block of streamed data into the
next, so even a multi-gigabyte zero region needs only one record.

MothCDC can also be called from C by enabling `--features capi`. Its
[dedup-bench fork](https://github.com/russellromney/dedup-bench/tree/MothCDC-integration)
tests MothCDC alongside the other chunkers.

Adding MothCDC as a normal Cargo dependency builds only the Rust library. To
build a static library for a C or C++ program, run:

```sh
cargo rustc --release --features capi --lib -- --crate-type staticlib
```

### Current benchmarks

All numbers come from [UWASL dedup-bench](https://github.com/UWASL/dedup-bench),
the benchmark used to evaluate VectorCDC. It tests and times MothCDC in exactly
the same way as every other algorithm. Tests used a Fly performance-8x machine
(AMD EPYC with AVX2, 8 vCPUs, 16 GiB RAM) and aimed for chunks of about 8 KiB.

The test data includes a raw Debian VM image (768 MiB), Debian `.ova`
appliances (3 GiB, from the FAST '25 dataset), six consecutive Linux 6.6.x
source archives (8 GiB, used like successive backups), and enwik9 (1 GiB of
Wikipedia text that was not used for tuning). MothCDC uses the recommended
range `(min=4096, max=12288)`; the "wide" row uses `(2048, 14336)`.

Chunking speed in GiB/s (higher is better):

| algorithm | raw VM | Debian VMs (DEB) | Linux archives (LNX) | Wikipedia (enwik9) |
|---|---|---|---|---|
| AE-Min / FastCDC / RAM | 1.4 / 1.8 / 1.5 | 1.4 / 2.0 / 1.4 | 1.4 / 1.9 / 1.6 | 1.4 / 1.9 / 1.9 |
| SeqCDC | 3.2 | 6.4 | 7.1 | 5.9 |
| VectorCDC AE-Min (AVX2) | 14.0 | 5.4 | 7.9 | 8.5 |
| MothCDC (wide) | 10.6 | 11.4 | 9.5 | 8.0 |
| **MothCDC** | **16.6** | **15.5** | **14.1** | **13.4** |
| VectorCDC RAM (AVX2) | 23.4 | 19.6 | 24.7 | 29.6 |

Space saved by deduplication (higher is better; machine speed does not affect
this result):

| algorithm | raw VM | Debian VMs (DEB) | Linux archives (LNX) |
|---|---|---|---|
| **MothCDC** | **53.3%** | **5.2%** | **60.3%** |
| MothCDC (wide) | 53.2% | 4.9% | 59.2% |
| SeqCDC | 52.4% | 4.0% | 56.5% |
| FastCDC | 51.8% | 4.8% | 52.4% |
| AE-Min | 50.0% | 3.5% | 55.5% |
| RAM / VectorCDC RAM | 51.4% | 3.2% | 49.2% |

Records needed to describe the chunks (lower is better), using the same
benchmark and test data: most algorithms need one record per chunk. MothCDC
stores an uninterrupted series of identical chunks as one record. The benchmark
still includes every original chunk when measuring space savings. Speedups do
not change the result: the standard and faster versions produce the same chunk
boundaries and record counts.

| algorithm | raw VM | Debian VMs (DEB) | Linux archives (LNX) | Wikipedia (enwik9) |
|---|---:|---:|---:|---:|
| AE-Min / VectorCDC AE-Min | 97,296 | 356,419 | 938,534 | 117,956 |
| FastCDC | **48,657** | **328,137** | 793,428 | 96,342 |
| RAM / VectorCDC RAM | 77,413 | 360,523 | **721,404** | **50,972** |
| SeqCDC | 71,267 | 345,820 | 1,368,420 | 133,535 |
| MinCDC / MothCDC plain (wide) | 231,777 | 343,821 | 1,186,018 | 163,214 |
| MothCDC (wide) | 56,536 | 343,775 | 1,185,991 | 163,197 |
| **MothCDC** | 56,175 | 363,796 | 1,113,467 | 138,532 |

In short: **MothCDC saved the most space on every dataset and at every tested
chunk size (4K, 8K, and 16K). It was also the fastest option except for
VectorCDC RAM**, which saved the least space in these tests. On the VM image,
which contains long runs of zeros, the wide configuration turns 231,777 chunks
into 56,536 records (−75.6%). The recommended configuration turns 143,765
chunks into 56,175 records (−60.9%). On data without long identical runs, the
record count barely changes and the caterpillar adds about 1% overhead.

Some context for these results: MinCDC's average chunk is slightly smaller
than the target, while several other algorithms produce slightly larger
chunks. Comparing identical average chunk sizes would narrow its advantage
over AE-Min. At a 16K target, SeqCDC and plain MinCDC save the same amount of
space. enwik9 shows that the speed carries over to data we did not tune on,
but it does not test deduplication: a single text file has almost no duplicate
content for any algorithm. Speeds vary between shared machines, so compare
results within a column.

To reproduce the results, download the public
[test data and checksums](https://mincatcdc-bench-corpus.t3.storage.dev/corpus/MANIFEST.sha256)
from Tigris. The benchmark fork is linked above, and
`BENCH_CORPUS=<dir> cargo bench` runs the in-repo bench on your own files.
No special build flags are needed. MothCDC automatically chooses the fastest
implementation supported by the CPU (SSE2, AVX2, or AVX-512 on x86_64; NEON on
aarch64). Older results are in `CHANGELOG.md` and `examples/`.

### Choosing the chunk-size range

MothCDC accepts a minimum and maximum chunk size. Prefer a narrow range with a
fairly high minimum. Both `(4096, 12288)` and
`(2048, 14336)` average about 8 KiB, but the narrower range was 40–50% faster
and saved at least as much space on every dataset tested. This is because the
algorithm searches `max - min` bytes for each boundary. (The narrower range is
the "MinCDCHash4-l" setting used by upstream below.) A higher `max` produces
fewer records, but was slower and saved less space in these tests.
`cargo run --release --example frontier -- <paths>` shows the trade-off on
your own data. Leave the hash settings at their defaults; changing them made no
measurable difference.

### Using the caterpillar

For an in-memory byte slice, use `MothChunker` and iterate over `Segment`s
instead of `Chunk`s:

```rust
use mothcdc::MothChunker;

for seg in MothChunker::new(&data, 4096, 12288) {
    // Store the unique content once. `dedup_key()` returns the bytes to hash
    // for either a single chunk or a repeated chunk.
    store(hash(seg.dedup_key()), seg.dedup_key());
    // Record its position, total length, and represented chunk count.
    record(seg.offset(), seg.len(), seg.chunk_count());
}
```

`MothChunker::new` uses the default `MinCdcHash4` settings. To change the hash
settings, create a chunker with
`mothcdc::mincdc::MinCdcHash4::with_params(multiplier, addend)` and pass it to
`MothChunker::with_cdc(&data, min, max, cdc)`.

`MothChunker` works on data already in memory. For larger inputs,
`MothReadChunker` reads a stream with a fixed amount of memory and combines
repeats in the same way. Finish using each returned `Segment` before asking for
the next one, as with `ReadChunker`. A repeated region can cross read-buffer
boundaries and still become one record. MothCDC copies at most one chunk
(`max_size` bytes) to carry the repeat forward; it does not copy other content.

Offsets, segment lengths, and chunk counts use `u64`, so streams larger than
4 GiB work correctly even on 32-bit systems. A single chunk cannot exceed
`mothcdc::mincdc::MAX_CHUNK_SIZE`. Reader chunkers allocate at least 4 MiB;
their `try_new` constructors return an error for bad settings or insufficient
memory instead of panicking.

(We also tested a second layer that looked for repeating patterns starting at
different positions. MinCDC already finds consistent boundaries for most of
these patterns, so the layer rarely helped and reduced speed by 76–99%. See
`examples/CATBENCH_RESULTS.md` and the `proto/caterpillar-period` branch.)

To get the disk-image number, two 200 MiB APFS images were created
with `hdiutil` (`hdiutil create -size 200m -fs APFS ...`), each holding a real
source tree (the second also holds an extra version). About 92% of each image is
zero-filled unused space. Those zeros are stored in the image rather than left
as sparse holes, so filesystem hole detection would not skip them.
Both images were chunked with `cargo run --release --example catbench` using
`min=2048, max=14336`. Plain mincdc produced 182,701 records; the caterpillar
produced 7,798, with identical deduplicated content. The full method and results
for Linux kernels, containers, SQLite databases, and source trees are in
`examples/REALBENCH_RESULTS.md`. This disk-image result is one specific case,
not a promise for every workload.

## How MinCDC chooses chunk boundaries

MinCDC slides a small window across the data and scores the bytes at each
position. Between `min_size` and `max_size`, the position with the lowest score
becomes the next chunk boundary. If several positions have the same score, the
earliest one wins. For the exact definition, it chooses `i` between `min_size`
and `max_size` that minimizes `evaluate(bytes[i - w..i])`, where `w` is the
window size. It returns `bytes[..i]` as the next chunk and repeats on
`bytes[i..]`.

The library provides two CPU-accelerated versions of MinCDC. Both look at four
bytes at a time:

 - `MinCDC4` reads each four-byte window as one 32-bit number (little-endian) and
   uses that number directly as its score.
 - `MinCDCHash4` hashes that number first, using
   `hash(x) = x.wrapping_mul(a).wrapping_add(b)` for constants `a` and `b`.

**`MinCDCHash4` can be about 10% slower, but it is less sensitive to patterns in
the input data. It is the recommended default.**

## Original MinCDC benchmark

MinCDC is several times faster than the commonly used
[FastCDC](https://crates.io/crates/fastcdc), with similar space savings from
deduplication. For this original upstream benchmark, all available Linux 6.x
source archives were downloaded with `tools/download-linux.sh`. Every algorithm
was configured to aim for an average chunk size of 8 KiB.

Speed was measured on `linux-6.0.tar` after loading it into memory, so disk speed
was not part of the result. "Space saved" is the share of the original data that
does not need to be stored again; higher is better. The last column adjusts each
algorithm until its average chunk is within 1% of 8 KiB. **Matching average
chunk sizes matters because smaller chunks usually find more duplicate data.**

| Algorithm     | AMD 9950X | Apple M2 Pro | Space saved | Average chunk | Space saved at ~8 KiB |
| --------------|-----------|--------------|-------------|---------------|-----------------------|
| MinCDCHash4-s | 41.3 GB/s | 23.8 GB/s | 61.08% | 8,015 | 60.92% |
| MinCDCHash4-l | 44.5 GB/s | 15.7 GB/s | 61.57% | 8,221 | 61.57% |
| MinCDC4-s     | 41.7 GB/s | 26.1 GB/s | 62.11% | 7,383 | 60.52% |
| MinCDC4-l     | 42.0 GB/s | 16.9 GB/s | 64.51% | 6,436 | 60.69% |
| FastCDC-s     | 6.6 GB/s | 4.1 GB/s | 54.38% | 12,866 | 61.81% |
| FastCDC-l     | 5.2 GB/s | 3.2 GB/s | 54.87% | 12,764 | ~62%* |

The "-s" versions use a narrow chunk range around 8 KiB (6,144–10,240 bytes).
The "-l" versions use a wider range (4,096–12,288 bytes). FastCDC needed an
even higher maximum because it sometimes produces much larger chunks, as shown
below. Raising that maximum had little effect on its speed.

FastCDC-l's final result has an asterisk because its average chunk size could
not be brought within 1% of 8 KiB. A small setting change made the average jump
from 7,741 to 10,846 bytes. At an average of 7,741 bytes, MinCDCHash4-l saved
62.49%, compared with FastCDC-l's 62.75%.

## Original chunk-size distribution

MinCDCHash4 spreads chunk sizes almost evenly between `min_size` and `max_size`.
Its actual average therefore stays close to the expected size. FastCDC produces
a much wider range of sizes when aiming for 8 KiB:

| MinCDCHash4 | FastCDC |
|-------------|---------|
| <img src="assets/mincdc-chunk-size-distr.png" width=300> | <img src="assets/fastcdc-chunk-size-distr.png" width=300> |

FastCDC produces many chunks near 8 KiB, but it also produces enough much larger
chunks to raise the average. MinCDCHash4 never creates a chunk outside the
chosen range, except for the final chunk, which may be smaller.

MinCDC does lean slightly toward smaller chunks because it chooses the earlier
boundary when scores tie, but the effect is small.
