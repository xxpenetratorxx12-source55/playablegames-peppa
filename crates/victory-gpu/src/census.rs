//! The placement census: for the title memory the card uses, who touches
//! it (the title, the card, or both) and where each copy of it lies (the
//! title's own memory, our copies in the PC's memory, the card's memory),
//! so that a log says how much of the process's memory is a second copy
//! and how much of that a single copy could do without.
//!
//! On the console the CPU and GPU share one memory, so a title writes a
//! buffer or texture once and the GPU reads those bytes. Here a discrete
//! card has memory of its own: what the card reads fast must be in it, and
//! the title's bytes stay in the PC's memory beside it. Some memory has a
//! third copy, ours in the PC's memory between the two (a buffer's copy,
//! a buffer's CPU side, a texture's staging buffer).
//!
//! What a census line counts, by kind of use:
//!
//! - "filled once, read by the card": the title wrote it before the first
//!   use and not since, and no shader writes it. After the first fill
//!   only the card needs it: the copy in the card's memory could be the
//!   only one (our copy in the PC's memory, and in time the title's own
//!   pages, given back and guarded, brought back if the title touches
//!   them).
//! - "the title keeps writing it, the card reads": refilled after the
//!   first fill. One copy the CPU writes and the card reads: a buffer in
//!   card memory the CPU maps (`GPU_UPLOAD`, which needs a resizable BAR:
//!   Smart Access Memory on AMD), if the title never reads it back (a CPU
//!   read of that mapping crosses the bus a few bytes at a time).
//! - "shaders write it": what they wrote goes back to title memory; the
//!   bytes that went are given.
//!
//! The kinds are judged from what happened since each copy was made: a
//! copy made a moment ago counts as filled once. A title address lying in
//! two copies (windows cut finer than a buffer) counts twice.
//!
//! On by default in diagnostics builds: 30 seconds after the start and
//! every 60 seconds after. `VICTORY_CENSUS=0` turns it off; `=N` takes one
//! every N seconds (5 at least). Each runs on a thread of its own.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use windows::Win32::Graphics::Direct3D12::{
    D3D12_FEATURE_ARCHITECTURE1, D3D12_FEATURE_DATA_ARCHITECTURE1, ID3D12Device,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_ADAPTER_DESC1, DXGI_CREATE_FACTORY_FLAGS, IDXGIAdapter1, IDXGIFactory4,
};

use crate::resource::TextureRow;
use crate::shadow::{CopyRow, Lies};

const AMD: u32 = 0x1002;

/// Seconds between censuses (0: none), from `VICTORY_CENSUS`.
fn every() -> u64 {
    static EVERY: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *EVERY.get_or_init(|| match std::env::var("VICTORY_CENSUS").ok().and_then(|v| v.trim().parse::<u64>().ok()) {
        None => 60,
        Some(0) => 0,
        Some(n) => n.max(5),
    })
}

/// Called once a second by the thread that submits (`host::sweep`).
pub(crate) fn tick() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    static RUNNING: AtomicBool = AtomicBool::new(false);
    let every = every();
    if every == 0 {
        return;
    }
    let now = crate::host::now_ms().max(1);
    let next = match NEXT.compare_exchange(0, now + 30_000, Ordering::Relaxed, Ordering::Relaxed) {
        Ok(_) => return,
        Err(next) => next,
    };
    if now < next || RUNNING.swap(true, Ordering::Relaxed) {
        return;
    }
    NEXT.store(now + every * 1000, Ordering::Relaxed);
    let started = std::thread::Builder::new().name("victory-census".into()).spawn(|| {
        for line in take() {
            tracing::info!("placement census: {line}");
        }
        RUNNING.store(false, Ordering::Relaxed);
    });
    if started.is_err() {
        RUNNING.store(false, Ordering::Relaxed);
    }
}

/// What the card and the system say of it.
struct Card {
    desc: Option<DXGI_ADAPTER_DESC1>,
    /// UMA, cache-coherent UMA.
    uma: Option<(bool, bool)>,
    /// `GPU_UPLOAD` heaps.
    mapped: bool,
    /// [budget, in use] of the card's own memory and of the PC's shared with it.
    memory: Option<[[u64; 2]; 2]>,
}

fn card() -> Option<Card> {
    let host = crate::host::host()?;
    let memory = host.video_memory_groups().map(|[own, shared]| [[own[0], own[1]], [shared[0], shared[1]]]);
    Some(Card {
        desc: adapter_desc(&host.device),
        uma: architecture(&host.device),
        mapped: host.mapped_card_memory(),
        memory,
    })
}

fn adapter_desc(device: &ID3D12Device) -> Option<DXGI_ADAPTER_DESC1> {
    // SAFETY: plain queries.
    unsafe {
        CreateDXGIFactory2::<IDXGIFactory4>(DXGI_CREATE_FACTORY_FLAGS(0))
            .and_then(|f| f.EnumAdapterByLuid::<IDXGIAdapter1>(device.GetAdapterLuid()))
            .and_then(|a| a.GetDesc1())
            .ok()
    }
}

fn architecture(device: &ID3D12Device) -> Option<(bool, bool)> {
    let mut a = D3D12_FEATURE_DATA_ARCHITECTURE1::default();
    // SAFETY: writes `a` of the given size.
    unsafe { device.CheckFeatureSupport(D3D12_FEATURE_ARCHITECTURE1, (&raw mut a).cast(), size_of_val(&a) as u32) }
        .ok()?;
    Some((a.UMA.as_bool(), a.CacheCoherentUMA.as_bool()))
}

/// The AMD family a device id belongs to, as far as the ids we know say.
/// RDNA 2 desktop cards are Navi 21 to 24 (the RX 6800 XT is Navi 21,
/// 0x73BF); the ranges are the families' published id blocks.
pub(crate) fn amd_family(vendor: u32, device: u32) -> Option<&'static str> {
    if vendor != AMD {
        return None;
    }
    Some(match device {
        0x73A0..=0x73BF => "RDNA 2, Navi 21 (RX 6800 / 6800 XT / 6900 XT / 6950 XT)",
        0x73C0..=0x73DF => "RDNA 2, Navi 22 (RX 6700 / 6750)",
        0x73E0..=0x73FF => "RDNA 2, Navi 23 (RX 6600 / 6650)",
        0x7420..=0x743F => "RDNA 2, Navi 24 (RX 6400 / 6500)",
        0x7440..=0x749F => "RDNA 3",
        0x7550..=0x75BF => "RDNA 4",
        _ => "AMD, family not known here",
    })
}

/// Counts and bytes of one kind.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sum {
    pub(crate) n: u64,
    /// The title's own memory.
    pub(crate) title: u64,
    /// Our copies in the PC's memory.
    pub(crate) ours: u64,
    /// The card's memory.
    pub(crate) card: u64,
    /// Bytes moved: title to copy, and back.
    pub(crate) moved_in: u64,
    pub(crate) moved_back: u64,
}

impl Sum {
    fn add(&mut self, title: u64, ours: u64, card: u64, moved_in: u64, moved_back: u64) {
        self.n += 1;
        self.title += title;
        self.ours += ours;
        self.card += card;
        self.moved_in += moved_in;
        self.moved_back += moved_back;
    }
}

/// Buffer copies by use, as in the module's notes; in-place ones apart
/// (nothing is copied, so their use is not known).
pub(crate) const COPY_USES: [&str; 4] = [
    "filled once, read by the card",
    "the title keeps writing it, the card reads",
    "shaders write it",
    "in place (no copy: the card reads title memory over the bus)",
];

pub(crate) fn copy_use(row: &CopyRow) -> usize {
    if row.lies == Lies::InPlace {
        3
    } else if row.shader_written {
        2
    } else if row.fills > 1 {
        1
    } else {
        0
    }
}

/// Where a copy's bytes lie: ours in the PC's memory, the card's.
pub(crate) fn copy_bytes(row: &CopyRow) -> (u64, u64) {
    match row.lies {
        Lies::InPlace => (0, 0),
        Lies::PcMemory => (row.len, 0),
        Lies::CardWithCpuSide => (row.cpu_side, row.cpu_side),
        Lies::CardMapped => (0, row.len),
    }
}

/// Title textures by use.
pub(crate) const TEXTURE_USES: [&str; 5] = [
    "the title filled it once, the card reads",
    "the title fills it again and again, the card reads",
    "the card renders to or writes it",
    "the title fills it and the card wrote into it too",
    "no host copy yet",
];

/// `detiles`: how often title memory was detiled into its staging buffer,
/// if it still has one.
pub(crate) fn texture_use(row: &TextureRow, detiles: Option<u64>) -> usize {
    if !row.host {
        4
    } else if !row.filled {
        2
    } else if row.host_wrote {
        3
    } else if detiles.unwrap_or(1) > 1 {
        1
    } else {
        0
    }
}

/// The sums for the copies and the textures.
pub(crate) fn tally(
    copies: &[CopyRow],
    textures: &[TextureRow],
    staging: &std::collections::HashMap<usize, (u64, u64)>,
) -> ([Sum; 4], [Sum; 5]) {
    let mut c = [Sum::default(); 4];
    for row in copies {
        let (ours, card) = copy_bytes(row);
        c[copy_use(row)].add(row.len, ours, card, row.bytes_in, row.bytes_back);
    }
    let mut t = [Sum::default(); 5];
    for row in textures {
        let staged = staging.get(&row.at).copied();
        let ours = staged.map_or(0, |s| s.0);
        t[texture_use(row, staged.map(|s| s.1))].add(row.title_bytes, ours, row.card_bytes, 0, 0);
    }
    (c, t)
}

fn mb(v: u64) -> String {
    format!("{} MB", v >> 20)
}

fn take() -> Vec<String> {
    let mut lines = Vec::new();
    let card = card();
    let (working, private) = crate::copystat::process_memory();
    let mut amd = false;
    let mut rdna2 = false;
    if let Some(card) = &card {
        if let Some(d) = &card.desc {
            let name = String::from_utf16_lossy(&d.Description[..d.Description.iter().position(|&c| c == 0).unwrap_or(128)]);
            let family = amd_family(d.VendorId, d.DeviceId);
            amd = d.VendorId == AMD;
            rdna2 = family.is_some_and(|f| f.starts_with("RDNA 2"));
            lines.push(format!(
                "the card: {} (vendor {:#06x}, device {:#06x}{}), {} of its own, {} of the PC's shared with it",
                name.trim(),
                d.VendorId,
                d.DeviceId,
                family.map(|f| format!(", {f}")).unwrap_or_default(),
                mb(d.DedicatedVideoMemory as u64),
                mb(d.SharedSystemMemory as u64),
            ));
        }
        let uma = match card.uma {
            Some((_, true)) => "cache-coherent UMA: one memory, the card's buffers can be title memory itself at full speed",
            Some((true, false)) => "UMA, not cache coherent",
            Some((false, _)) => "discrete: memory of its own, the PC's memory over the bus",
            None => "not said",
        };
        lines.push(format!(
            "the card's memory: {uma}; card memory the CPU maps (GPU_UPLOAD, resizable BAR{}): {}",
            if amd { " / Smart Access Memory" } else { "" },
            if card.mapped { "yes" } else { "no" },
        ));
        if let Some([own, shared]) = card.memory {
            lines.push(format!(
                "in use now: the card's own {} of a budget of {}, the PC's shared with it {} of {}; the process {} private, {} in physical memory",
                mb(own[1]),
                mb(own[0]),
                mb(shared[1]),
                mb(shared[0]),
                mb(private),
                mb(working),
            ));
        }
    }

    let copies = crate::shadow::census_rows();
    let textures = crate::resource::texture_rows();
    let staging = crate::texture::staging_census();
    let (c, t) = tally(&copies, &textures, &staging);

    let line = |what: &str, s: &Sum| {
        format!(
            "{what}: {} of {} title memory; our copies in the PC's memory {}, the card's {}{}",
            s.n,
            mb(s.title),
            mb(s.ours),
            mb(s.card),
            if s.moved_in + s.moved_back > 0 {
                format!("; moved since made: {} in, {} back", mb(s.moved_in), mb(s.moved_back))
            } else {
                String::new()
            },
        )
    };
    for (what, s) in COPY_USES.iter().zip(&c) {
        if s.n > 0 {
            lines.push(line(&format!("buffers, {what}"), s));
        }
    }
    let by_lies = [Lies::PcMemory, Lies::CardWithCpuSide, Lies::CardMapped, Lies::InPlace].map(|lies| {
        let rows = copies.iter().filter(|r| r.lies == lies);
        let (n, bytes) = rows.fold((0u64, 0u64), |a, r| (a.0 + 1, a.1 + r.len));
        (lies, n, bytes)
    });
    lines.push(format!(
        "buffers by where they lie: {}",
        by_lies.iter().map(|(lies, n, bytes)| format!("{lies:?} {n} of {}", mb(*bytes))).collect::<Vec<_>>().join(", ")
    ));
    let windows = copies.iter().filter(|r| r.window).fold((0u64, 0u64), |a, r| (a.0 + 1, a.1 + r.len));
    lines.push(format!(
        "of the buffers, {} of {} are windows of title memory outside title buffers",
        windows.0,
        mb(windows.1)
    ));
    for (what, s) in TEXTURE_USES.iter().zip(&t) {
        if s.n > 0 {
            lines.push(line(&format!("textures, {what}"), s));
        }
    }
    lines.push(format!(
        "of the textures, {} are placed in title memory; {} get tiles level by level, whose card memory is in the \
         tile counts and not above",
        textures.iter().filter(|r| r.placed).count(),
        textures.iter().filter(|r| r.tiled).count(),
    ));

    let [settled, ever, freed, unwritten, woke] = crate::texture::settle_census();
    if ever > 0 {
        lines.push(format!(
            "settled textures (VICTORY_STAGING_SETTLE_S): {settled} now; since the start {ever} settled, {} of staging let go; \
             looks that found them unwritten {unwritten}, written or unwatched (staged anew) {woke}",
            mb(freed)
        ));
    }

    // What one copy would do without, by kind.
    let ours_all: u64 = c.iter().chain(&t).map(|s| s.ours).sum();
    let title_all: u64 = c.iter().chain(&t).map(|s| s.title).sum();
    let card_all: u64 = c.iter().chain(&t).map(|s| s.card).sum();
    lines.push(format!(
        "in all: title memory the card uses {}, our copies of it in the PC's memory {}, the card's copies {}",
        mb(title_all),
        mb(ours_all),
        mb(card_all),
    ));
    let card_only = c[0].ours + t[0].ours;
    lines.push(format!(
        "could go: our PC-memory copies of what was filled once and only the card reads {} (buffers {}, texture staging {}); \
         to hold the buffers in the card's memory alone it needs {} more of it",
        mb(card_only),
        mb(c[0].ours),
        mb(t[0].ours),
        mb(copies.iter().filter(|r| copy_use(r) == 0 && r.lies == Lies::PcMemory).map(|r| r.len).sum()),
    ));
    let kept_writing = c[1].ours;
    let mapped = card.as_ref().is_some_and(|c| c.mapped);
    lines.push(format!(
        "could go: our PC-memory copies of buffers the title keeps writing {}: {}",
        mb(kept_writing),
        if mapped {
            "card memory the CPU maps is there, where the title never reads them back".to_owned()
        } else if amd {
            "card memory the CPU maps is not there: on a Radeon it needs Smart Access Memory (resizable BAR) on in the \
             firmware, a driver that offers GPU_UPLOAD heaps and the Agility SDK that has them"
                .to_owned()
        } else {
            "card memory the CPU maps is not there (resizable BAR off, or no driver support)".to_owned()
        },
    ));
    lines.push(format!(
        "title memory only the card uses after its first fill: {} (buffers {}, textures {}); given back and guarded, \
         restored when the title touches it, the PC's memory would hold it no more; and {} behind textures the card \
         renders, which the title reads only where it reads them back",
        mb(c[0].title + t[0].title),
        mb(c[0].title),
        mb(t[0].title),
        mb(t[2].title),
    ));
    if rdna2 {
        lines.push(
            "RDNA 2: the console's GPU is RDNA 2 as well, but Direct3D gives no way to place a texture in the card's own \
             tiling, so textures stay converted copies; the card reads the PC's memory over PCIe 4.0 (some 25 GB/s \
             against 512 GB/s of its own), so in-place memory suits what the card reads rarely"
                .to_owned(),
        );
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy(lies: Lies, fills: u64, shader_written: bool) -> CopyRow {
        CopyRow {
            len: 4 << 20,
            window: false,
            lies,
            cpu_side: if lies == Lies::CardWithCpuSide { 4 << 20 } else { 0 },
            fills,
            bytes_in: fills * (4 << 20),
            shader_written,
            bytes_back: if shader_written { 1 << 20 } else { 0 },
        }
    }

    fn texture(at: usize, host: bool, filled: bool, host_wrote: bool) -> TextureRow {
        TextureRow {
            at,
            title_bytes: 1 << 20,
            host,
            card_bytes: if host { 1 << 20 } else { 0 },
            tiled: false,
            filled,
            host_wrote,
            placed: false,
        }
    }

    /// A copy's use follows what moved through it; in place it is its own.
    #[test]
    fn copies_are_judged_by_what_moved() {
        assert_eq!(copy_use(&copy(Lies::PcMemory, 0, false)), 0);
        assert_eq!(copy_use(&copy(Lies::PcMemory, 1, false)), 0);
        assert_eq!(copy_use(&copy(Lies::PcMemory, 2, false)), 1);
        assert_eq!(copy_use(&copy(Lies::CardWithCpuSide, 9, true)), 2);
        assert_eq!(copy_use(&copy(Lies::InPlace, 0, true)), 3);
    }

    /// Ours in the PC's memory and the card's, by where a copy lies.
    #[test]
    fn copies_count_where_their_bytes_lie() {
        assert_eq!(copy_bytes(&copy(Lies::PcMemory, 1, false)), (4 << 20, 0));
        assert_eq!(copy_bytes(&copy(Lies::CardWithCpuSide, 1, false)), (4 << 20, 4 << 20));
        assert_eq!(copy_bytes(&copy(Lies::CardMapped, 1, false)), (0, 4 << 20));
        assert_eq!(copy_bytes(&copy(Lies::InPlace, 1, false)), (0, 0));
    }

    /// A texture refilled counts apart from one filled once; one without a
    /// staging buffer left counts as filled once.
    #[test]
    fn textures_are_judged_by_their_fills() {
        assert_eq!(texture_use(&texture(1, true, true, false), Some(1)), 0);
        assert_eq!(texture_use(&texture(1, true, true, false), None), 0);
        assert_eq!(texture_use(&texture(1, true, true, false), Some(3)), 1);
        assert_eq!(texture_use(&texture(1, true, false, false), Some(3)), 2);
        assert_eq!(texture_use(&texture(1, true, true, true), Some(1)), 3);
        assert_eq!(texture_use(&texture(1, false, false, false), None), 4);
    }

    #[test]
    fn the_tally_adds_staging_to_its_texture() {
        let copies = [copy(Lies::PcMemory, 1, false), copy(Lies::PcMemory, 5, false)];
        let textures = [texture(7, true, true, false), texture(8, true, true, false)];
        let staging = [(7usize, (2u64 << 20, 1u64)), (8, (3 << 20, 4))].into_iter().collect();
        let (c, t) = tally(&copies, &textures, &staging);
        assert_eq!((c[0].n, c[0].ours, c[1].n, c[1].moved_in), (1, 4 << 20, 1, 20 << 20));
        assert_eq!((t[0].n, t[0].ours, t[1].n, t[1].ours), (1, 2 << 20, 1, 3 << 20));
    }

    #[test]
    fn rdna2_cards_are_named() {
        assert!(amd_family(0x1002, 0x73BF).is_some_and(|f| f.contains("Navi 21")));
        assert!(amd_family(0x1002, 0x73DF).is_some_and(|f| f.starts_with("RDNA 2")));
        assert_eq!(amd_family(0x10DE, 0x73BF), None);
    }
}
