//! How much memory the graphics device is asked to set aside, which is most of what a window holds.
//!
//! `task-1805` measured a whole Unluminous window at 223 MB of working set and the graphics driver at 55% of
//! it, and concluded there was no care inside Unluminous that could get that back. `task-2218` found one
//! thing that can, and it is a setting rather than care.
//!
//! wgpu suballocates its buffers and textures out of large blocks, and how large is a hint the device is
//! created with. eframe asks for the default, `MemoryHints::Performance`, which on DX12 takes device memory
//! in blocks of at least 128 MB and host memory, which is the machine's own RAM and counts against the
//! process, in blocks of at least 64 MB. A window that draws a few textures and one vertex buffer a frame
//! never fills either. `MemoryHints::MemoryUsage` takes 8 MB and 4 MB blocks instead: the same buffers,
//! packed into blocks the size of what is actually in them. Nothing is drawn differently.
//!
//! Every limit and feature eframe asks for is kept: the descriptor eframe would have built is built first
//! and only the hint is changed in it.

use std::sync::Arc;

/// Ask for a device that allocates in blocks the size of what a text editor draws.
pub fn frugal(mut options: eframe::NativeOptions) -> eframe::NativeOptions {
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        let eframes_own = setup.device_descriptor.clone();
        setup.device_descriptor = Arc::new(move |adapter| eframe::wgpu::DeviceDescriptor {
            memory_hints: eframe::wgpu::MemoryHints::MemoryUsage,
            ..eframes_own(adapter)
        });
    }
    options
}
