//! Allocation counters compiled only into an explicitly requested diagnostic build.

#[cfg(feature = "diagnostic-allocations")]
use std::alloc::{GlobalAlloc, Layout, System};
#[cfg(feature = "diagnostic-allocations")]
use std::sync::atomic::{AtomicU64, Ordering};

/// A monotonically increasing allocation-count and requested-byte snapshot.
#[derive(Debug, Clone, Copy, Default)]
pub struct Snapshot {
    pub allocations: u64,
    pub bytes: u64,
    /// Bytes allocated and not yet freed, which is the Rust heap's share of the process (`task-2279`).
    pub live: u64,
}

#[cfg(feature = "diagnostic-allocations")]
struct CountingAllocator;

#[cfg(feature = "diagnostic-allocations")]
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "diagnostic-allocations")]
static BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "diagnostic-allocations")]
static LIVE: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "diagnostic-allocations")]
unsafe impl GlobalAlloc for CountingAllocator {
    /// Counts an allocation before delegating unchanged to the system allocator.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        LIVE.fetch_add(layout.size() as u64, Ordering::Relaxed);
        let pointer = unsafe { System.alloc(layout) };
        super::allocation_profile::allocated(pointer, layout.size());
        pointer
    }

    /// Delegates release unchanged, and takes the bytes off the live count.
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        super::allocation_profile::freed(pointer);
        unsafe { System.dealloc(pointer, layout) }
    }

    /// Counts the requested grown size before delegating unchanged to the system allocator.
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(size as u64, Ordering::Relaxed);
        LIVE.fetch_add(size as u64, Ordering::Relaxed);
        LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        super::allocation_profile::freed(pointer);
        let grown = unsafe { System.realloc(pointer, layout, size) };
        super::allocation_profile::allocated(grown, size);
        grown
    }
}

#[cfg(feature = "diagnostic-allocations")]
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Reads the counters without allocating.
#[cfg(feature = "diagnostic-allocations")]
pub fn snapshot() -> Snapshot {
    Snapshot {
        allocations: ALLOCATIONS.load(Ordering::Relaxed),
        bytes: BYTES.load(Ordering::Relaxed),
        live: LIVE.load(Ordering::Relaxed),
    }
}

/// Reads the counters; a normal release build has no wrapper and returns zeroes.
#[cfg(not(feature = "diagnostic-allocations"))]
pub fn snapshot() -> Snapshot {
    Snapshot::default()
}

impl Snapshot {
    /// Answers the counters accumulated since an earlier snapshot.
    pub fn since(self, earlier: Self) -> Self {
        Self {
            allocations: self.allocations.saturating_sub(earlier.allocations),
            bytes: self.bytes.saturating_sub(earlier.bytes),
            live: self.live,
        }
    }
}
