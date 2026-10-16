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
//! **`task-2279` found a second one: the descriptor heap.** wgpu's DX12 backend creates one shader visible
//! heap for every texture view and buffer binding the device will ever hold at once, sized from
//! `Limits::max_non_sampler_bindings`, whose default is a million. At 32 bytes a descriptor on the card it
//! was measured on, that is a 30.6 MB block the driver keeps in the process's own memory, every byte of it
//! resident, for a window that binds a few dozen textures. [`BINDINGS`] is still hundreds of times what the
//! window uses.
//!
//! Every other limit and feature eframe asks for is kept: the descriptor eframe would have built is built
//! first and only the hint and that one limit are changed in it.

use std::sync::Arc;

/// How many texture views and buffer bindings the device can hold at once: the size of the DX12 descriptor
/// heap. The window binds one view a texture (the font atlas, the glyph atlas, each picture, each canvas the
/// decoration draws) and a few buffers, which is a few dozen and at most a few hundred.
pub const BINDINGS: u32 = 16_384;

/// Ask for a device that allocates in blocks the size of what a text editor draws.
pub fn frugal(mut options: eframe::NativeOptions) -> eframe::NativeOptions {
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        let eframes_own = setup.device_descriptor.clone();
        setup.device_descriptor = Arc::new(move |adapter| {
            let own = eframes_own(adapter);
            eframe::wgpu::DeviceDescriptor {
                memory_hints: eframe::wgpu::MemoryHints::MemoryUsage,
                required_limits: eframe::wgpu::Limits {
                    max_non_sampler_bindings: own
                        .required_limits
                        .max_non_sampler_bindings
                        .min(BINDINGS),
                    ..own.required_limits.clone()
                },
                ..own
            }
        });
    }
    options
}
