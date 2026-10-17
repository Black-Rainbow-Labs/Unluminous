//! A modal's backdrop: the previous frame, shrunk and blurred on the GPU (`task-2288`).
//!
//! `task-2247` made a modal's backdrop a blurred photograph of the window, and the photograph was a screenshot:
//! the modal drawn invisible for a frame, the whole window read back from the GPU, turned into egui colours one
//! pixel at a time, shrunk and blurred on the window's thread, and uploaded again. Opening Settings measured 75
//! to 83 ms on an idle machine and 170 to 235 ms with the processors busy, most of it that round trip.
//!
//! None of it has to leave the GPU. The patched egui-wgpu (`vendor/egui-wgpu`, `WgpuConfiguration::scene_texture`)
//! draws every frame into a texture of its own and hands it to paint callbacks before they are prepared, when it
//! still holds the previous frame, which is the window as it was before the modal. [`SceneBlur`] is the callback:
//! its `prepare` shrinks that texture by halves, resamples it to [`OUTPUT`], and blurs it across and down into a
//! texture egui knows as [`output_texture`]. `modal.rs` draws that texture where it drew the photograph, on the
//! modal's first frame.
//!
//! A painter without the scene texture, which every screenshot test's is, has no [`output_texture`], and the
//! modal takes the photograph as it did.

use eframe::egui_wgpu;
use eframe::wgpu;

/// The size of the blurred picture. Fixed, because egui knows the texture by an id registered once and a new size
/// would need the renderer, which is borrowed while callbacks are prepared. It is stretched over the window, and
/// [`DEVIATION`] is measured in the window's pixels on each axis, so the blur is round whatever the window's shape.
pub const OUTPUT: [u32; 2] = [640, 400];

/// The blur's standard deviation in the window's own pixels: about what shrinking by four and three box blurs of
/// radius two came to on the processor, which is enough that text behind a dialog no longer reads.
pub const DEVIATION: f32 = 10.0;

/// Where a window keeps the id egui draws the blurred picture by, once [`install`] has run.
///
/// In the window's own context rather than a static, because the id belongs to one renderer: a test binary
/// draws many windows, each with a renderer of its own, and an id from one is a texture another does not have.
fn output_slot() -> egui::Id {
    egui::Id::new("unluminous-scene-blur-output")
}

/// The texture egui draws the blur from in this window, or `None` where nothing installed it, which is every
/// window whose painter hands callbacks no scene texture.
///
/// @param ctx - the window's context
pub fn output_texture(ctx: &egui::Context) -> Option<egui::TextureId> {
    ctx.data(|data| data.get_temp(output_slot()))
}

/// What the callback keeps between frames, in the renderer's callback resources.
struct Resources {
    shrink: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Each halving of the scene, kept while the scene stays the same size.
    halves: Vec<wgpu::Texture>,
    /// The scene resampled to [`OUTPUT`], then blurred across into `across`, then down into `output`.
    resampled: wgpu::Texture,
    across: wgpu::Texture,
    output: wgpu::Texture,
    /// One uniform buffer for each pass that reads one, written when the scene's size changes.
    still: wgpu::Buffer,
    horizontal: wgpu::Buffer,
    vertical: wgpu::Buffer,
    /// The scene size the chain and the uniforms were made for.
    made_for: [u32; 2],
    /// The opening last blurred, so a modal that is drawn again is not blurred again.
    blurred: Option<u64>,
}

/// Make the pipelines and the output texture and tell egui about it. Called once, with the window's render state.
///
/// @param render_state - eframe's, from `CreationContext::wgpu_render_state`
/// @param ctx - the window's context, where the output's id is kept
pub fn install(render_state: &egui_wgpu::RenderState, ctx: &egui::Context) {
    let device = &render_state.device;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("unluminous scene blur"),
        source: wgpu::ShaderSource::Wgsl(include_str!("scene_blur.wgsl").into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("unluminous scene blur"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("unluminous scene blur"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = |entry: &str| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("unluminous scene blur"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &[Some(FORMAT.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    };
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("unluminous scene blur"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    });
    let uniform = || {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("unluminous scene blur"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    };
    let output = texture(device, OUTPUT);
    let resources = Resources {
        shrink: pipeline("fs_shrink"),
        blur: pipeline("fs_blur"),
        layout,
        sampler,
        halves: Vec::new(),
        resampled: texture(device, OUTPUT),
        across: texture(device, OUTPUT),
        output: output.clone(),
        still: uniform(),
        horizontal: uniform(),
        vertical: uniform(),
        made_for: [0, 0],
        blurred: None,
    };
    let mut renderer = render_state.renderer.write();
    let id = renderer.register_native_texture(
        device,
        &output.create_view(&wgpu::TextureViewDescriptor::default()),
        wgpu::FilterMode::Linear,
    );
    renderer.callback_resources.insert(resources);
    ctx.data_mut(|data| data.insert_temp(output_slot(), id));
}

/// The format every texture in the chain is kept in. egui samples a texture's values as gamma encoded colour,
/// which is what the framebuffer holds, so the values pass through unchanged.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// A texture the chain draws into and reads from.
fn texture(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("unluminous scene blur"),
        size: wgpu::Extent3d {
            width: size[0].max(1),
            height: size[1].max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

/// The sizes the scene is halved through before it is resampled to [`OUTPUT`]: each half of the one before, for
/// as long as the next half would still be at least as large as the output on both sides.
///
/// @param scene - the scene's size in pixels
pub fn halvings(scene: [u32; 2]) -> Vec<[u32; 2]> {
    let mut sizes = Vec::new();
    let mut size = scene;
    while size[0] / 2 >= OUTPUT[0] && size[1] / 2 >= OUTPUT[1] {
        size = [size[0] / 2, size[1] / 2];
        sizes.push(size);
    }
    sizes
}

/// The paint callback that blurs the previous frame, pushed by a modal on the frame it opens.
pub struct SceneBlur {
    /// Which opening of a modal this is, so the blur is made once for it.
    pub opening: u64,
}

impl egui_wgpu::CallbackTrait for SceneBlur {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(scene) = callback_resources.get::<egui_wgpu::SceneTexture>().cloned() else {
            crate::services::frame_trace::mark("scene-blur-no-scene");
            return Vec::new();
        };
        let Some(resources) = callback_resources.get_mut::<Resources>() else {
            return Vec::new();
        };
        if resources.blurred == Some(self.opening) {
            return Vec::new();
        }
        resources.blurred = Some(self.opening);
        let size = [scene.texture.width(), scene.texture.height()];
        if resources.made_for != size {
            resources.halves =
                halvings(size).into_iter().map(|half| texture(device, half)).collect();
            write_the_uniforms(queue, resources, size);
            resources.made_for = size;
        }
        let mut source = scene.texture.create_view(&wgpu::TextureViewDescriptor::default());
        for half in &resources.halves {
            let target = half.create_view(&wgpu::TextureViewDescriptor::default());
            pass(device, encoder, resources, &resources.shrink, &source, &resources.still, &target);
            source = target;
        }
        let resampled = resources.resampled.create_view(&wgpu::TextureViewDescriptor::default());
        let across = resources.across.create_view(&wgpu::TextureViewDescriptor::default());
        let output = resources.output.create_view(&wgpu::TextureViewDescriptor::default());
        pass(device, encoder, resources, &resources.shrink, &source, &resources.still, &resampled);
        pass(
            device,
            encoder,
            resources,
            &resources.blur,
            &resampled,
            &resources.horizontal,
            &across,
        );
        pass(device, encoder, resources, &resources.blur, &across, &resources.vertical, &output);
        crate::services::frame_trace::mark("scene-blur-made");
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        _render_pass: &mut wgpu::RenderPass<'static>,
        _callback_resources: &egui_wgpu::CallbackResources,
    ) {
    }
}

/// Write each pass's step and deviation for a scene of this size: one texel of [`OUTPUT`] along the pass's
/// direction, and [`DEVIATION`] window pixels in texels of the output on that axis.
fn write_the_uniforms(queue: &wgpu::Queue, resources: &Resources, scene: [u32; 2]) {
    let across = DEVIATION * OUTPUT[0] as f32 / scene[0].max(1) as f32;
    let down = DEVIATION * OUTPUT[1] as f32 / scene[1].max(1) as f32;
    let words = |step: [f32; 2], deviation: f32| -> Vec<u8> {
        [step[0], step[1], deviation.max(0.01), 0.0]
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect()
    };
    queue.write_buffer(&resources.still, 0, &words([0.0, 0.0], 1.0));
    queue.write_buffer(&resources.horizontal, 0, &words([1.0 / OUTPUT[0] as f32, 0.0], across));
    queue.write_buffer(&resources.vertical, 0, &words([0.0, 1.0 / OUTPUT[1] as f32], down));
}

/// One pass of the chain: read `source` through `pipeline` into `target`.
fn pass(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    resources: &Resources,
    pipeline: &wgpu::RenderPipeline,
    source: &wgpu::TextureView,
    uniform: &wgpu::Buffer,
    target: &wgpu::TextureView,
) {
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("unluminous scene blur"),
        layout: &resources.layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(source),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&resources.sampler),
            },
            wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
        ],
    });
    let mut render = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("unluminous scene blur"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    render.set_pipeline(pipeline);
    render.set_bind_group(0, &bind_group, &[]);
    render.draw(0..3, 0..1);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shader parses and validates, which is the first thing the window does with it: a shader that does
    /// not is a window that panics on its first frame.
    #[test]
    fn the_shader_is_valid_wgsl() {
        let module =
            naga::front::wgsl::parse_str(include_str!("scene_blur.wgsl")).unwrap_or_else(|error| {
                panic!("{}", error.emit_to_string(include_str!("scene_blur.wgsl")))
            });
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("the shader validates");
        for entry in ["vs_main", "fs_shrink", "fs_blur"] {
            assert!(
                module.entry_points.iter().any(|point| point.name == entry),
                "{entry} is missing"
            );
        }
    }

    /// A 4K scene is halved twice, to 960 by 540, and then resampled; a scene already the output's size or
    /// smaller is resampled straight away.
    #[test]
    fn the_scene_is_halved_until_the_next_half_would_be_smaller_than_the_output() {
        assert_eq!(halvings([3840, 2160]), vec![[1920, 1080], [960, 540]]);
        assert_eq!(halvings([640, 400]), Vec::<[u32; 2]>::new());
        assert_eq!(halvings([1300, 900]), vec![[650, 450]]);
    }
}
