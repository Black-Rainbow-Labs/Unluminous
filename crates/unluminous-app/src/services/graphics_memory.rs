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

/// How frames are presented: egui-wgpu's own choice (`AutoVsync`, two frames in flight) unless `present` names
/// `fifo`, `mailbox` or `immediate`, or `latency` names a number of frames. Both are only for measuring the
/// choices against each other (`task-2288`); a value that is not one of these changes nothing.
///
/// @param surface - the configuration eframe would use
/// @param present - `UNLUMINOUS_PRESENT`, if it is set
/// @param latency - `UNLUMINOUS_FRAME_LATENCY`, if it is set
pub fn presentation(
    mut surface: eframe::egui_wgpu::SurfaceConfig,
    present: Option<&str>,
    latency: Option<&str>,
) -> eframe::egui_wgpu::SurfaceConfig {
    match present.map(str::trim) {
        Some("fifo") => surface.present_mode = eframe::wgpu::PresentMode::Fifo,
        Some("mailbox") => surface.present_mode = eframe::wgpu::PresentMode::Mailbox,
        Some("immediate") => surface.present_mode = eframe::wgpu::PresentMode::Immediate,
        _ => {}
    }
    if let Some(frames) = latency
        .and_then(|named| named.trim().parse::<u32>().ok())
        .filter(|frames| (1..=3).contains(frames))
    {
        surface.desired_maximum_frame_latency = Some(frames);
    }
    surface
}

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

#[cfg(test)]
mod presentation_tests {
    use super::*;
    use eframe::egui_wgpu::SurfaceConfig;

    /// Nothing named leaves egui-wgpu's own choice alone, and each name changes only what it names.
    #[test]
    fn only_what_is_named_changes() {
        let own = SurfaceConfig::HIGH_THROUGHPUT;
        assert_eq!(presentation(own, None, None), own);
        assert_eq!(presentation(own, Some("vsync"), Some("seven")), own);
        let mailbox = presentation(own, Some("mailbox"), None);
        assert_eq!(mailbox.present_mode, eframe::wgpu::PresentMode::Mailbox);
        assert_eq!(mailbox.desired_maximum_frame_latency, own.desired_maximum_frame_latency);
        assert_eq!(presentation(own, None, Some("1")).desired_maximum_frame_latency, Some(1));
        assert_eq!(
            presentation(own, None, Some("9")).desired_maximum_frame_latency,
            own.desired_maximum_frame_latency
        );
    }
}
