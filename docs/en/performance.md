---
sidebar_position: 90
sidebar_label: Performance
---

# Measuring resource use

## Windows loopback measurements for the TCP buffer change

The two local Release builds were measured on 20 November 2026 on Windows 10 x64, Intel Core i7-11850H, using Rust 1.95.0 and Dart 3.13.7. Each build ran three times with 128 registered TCP tunnels, one client, TCP multiplexing enabled, TLS disabled and 64 warmed connections to one tunnel. Eight sockets each sent and verified 512 echoes of 64 KiB per round, for 512 MiB in both directions combined. Memory was sampled once per process after all sockets had carried data, before the timed transfer. The sample excludes the echo server, benchmark driver and Flutter UI.

| Median of three runs | Before the buffer change | After the buffer change |
| --- | ---: | ---: |
| Client working set | 39.94 MiB | 11.86 MiB |
| Server working set | 41.13 MiB | 13.12 MiB |
| Client private bytes | 35.31 MiB | 6.81 MiB |
| Server private bytes | 36.31 MiB | 8.11 MiB |
| Verified duplex throughput | 455.4 MiB/s | 443.4 MiB/s |

Throughput ranged from 431.1 to 458.7 MiB/s before the change and from 427.9 to 457.3 MiB/s after it. The median fell by 2.6%. The ranges overlap. WAN latency, TLS and sustained saturation need their own measurements. Windows working set and private bytes describe different aspects of memory and should be read separately.

After each transfer, the driver made five warm-up requests and 100 measured requests per endpoint over loopback HTTP. Times include Basic authentication, response serialization, transport and JSON decoding in Dart. Each cell gives the median of the three per-run p50 or p95 values.

| API request | Before p50 / p95 (µs) | After p50 / p95 (µs) |
| --- | ---: | ---: |
| System overview | 363 / 458 | 188 / 354 |
| Fourth tunnel page, 20 rows, name filter matching all 128 | 397 / 511 | 252 / 394 |
| Client page with one active client | 342 / 437 | 175 / 226 |

## Follow-up measurement for lazy traffic history

The baseline here already uses 32 KiB copy buffers. On the same Windows machine, the saved baseline and final binaries each completed three fresh runs of the load above. Memory was sampled with the 64 warmed connections still open. Stream byte totals are recorded on close, so this sample specifically exercises registered tunnels before their first traffic counter update.

| Median of three runs | Before lazy history | After lazy history |
| --- | ---: | ---: |
| Server working set | 13.14 MiB | 12.96 MiB |
| Server private bytes | 8.11 MiB | 7.79 MiB |
| Verified duplex throughput | 410.8 MiB/s | 432.8 MiB/s |
| System overview p50 / p95 | 206 / 306 µs | 219 / 376 µs |
| Filtered tunnel page p50 / p95 | 261 / 412 µs | 268 / 410 µs |
| Client page p50 / p95 | 197 / 282 µs | 175 / 262 µs |

Throughput ranged from 399.4 to 410.9 MiB/s before and 419.8 to 454.5 MiB/s after in the final comparison. An earlier three-run pair during development yielded medians of 417.7 and 420.0 MiB/s and overlapping ranges. The timings vary between batches. The directly attributable saving is four heap allocations and 496 bytes of counter payload per registered tunnel awaiting its first recorded byte, excluding allocator overhead. With 128 such tunnels this removes 62 KiB of counter payload. Process memory also includes other allocations and operating-system effects.

The six original JSON lines are in `docs/benchmarks/traffic-history-windows-before.jsonl` and `docs/benchmarks/traffic-history-windows-after.jsonl` in the source tree.

## Six-package and single-package Release builds

On 21 November 2026 the same Windows host ran the 128-tunnel workload above after the APK build finished and the Android emulator exited. The order was old, new, new, old, old, new. Each trial transferred and verified 512 MiB over loopback. Both builds already contained the TCP buffer and lazy history changes.

| Median of three trials | Six packages | Single package |
| --- | ---: | ---: |
| Verified duplex throughput | 239.3 MiB/s | 223.8 MiB/s |
| Client working set / private bytes | 11.82 / 6.79 MiB | 11.80 / 6.82 MiB |
| Server working set / private bytes | 12.90 / 7.77 MiB | 13.02 / 7.95 MiB |
| System overview p50 / p95 | 289 / 418 µs | 307 / 446 µs |
| Filtered tunnel page p50 / p95 | 409 / 675 µs | 423 / 589 µs |
| Client page p50 / p95 | 302 / 401 µs | 302 / 452 µs |

The new build’s median throughput was 6.5% lower in this batch. Individual trials ranged from 228.0 to 241.3 MiB/s on the old build and from 195.0 to 227.8 MiB/s on the new build. Several source and dependency changes landed together, so the comparison cannot isolate the cost of package layout. The six samples are in `docs/benchmarks/single-package-windows-before.jsonl` and `docs/benchmarks/single-package-windows-after.jsonl`.

## Memory with 16 KiB relay buffers

Each active TCP relay now requests 16 KiB per direction, saving 32 KiB across both directions per process. The client also reuses the tunnel entries prepared for registration and the server shares its configuration across sessions. Both sides of this comparison used a single Cargo package. The same Windows host repeated the 128-tunnel workload described above with 64 warmed connections and six interleaved trials.

| Median of three trials | Previous build | Current build |
| --- | ---: | ---: |
| Client working set / private bytes | 11.80 / 6.81 MiB | 10.01 / 5.18 MiB |
| Server working set / private bytes | 12.96 / 7.82 MiB | 10.99 / 5.95 MiB |
| Verified duplex throughput | 363.3 MiB/s | 392.9 MiB/s |

The working sets fell in all three trials. Throughput ranged from 342.6 to 420.3 MiB/s before the change and from 387.2 to 413.3 MiB/s after it. The ranges overlap. The samples, including per-trial API timings, are in `docs/benchmarks/flat-windows-memory.jsonl`.

## Time and space costs

| Operation | Time | Auxiliary space and limits |
| --- | --- | --- |
| Control frame with `B` body bytes | O(B) framing and decoding for fixed schema depth. Draining completed data tasks preserves the partially read frame. | One reusable read buffer per reader. Body limits are 4 MiB on control streams and 128 KiB on UDP streams. |
| Login replay detection | Expected O(1) insertion plus O(E) for `E` expired digests removed by this login. Each digest expires once. | Up to 100,000 distinct 32-byte digests, stored in a hash set and an ordered expiry queue. |
| TCP relay, `N` transferred bytes | O(N) | Two 16 KiB copy buffers per relay per process. Kernel, TLS and multiplexing buffers are additional. |
| UDP forwarding, `B` bytes and `P` local peers | O(B) per forwarded datagram, expected O(1) peer lookup | O(P × receive-buffer size + queue slots × payload size). Up to 1,024 peers per forwarder. Each channel has at most 1,024 slots and about 2 MiB of payload capacity, plus metadata. Full outbound queues drop before copying. |
| Data connection pool | Expected O(1) enqueue/dequeue | Space proportional to pool capacity plus waiting tasks. A new stream wakes one waiter. Shutdown wakes all. |
| Config diff, `A` old tunnels, `B` new, `D` changes | Expected O(A+B+D log D) with bounded field lengths | O(A+D) for the old-name index and sorted changes. |
| Tunnel registration and lookup, `T` registered names | O(log T) name comparisons | O(T) registry. Maintaining name order moves sorting work out of each page request. |
| Tunnel page, `K` requested rows | O(T+K) with bounded name and filter lengths. For name length `L` and filter length `Q`, the byte-window search costs up to O(T × L × Q). | O(K) returned records. The scan counts every match for accurate pagination. Row details and statistics are read for the requested page. |
| System overview | O(F+Y) expected for `F` retained offline clients and `Y` protocol types at fixed history size | O(Y) for returned counters. Each of the offline client index and closed tunnel history retains at most 1,024 records. Oldest records are evicted in O(log 1024) ordered-index operations. |
| Client page, `C` online and retained offline clients | O(C log C + Tₚ) worst case with bounded strings, where `Tₚ` is the number of tunnels belonging to clients on this page | O(C+K) transient entries. Online rows precede offline rows, then sort by agent and session ID. |
| Tunnel traffic history, `H` time buckets | Updating bytes in the same period takes O(1) expected time. Clearing expired buckets takes O(H) in the worst case. Reading a chart takes O(H). | Across `T` retained tunnels of which `A` have recorded bytes, tunnel metrics occupy O(T + A × H). An idle tunnel allocates no history array until a nonzero transfer is recorded. A chart copies up to 24 byte-count pairs under the mutex and formats labels after releasing it. |
| Desktop tunnel list, `T` configured routes | The initial view builds O(V) rows for the viewport and its scroll cache. | The configuration occupies O(T). Rows for the viewport and cache occupy O(V) additional space, plus any rows retained during editing or text selection. |
| Desktop activity | Amortized O(1) enqueue and eviction. Unfiltered display reads O(V) visible rows. A filter scans the retained text. | Rust and Flutter each retain at most 500 pending or displayed lines. An unfiltered view uses O(1) additional row-index space. Filtered results retain O(M) references for `M` matches. |

With 500 existing lines, a Dart microbenchmark measured 100,000 evictions and appends at 152.2 ms using a list and 1.09 ms using a ring queue. For 10,000 unfiltered views reading 24 rows, copying all 500 references first took 22.88 ms versus 0.84 ms for direct lookup. These medians cover collection operations in the Dart VM. Flutter layout, painting and desktop process memory were outside this measurement.

## Reproduce

```sh
cargo build --locked --release --bin astrave --bin astrave-server
dart scripts/benchmark.dart target/release
dart scripts/benchmark.dart --logs
```

Pass two binary directories to `dart scripts/benchmark.dart OLD_DIRECTORY NEW_DIRECTORY` to run six trials in the order old, new, new, old, old, new. Each output row includes its build label. Each trial uses fresh loopback ports and configs, verifies echo contents and checks the first name on the fourth tunnel page. The process memory sampler runs on Windows.
