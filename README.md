# playablegames-peppa

A fork of [VIctory](https://github.com/Marvisak/VIctory), a compatibility layer
that runs an Xbox Series title natively on Windows from files the user supplies.
It carries [fxceless/victory-fork](https://github.com/fxceless/victory-fork)
and [avatarmanz/VIctory-7900XTX](https://github.com/avatarmanz/VIctory-7900XTX)
(speed on AMD RDNA 3), with fixes for NVIDIA cards and 32 GB of RAM on top.

**AMD and NVIDIA cards are both supported**: the AMD work this tree carries is
kept. **For now, all testing and new work here is on NVIDIA.** The goal is 40 ms
a frame (25 fps) in the world on an RTX 5080, with frame generation covering the
rest. AMD results and fixes are welcome, but they are not tested here.

**This repository contains no game files, code or assets.** You need your own
copy of the game. It is not affiliated with or endorsed by the game's developer
or publisher.

## Where it stands (2026-11-26)

Measured on one PC: Windows 11, RTX 5080 16 GB (driver 621.24), Ryzen 7 5700X3D,
32 GB RAM, 1080p, Series X profile, in the first room from a save, after the
driver had finished compiling shaders. One run per row so far; repeats are next.

| Build and settings | ms a frame |
|---|---|
| This tree, `scripts\play.ps1 -Safe`'s switches + `VICTORY_JOIN_PARTS=8 VICTORY_CARD_BUFFERS_MIN_KB=64 VICTORY_CARD_BUFFERS_MB=2048` | 66.5 (65.0-67.9) |
| This tree, `Play-VIctory.cmd`'s switches (thin recording on) + `VICTORY_CARD_BUFFERS_MB=2048` | 61-67 in the room |
| avatarmanz/VIctory-7900XTX, the first row's switches, same PC | 77.3 |

What is known to be wrong or missing on NVIDIA:

- Four compute programs written for 64-lane waves have no form for NVIDIA's
  32-lane waves and are left out (about one dispatch a frame each). What they
  draw is not known yet. The AMD tree's form of them is refused by the NVIDIA
  driver as well. Work on such programs elsewhere: avatarmanz/VIctory-7900XTX
  #152, #142 and #134; fxceless/victory-fork #1248, #1249 and #1261.
- Without the ray-tracing structure builder, the ray-tracing resolve and ray
  activation programs are left out on purpose: they have no hits to shade
  (`VICTORY_RT_RESOLVE=1` and `VICTORY_ACTIVATE_RAYS=1` bring them back).
- 32 GB of RAM is tight: the process used about 31.8 GB of private memory on
  Series X.
- The first runs of a new build or a new driver are slow while the driver
  compiles shaders in the background (20 minutes or more). The NVIDIA driver
  keeps them by the executable's path, so run a rebuilt exe from the same path.
- From the trees it carries: trees and far buildings flicker outdoors in places,
  rooms with little direct light are too dark without ray tracing, and textures
  that just came into view appear late.

## How to run it

You need Windows 10/11 x64, a CPU with AVX2, a Direct3D 12 GPU with shader
model 6.6 and mesh shaders, Rust (the toolchain is pinned), the MSVC C++ build
tools with a Windows SDK, and your own dump of the title. There are no prebuilt
downloads: build it from source.

```powershell
git clone https://github.com/Peppacodes/playablegames-peppa
cd playablegames-peppa
cargo build --release --locked -p victory
$env:VICTORY_GAME_DIR = 'D:\Games\MyDump'    # your own dump, outside this folder
.\Play-VIctory.cmd pareto-residency
```

The launcher picks the Series S profile and settings tested on RTX cards, and
keeps its shader cache under `%LOCALAPPDATA%\VIctory\cache-pareto`. If the
title's own watchdog ends a run (it can while frames are slow),
set `VICTORY_NO_WATCHDOG=1` first.

The diagnostics build and avatarmanz's launcher work too:

```powershell
copy victory.example.toml victory.toml      # then set your paths, outside this folder
cargo build --locked --profile diag -p victory --features diagnostics
scripts\play.ps1 -Exe <path of your copy's executable>
```

`scripts\play.ps1 -Safe` leaves out the newest, faster ways; try it first when
something is wrong. Before the first run, `target\diag\victory.exe shaders <the
shader archives>` translates every shader ahead of time and
`target\diag\victory.exe warm --used` makes the pipelines earlier runs drew with.
Commands and debugging switches: [docs/README-fxceless.md](docs/README-fxceless.md).
Every `VICTORY_...` switch is described in a comment where it is read.

## Helping

Results, bug reports and pull requests are all welcome.

- **A result** (an issue): your card, CPU, RAM, driver version, console profile,
  resolution, the switches you set, the median ms a frame once the driver had
  stopped compiling, and whether the picture was right.
- **A pull request**: one change per pull request; say what it fixes, how you
  measured it (before and after, same build path, same place in the game) and on
  which card. When you port a change from another tree, name its commit and its
  author.
- Never add game files or anything made from them
  ([docs/asset-policy.md](docs/asset-policy.md)), and keep the title's name out of
  code, commits and pull requests.

## What is different here

- **NVIDIA and 32 GB fixes on avatarmanz's tree** (made before this repository;
  the history has their commits): dynamic descriptor slots given back at
  their flight's fence, which makes thin recording (`VICTORY_THIN`) work on
  NVIDIA; residency budgets for 32 GB hosts; wave32 forms of more programs for
  NVIDIA; the Series S two-plane black screen fixed; one oversized indirect record
  no longer empties the rest of its list (`VICTORY_INDIRECT_REST=0` by default);
  ray-tracing resolve left out without the structure builder; the
  `Play-VIctory.cmd` launcher. Notes: [docs/bug-journal.md](docs/bug-journal.md).
- **Added here**: an opt-in hook for the title's watchdog (`VICTORY_NO_WATCHDOG=1`,
  build 1.0.2.1), and 64 MB stacks for the pipeline workers, since NVIDIA's
  compiler overflowed the default 2 MB on a world compute program.
- **From avatarmanz/VIctory-7900XTX**, in short (the reasons and measurements are
  in [docs/findings.md](docs/findings.md)): list calls written down on the
  title's threads and translated on eight of ours (`VICTORY_THIN`); submissions
  prepared on a thread of ours and joined (`VICTORY_ASYNC_SUBMIT`,
  `VICTORY_JOIN_PARTS`); what shaders wrote brought back beside the write guards
  (`VICTORY_RELAY`, `VICTORY_LAND`); buffer fills done on the card
  (`VICTORY_CARD_STEPS`); cheaper catching of the title's writes; textures uploaded
  on a copy queue of their own; textures that shaders write and the title reads
  sent back to title memory; frame record and replay ([docs/replay.md](docs/replay.md)).

## Credits

This is a tree of other people's work with more on top. The history is kept, and
commits taken from elsewhere name the commit and its author.

- **[VIctory](https://github.com/Marvisak/VIctory)** by Marvisak and its
  contributors: the loader, the runtime services, the Direct3D 12 back end and
  the shader recompiler.
- **[avatarmanz/VIctory-7900XTX](https://github.com/avatarmanz/VIctory-7900XTX)**
  by avatarmanz: the speed work this tree is built on, and the bringing together
  of the forks below.
- **[fxceless/victory-fork](https://github.com/fxceless/victory-fork)**: the fork
  avatarmanz started from, and the people whose pull requests it carries.
- **[DexNom/VIctory](https://github.com/DexNom/VIctory)**: ray tracing fixes,
  cube gradients, sparse textures.
- **[Belobug/VIctory-Belobug-AMD](https://github.com/Belobug/VIctory-Belobug-AMD)**:
  the ambient light idea, pipeline workers, and work by romainhedouin.
- **[FQY55kai/ZJOU-VICTORY](https://github.com/FQY55kai/ZJOU-VICTORY)**.
- **[myokex/victory-amd](https://github.com/myokex/victory-amd)**: the guard over
  every view of a page (with Spexivus, joroyt and tightfall), and Joshua Odom's
  reads into title memory.
- **Geckomacabre** and **cxy0072019-sketch**: the texture read-back.
- **[XCL](https://github.com/ivall/xcl)** (GPL-2.0-only) was read in avatarmanz's
  tree for how the same title behaves under another layer.
- Changes in avatarmanz's tree and here were made with
  [Claude Code](https://claude.com/claude-code).

## License

GPL-3.0-or-later, as VIctory. See [LICENSE](LICENSE); the one file that is not
covered is named in [docs/asset-policy.md](docs/asset-policy.md).
