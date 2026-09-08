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
}

#[cfg(feature = "diagnostic-allocations")]
struct CountingAllocator;

#[cfg(feature = "diagnostic-allocations")]
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "diagnostic-allocations")]
static BYTES: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "diagnostic-allocations")]
unsafe impl GlobalAlloc for CountingAllocator {
    /// Counts an allocation before delegating unchanged to the system allocator.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    /// Delegates release unchanged; retained resource use remains the operating system's measurement.
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    /// Counts the requested grown size before delegating unchanged to the system allocator.
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(size as u64, Ordering::Relaxed);
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[cfg(feature = "diagnostic-allocations")]
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Reads the counters without allocating; a normal release build returns zeroes and has no wrapper.
pub fn snapshot() -> Snapshot {
    #[cfg(feature = "diagnostic-allocations")]
    {
        return Snapshot {
            allocations: ALLOCATIONS.load(Ordering::Relaxed),
            bytes: BYTES.load(Ordering::Relaxed),
        };
    }
    #[cfg(not(feature = "diagnostic-allocations"))]
    Snapshot::default()
}

impl Snapshot {
    /// Answers the counters accumulated since an earlier snapshot.
    pub fn since(self, earlier: Self) -> Self {
        Self {
            allocations: self.allocations.saturating_sub(earlier.allocations),
            bytes: self.bytes.saturating_sub(earlier.bytes),
        }
    }
}
