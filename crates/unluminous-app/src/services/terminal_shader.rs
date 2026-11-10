//! Termoil's 22 shaders over Unluminous's terminals (`task-2597`).
//!
//! A Termoil shader is a WGSL function, `termoil_shade(uv, color)`, run for every pixel of the frame Termoil drew,
//! with `docs/shaders.md` in Termoil as its contract. The files in `terminal_shaders/` are copies of Termoil's own,
//! and the wrapper below is Termoil's `termoil-render/src/shader.rs` wrapper, so a shader written for one runs in
//! the other unchanged.
//!
//! Unluminous draws a terminal with egui's painter rather than Termoil's renderer, so the pass runs over the
//! window instead. Each grid adds an empty paint callback over the rectangle it was drawn in ([`shade`]), whose
//! `paint` records the rectangle in pixels. The vendored egui-wgpu runs [`Passes`] once egui has drawn the frame
//! and before it is copied to the window or read for a screenshot (`AfterTheFrameHook`), and for each recorded
//! rectangle it copies that part of **this** frame into a texture and draws the shader back over the same part. Reading this frame matters: a callback's own
//! scene texture holds the previous frame, which already has the shader drawn into it, and the glow would grow
//! every frame.
//!
//! What a grid is drawn with is the `terminal.shader` setting, which is per thread like the terminal palette.
//! A window that has no scene texture, which is every screenshot test's, never runs the hook and draws no shader.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use eframe::egui_wgpu;
use eframe::wgpu;

/// One shader that ships with Termoil.
#[derive(Debug)]
pub struct Builtin {
    /// The name `terminal.shader` takes.
    pub name: &'static str,
    /// One line saying what it looks like.
    pub summary: &'static str,
    /// The WGSL source, following Termoil's contract.
    pub source: &'static str,
}

/// Declares a shader from its file in `terminal_shaders/`.
macro_rules! builtin {
    ($name:literal, $summary:literal) => {
        Builtin {
            name: $name,
            summary: $summary,
            source: include_str!(concat!("terminal_shaders/", $name, ".wgsl")),
        }
    };
}

/// Every shader, in the order Termoil's shader list shows them.
pub const BUILTINS: &[Builtin] = &[
    builtin!(
        "neon-glow",
        "The Mac neon look: text bloom, a drifting magenta to cyan wash, a pulsing cursor halo"
    ),
    builtin!("crt", "Curved glass tube, scanlines, aperture grille, color fringes, dark corners"),
    builtin!("retro", "Old terminal monitor: soft curve, glow, scanlines, noise and flicker"),
    builtin!("neon", "Text glows like neon tubing, colors pushed to full saturation"),
    builtin!("amber", "Monochrome amber phosphor, like an IBM 3270"),
    builtin!(
        "green-phosphor",
        "Monochrome green phosphor with a slow phosphor trail, like a VT100"
    ),
    builtin!("scanlines", "Only the dark lines between rows, colors unchanged"),
    builtin!("bloom", "A soft halo of light around bright text"),
    builtin!("vhs", "Worn video tape: wobble, color drift and a rolling noise band"),
    builtin!("glitch", "A digital signal that breaks into shifted, split bands now and then"),
    builtin!("chromatic", "Red and blue slide apart toward the edges, as through a cheap lens"),
    builtin!("vignette", "Corners and edges fall gently into shadow"),
    builtin!("film-grain", "Fine moving grain, stronger in the dark parts"),
    builtin!("tft", "An old LCD seen up close: red, green and blue stripes"),
    builtin!("game-boy", "The four greens of the original handheld, dithered"),
    builtin!("sepia", "Warm brown tones of an old photograph"),
    builtin!("noir", "High contrast black and white"),
    builtin!("hologram", "Projected cyan display with sweeping lines and flicker"),
    builtin!("synthwave", "80s sunset grade: purple shadows, pink highlights, a magenta wash"),
    builtin!("underwater", "Rippling picture under a blue green tint with drifting light"),
    builtin!("cursor-glow", "A soft pulsing light around the cursor"),
    builtin!("blueprint", "White lines on drafting blue with a faint grid"),
];

/// The shader with this name, ignoring case.
pub fn named(name: &str) -> Option<&'static Builtin> {
    let name = name.trim();
    BUILTINS.iter().find(|builtin| builtin.name.eq_ignore_ascii_case(name))
}

/// True when a shader moves on its own, so the window redraws while it is on: Termoil's own test, whether the
/// source reads the time.
pub fn moves(builtin: &Builtin) -> bool {
    builtin
        .source
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .any(|line| line.contains("termoil.time"))
}

thread_local! {
    /// The shader the terminals on this thread are drawn with, from `terminal.shader`. Empty is none.
    static CHOSEN: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    /// A shader drawn for now without being chosen, which the shader picker does as its list is moved through.
    static PREVIEW: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Choose the shader the terminals are drawn with, by name. An empty name, `none`, or a name that is not a shader
/// turns it off. Answers whether the name was a shader.
pub fn choose(name: &str) -> bool {
    let found = named(name);
    CHOSEN.with(|chosen| {
        *chosen.borrow_mut() = found.map(|builtin| builtin.name.to_owned()).unwrap_or_default()
    });
    found.is_some()
}

/// Draw the terminals with a shader for now, or with none for `Some("")`, or put the chosen one back with `None`.
pub fn preview(name: Option<&str>) {
    PREVIEW.with(|preview| *preview.borrow_mut() = name.map(str::to_owned));
}

/// True while the shader picker is previewing, when the shader is drawn even under a modal.
pub fn previewing() -> bool {
    PREVIEW.with(|preview| preview.borrow().is_some())
}

/// The shader the terminals are drawn with just now: the one being previewed, or else the one chosen.
pub fn in_use() -> Option<&'static Builtin> {
    let previewed = PREVIEW.with(|preview| preview.borrow().clone());
    match previewed {
        Some(name) => named(&name),
        None => CHOSEN.with(|chosen| named(&chosen.borrow())),
    }
}

/// The wrapper Termoil appends to a shader, from `termoil-render/src/shader.rs`, with the one constant Termoil
/// defines when it joins a shader to a cursor effect. Unluminous has no cursor effects, so the shader keeps its own
/// cursor light.
const WRAPPER: &str = r#"

const TERMOIL_SHADER_CURSOR: f32 = 1.0;

struct TermoilUniforms {
    size: vec2<f32>,
    cursor: vec2<f32>,
    time: f32,
    pad0: f32,
    pad1: f32,
    pad2: f32,
}

@group(0) @binding(0) var termoil_frame: texture_2d<f32>;
@group(0) @binding(1) var termoil_sampler: sampler;
@group(0) @binding(2) var<uniform> termoil: TermoilUniforms;

struct TermoilVertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn termoil_vertex(@builtin(vertex_index) index: u32) -> TermoilVertexOut {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: TermoilVertexOut;
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

@fragment
fn termoil_fragment(in: TermoilVertexOut) -> @location(0) vec4<f32> {
    let color = textureSampleLevel(termoil_frame, termoil_sampler, in.uv, 0.0);
    return termoil_shade(in.uv, color);
}
"#;

/// One rectangle of the window to draw a shader over this frame, in the window's pixels.
#[derive(Debug, Clone, Copy)]
struct Job {
    shader: &'static Builtin,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    /// The cursor's center inside the rectangle, in pixels, or negative when there is none.
    cursor: [f32; 2],
}

/// The rectangles recorded this frame, shared by the grids' callbacks and the hook that draws them.
#[derive(Clone, Default)]
struct Jobs(Arc<Mutex<Vec<Job>>>);

/// The hook: the compiled shaders and what each frame's passes need.
struct Passes {
    jobs: Jobs,
    format: Option<wgpu::TextureFormat>,
    pipelines: HashMap<&'static str, wgpu::RenderPipeline>,
    layout: Option<wgpu::BindGroupLayout>,
    sampler: Option<wgpu::Sampler>,
    /// A texture for each job of a frame, kept while its size stays the same.
    copies: Vec<wgpu::Texture>,
    started: std::time::Instant,
}

impl Passes {
    /// The bind group layout every shader shares: the frame, a sampler and the uniforms.
    fn layout(&mut self, device: &wgpu::Device) -> wgpu::BindGroupLayout {
        self.layout
            .get_or_insert_with(|| {
                let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
                    binding,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty,
                    count: None,
                };
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("unluminous-terminal-shader"),
                    entries: &[
                        entry(
                            0,
                            wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                        ),
                        entry(1, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                        entry(
                            2,
                            wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                        ),
                    ],
                })
            })
            .clone()
    }

    /// The pipeline for a shader, compiled the first time it is drawn into a target of this format.
    fn pipeline(
        &mut self,
        device: &wgpu::Device,
        shader: &'static Builtin,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        if self.format != Some(format) {
            self.pipelines.clear();
            self.format = Some(format);
        }
        if let Some(found) = self.pipelines.get(shader.name) {
            return found.clone();
        }
        let layout = self.layout(device);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(shader.name),
            source: wgpu::ShaderSource::Wgsl(format!("{}{WRAPPER}", shader.source).into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("unluminous-terminal-shader"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(shader.name),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("termoil_vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("termoil_fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert(shader.name, pipeline.clone());
        pipeline
    }

    /// The texture job `index` copies its part of the frame into, made again when the size changes.
    fn copy_for(
        &mut self,
        device: &wgpu::Device,
        index: usize,
        job: &Job,
        format: wgpu::TextureFormat,
    ) -> wgpu::Texture {
        let fits = |texture: &wgpu::Texture| {
            texture.width() == job.width
                && texture.height() == job.height
                && texture.format() == format
        };
        if self.copies.get(index).is_some_and(fits) {
            return self.copies[index].clone();
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("unluminous-terminal-shader-frame"),
            size: wgpu::Extent3d { width: job.width, height: job.height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        if index < self.copies.len() {
            self.copies[index] = texture.clone();
        } else {
            self.copies.push(texture.clone());
        }
        texture
    }
}

impl egui_wgpu::AfterTheFrame for Passes {
    fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::Texture,
        target: &wgpu::Texture,
    ) {
        let jobs = std::mem::take(
            &mut *self.jobs.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
        if jobs.is_empty() {
            return;
        }
        let format = target.format();
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = self
            .sampler
            .get_or_insert_with(|| {
                device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("unluminous-terminal-shader"),
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                })
            })
            .clone();
        let time = self.started.elapsed().as_secs_f32();
        for (index, job) in jobs.iter().enumerate() {
            // Inside the window, and not empty, or there is nothing to copy.
            let width = job.width.min(target.width().saturating_sub(job.x));
            let height = job.height.min(target.height().saturating_sub(job.y));
            if width == 0 || height == 0 {
                continue;
            }
            let job = Job { width, height, ..*job };
            let copy = self.copy_for(device, index, &job, scene.format());
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: scene,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: job.x, y: job.y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &copy,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            );
            let values: [f32; 8] =
                [width as f32, height as f32, job.cursor[0], job.cursor[1], time, 0.0, 0.0, 0.0];
            let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("unluminous-terminal-shader"),
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bytes: Vec<u8> = values.iter().flat_map(|value| value.to_le_bytes()).collect();
            queue.write_buffer(&uniforms, 0, &bytes);
            let layout = self.layout(device);
            let view = copy.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("unluminous-terminal-shader"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry { binding: 2, resource: uniforms.as_entire_binding() },
                ],
            });
            let pipeline = self.pipeline(device, job.shader, format);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("unluminous-terminal-shader"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            pass.set_viewport(job.x as f32, job.y as f32, width as f32, height as f32, 0.0, 1.0);
            pass.set_scissor_rect(job.x, job.y, width, height);
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

/// The empty paint callback a grid adds over itself, which records where it is for the hook.
struct Register {
    shader: &'static Builtin,
    /// The cursor's center in points, when the grid shows one.
    cursor: Option<egui::Pos2>,
}

impl egui_wgpu::CallbackTrait for Register {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        // The hook is put in on the first frame a shader is drawn, and stays.
        if resources.get::<Jobs>().is_none() {
            let jobs = Jobs::default();
            resources.insert(jobs.clone());
            resources.insert(egui_wgpu::AfterTheFrameHook(Box::new(Passes {
                jobs,
                format: None,
                pipelines: HashMap::new(),
                layout: None,
                sampler: None,
                copies: Vec::new(),
                started: std::time::Instant::now(),
            })));
        }
        Vec::new()
    }

    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        _render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(jobs) = resources.get::<Jobs>() else { return };
        let viewport = info.viewport_in_pixels();
        let clip = info.clip_rect_in_pixels();
        let left = viewport.left_px.max(clip.left_px).max(0);
        let top = viewport.top_px.max(clip.top_px).max(0);
        let right = (viewport.left_px + viewport.width_px).min(clip.left_px + clip.width_px);
        let bottom = (viewport.top_px + viewport.height_px).min(clip.top_px + clip.height_px);
        if right <= left || bottom <= top {
            return;
        }
        let cursor = match self.cursor {
            Some(at) => [
                at.x * info.pixels_per_point - left as f32,
                at.y * info.pixels_per_point - top as f32,
            ],
            None => [-1.0, -1.0],
        };
        let job = Job {
            shader: self.shader,
            x: left as u32,
            y: top as u32,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
            cursor,
        };
        jobs.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(job);
    }
}

/// Draw the shader in use over `rect`, a terminal grid that has just been drawn, with its cursor at `cursor`.
/// Nothing while no shader is in use. A shader that moves asks for the next frame.
pub fn shade(ui: &egui::Ui, rect: egui::Rect, cursor: Option<egui::Pos2>) {
    let Some(shader) = in_use() else { return };
    ui.painter().add(egui_wgpu::Callback::new_paint_callback(rect, Register { shader, cursor }));
    if moves(shader) {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shader_defines_termoil_s_function_and_none_declares_a_binding() {
        assert_eq!(BUILTINS.len(), 22);
        for builtin in BUILTINS {
            assert!(
                builtin.source.contains("fn termoil_shade("),
                "{} has no termoil_shade",
                builtin.name
            );
            assert!(
                !builtin.source.contains("@binding"),
                "{} declares its own binding",
                builtin.name
            );
            assert!(
                !builtin.source.contains("const TERMOIL_SHADER_CURSOR"),
                "{} declares the cursor constant",
                builtin.name
            );
        }
    }

    #[test]
    fn every_shader_compiles_with_the_wrapper() {
        for builtin in BUILTINS {
            let text = format!("{}{WRAPPER}", builtin.source);
            let module = naga::front::wgsl::parse_str(&text).unwrap_or_else(|problem| {
                panic!("{} does not parse: {}", builtin.name, problem.emit_to_string(&text))
            });
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::default(),
            )
            .validate(&module)
            .unwrap_or_else(|problem| panic!("{} does not validate: {problem:?}", builtin.name));
        }
    }

    #[test]
    fn a_chosen_shader_is_in_use_and_a_preview_wins_until_it_is_put_back() {
        assert!(choose("neon"));
        assert_eq!(in_use().map(|builtin| builtin.name), Some("neon"));
        assert!(moves(named("neon-glow").expect("shipped")));
        assert!(!moves(named("neon").expect("shipped")));
        preview(Some("crt"));
        assert_eq!(in_use().map(|builtin| builtin.name), Some("crt"));
        preview(Some(""));
        assert!(in_use().is_none(), "previewing none");
        preview(None);
        assert_eq!(in_use().map(|builtin| builtin.name), Some("neon"));
        assert!(!choose("none"));
        assert!(in_use().is_none());
    }
}
