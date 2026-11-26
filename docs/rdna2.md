# AMD RDNA 2 (RX 6800 XT and its kin)

Nothing here has run on an RDNA 2 card yet. This is what the code does on one, read
from the source, and what to look at on the first runs. The tree's AMD measurements are
of an RX 7900 XTX (RDNA 3, [findings.md](findings.md)).

## What adapts by itself

- **Wave size.** The host's wave widths are asked at start (`host wave lanes` in the
  log). An RDNA card runs waves of 32 and 64, so a console wave64 compute program gets
  `WaveSize(64)` and runs as written. Everything for 32-lane hosts (the half-wave
  forms, `VICTORY_WAVE_WIDE`, `VICTORY_WAVE_LOCAL`, the retries without early-outs)
  only runs where the widest wave is 32. The four compute programs NVIDIA leaves out
  (README, "What is known to be wrong") should run here; that is the first thing to see.
- **RDNA float rules.** With an AMD card of waves 32 and 64 the translator writes the
  console's float instructions as the single host operation the driver turns back into
  the same instruction (`VICTORY_RDNA_FLOAT`, all on): 0.8 ms on the 7900 XTX.
- **Descriptor heaps.** The heap size is probed at start, so the NVIDIA cap of
  1,000,000 views does not apply; view indices are clamped on every host. The
  dynamic slots grow with the heap.
- **Card copies of buffers.** Their size limits follow the card's budget: a 16 GB card
  gets 4 MiB / 2048 MiB, as the RTX 5080 does.
- **Indirect dispatches.** On an AMD card the clamp lets a dimension go to 2^20 groups,
  not 65,535 (`VICTORY_INDIRECT_WIDE=0`: 65,535): cutting them made blocky patches.

## What `Play-VIctory.cmd rdna2` changes

The settings are those of `pareto-residency`: most of them (`VICTORY_THIN`,
`VICTORY_JOIN_PARTS`, `VICTORY_RELAY`, `VICTORY_CARD_STEPS`) were made and measured on
the 7900 XTX. On top:

- `VICTORY_CACHE_DIR` is `%LOCALAPPDATA%\VIctory\cache-rdna2`, a cache of its own.
- `VICTORY_WINDOWS_BAR_MB=1024`: windows of title memory lie in card memory the CPU maps
  (`GPU_UPLOAD` heaps). They have no copy in the PC's memory and no list of changes to
  send. The replayed yard frame on the 7900 XTX: 36.3 ms, 31.9 with them. It needs Smart
  Access Memory (resizable BAR) on in the firmware and a driver that offers the heaps;
  without them nothing changes (`card memory the CPU maps` in the log says which).
  1024 is a first guess for a 16 GB card: it stops at 80 % of the card's budget anyway.
- It runs `target\release\victory.exe`, not the RTX build's file name.

## The first runs

1. Build the diagnostics build (README) and run with `rdna2`. Shader compiles take long
   the first time; let the driver finish before measuring.
2. In the log: `host wave lanes` (min 32, max 64), `card memory the CPU maps`
   (supported), and the `placement census:` lines, which name the card and its family
   and say how much memory is a second or third copy, and of what.
3. Look at what the four wave64 programs draw (README) now that they run.
4. Compare `VICTORY_WINDOWS_BAR_MB=0` against the default, same build path, same place.

## What is known of AMD from the trees this carries

- Pixel shaders that read their quad's neighbours with wave reads: on AMD helper lanes
  take no part in them (fixed; square holes in shadows before).
- Unordered early pipelines for everything crashed AMD's compiler (not used).
- A view index past the heap removed the device on RDNA 3 (clamped since).
- The card reads the PC's memory over PCIe 4.0 on this generation, some 25 GB/s against
  512 GB/s of its own on a 6800 XT: whatever the card reads often belongs in its memory.
