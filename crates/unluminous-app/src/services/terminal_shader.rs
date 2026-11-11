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

/// The wrapper Termoil appends to a shader, from `termoil-render/src/shader.rs`. `TERMOIL_SHADER_CURSOR` comes
/// before it, from [`terminal_cursor_effect::compose`](crate::services::terminal_cursor_effect::compose).
const WRAPPER: &str = r#"

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

/// What a terminal is drawn with: a shader, a cursor effect, or both. Never neither.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub shader: Option<&'static Builtin>,
    pub effect: Option<&'static crate::services::terminal_cursor_effect::Effect>,
}

impl Look {
    /// The name the compiled pipeline is kept under.
    fn key(&self) -> String {
        let name = |found: Option<&str>| found.unwrap_or("").to_owned();
        format!(
            "{}+{}",
            name(self.shader.map(|shader| shader.name)),
            name(self.effect.map(|effect| effect.name))
        )
    }

    /// The whole WGSL compiled for this look: the shader and the effect joined the way Termoil joins them, then
    /// the wrapper.
    pub fn source(&self) -> String {
        let joined = crate::services::terminal_cursor_effect::compose(
            self.shader.map(|shader| shader.source),
            self.effect,
        );
        format!("{joined}{WRAPPER}")
    }

    /// True when the shader or the effect moves on its own.
    pub(crate) fn moves(&self) -> bool {
        self.shader.is_some_and(moves)
            || self.effect.is_some_and(crate::services::terminal_cursor_effect::moves)
    }
}

/// A rectangle in the window's pixels: left, top, right and bottom.
pub type Pixels = [u32; 4];

/// One rectangle of the window to draw a shader over this frame, in the window's pixels.
#[derive(Debug, Clone)]
pub(crate) struct Job {
    look: Look,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    /// The cursor's center inside the rectangle, in pixels, or negative when there is none.
    cursor: [f32; 2],
    /// The parts of the rectangle something is drawn over, which the shader is kept out of (`task-2601`).
    holes: Vec<Pixels>,
    /// True for the pass over the whole window (`task-2612`), whose holes are worked out in the hook from the grids
    /// drawn with their own look.
    whole: bool,
}

/// The rectangles recorded this frame, shared by the grids' callbacks and the hook that draws them.
#[derive(Clone, Default)]
struct Jobs(Arc<Mutex<Vec<Job>>>);

/// The hook: the compiled shaders and what each frame's passes need.
struct Passes {
    jobs: Jobs,
    format: Option<wgpu::TextureFormat>,
    pipelines: HashMap<String, wgpu::RenderPipeline>,
    layout: Option<wgpu::BindGroupLayout>,
    sampler: Option<wgpu::Sampler>,
    /// A texture for each job of a frame, kept while its size stays the same.
    copies: Vec<wgpu::Texture>,
    /// The copy of the whole window the window pass reads, without any shader in it (`task-2612`).
    whole: Option<wgpu::Texture>,
    /// Whether the last frame drew the window pass, so [`whole`](Self::whole) holds that frame.
    drew_whole: bool,
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

    /// The pipeline for a shader and cursor effect, compiled the first time they are drawn into a target of this
    /// format.
    fn pipeline(
        &mut self,
        device: &wgpu::Device,
        look: Look,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        if self.format != Some(format) {
            self.pipelines.clear();
            self.format = Some(format);
        }
        let key = look.key();
        if let Some(found) = self.pipelines.get(&key) {
            return found.clone();
        }
        let layout = self.layout(device);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&key),
            source: wgpu::ShaderSource::Wgsl(look.source().into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("unluminous-terminal-shader"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&key),
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
        self.pipelines.insert(key, pipeline.clone());
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
        if job.whole && self.whole.as_ref().is_some_and(fits) {
            return self.whole.clone().expect("checked");
        }
        if !job.whole && self.copies.get(index).is_some_and(fits) {
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
        if job.whole {
            self.whole = Some(texture.clone());
        } else if index < self.copies.len() {
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
        let recorded = std::mem::take(
            &mut *self.jobs.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
        let jobs = in_the_window(order_the_jobs(recorded), target.width(), target.height());
        self.drew_whole = jobs.iter().any(|job| job.whole);
        if jobs.is_empty() {
            return;
        }
        // Every copy before any draw, so no pass reads what another pass has already shaded (`task-2612`).
        let copies: Vec<wgpu::Texture> = jobs
            .iter()
            .enumerate()
            .map(|(index, job)| {
                let copy = self.copy_for(device, index, job, scene.format());
                copy_out(encoder, scene, &copy, job);
                copy
            })
            .collect();
        let time = self.started.elapsed().as_secs_f32();
        for (job, copy) in jobs.iter().zip(&copies) {
            self.draw_one(device, queue, encoder, target, job, copy, time);
        }
    }

    fn unshaded(&self) -> Option<wgpu::Texture> {
        self.whole.clone().filter(|_| self.drew_whole)
    }
}

impl Passes {
    /// Draw one job's shader over its part of `target`, reading `copy`, and only over the parts nothing is cut out of.
    #[expect(clippy::too_many_arguments)]
    fn draw_one(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
        job: &Job,
        copy: &wgpu::Texture,
        time: f32,
    ) {
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
        let (width, height) = (job.width, job.height);
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
        let pipeline = self.pipeline(device, job.look, format);
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
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        // Once for each part nothing is drawn over, so a menu, a tooltip or a modal over the terminal keeps the
        // colors it was drawn in (`task-2601`).
        let whole = [job.x, job.y, job.x + width, job.y + height];
        for [left, top, right, bottom] in uncovered(whole, &job.holes) {
            pass.set_scissor_rect(left, top, right - left, bottom - top);
            pass.draw(0..3, 0..1);
        }
    }
}

/// Copy a job's part of the frame into the texture its shader reads.
fn copy_out(
    encoder: &mut wgpu::CommandEncoder,
    scene: &wgpu::Texture,
    copy: &wgpu::Texture,
    job: &Job,
) {
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: scene,
            mip_level: 0,
            origin: wgpu::Origin3d { x: job.x, y: job.y, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: copy,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d { width: job.width, height: job.height, depth_or_array_layers: 1 },
    );
}

/// The jobs cut to the window, without the ones left empty.
///
/// @param jobs - the frame's jobs
/// @param width - the window's width in pixels
/// @param height - the window's height in pixels
fn in_the_window(jobs: Vec<Job>, width: u32, height: u32) -> Vec<Job> {
    jobs.into_iter()
        .map(|job| Job {
            width: job.width.min(width.saturating_sub(job.x)),
            height: job.height.min(height.saturating_sub(job.y)),
            ..job
        })
        .filter(|job| job.width > 0 && job.height > 0)
        .collect()
}

/// The window pass first, with every grid drawn in its own look cut out of it, and then the grids (`task-2612`).
///
/// A grid's pass draws only the parts of the grid nothing is drawn over, so those parts are what the window pass
/// leaves to it. A menu or a modal over a grid is a hole in the grid's pass and gets the window's shader.
///
/// @param jobs - the frame's jobs, in the order they were recorded
pub(crate) fn order_the_jobs(jobs: Vec<Job>) -> Vec<Job> {
    let (windows, grids): (Vec<Job>, Vec<Job>) = jobs.into_iter().partition(|job| job.whole);
    let given_to_the_grids: Vec<Pixels> = grids
        .iter()
        .flat_map(|grid| {
            uncovered([grid.x, grid.y, grid.x + grid.width, grid.y + grid.height], &grid.holes)
        })
        .collect();
    // One window pass a frame: a second, from a tab drawn twice, would shade the window twice.
    let window =
        windows.into_iter().last().map(|window| Job { holes: given_to_the_grids, ..window });
    window.into_iter().chain(grids).collect()
}

/// The empty paint callback a grid adds over itself, which records where it is for the hook.
struct Register {
    look: Look,
    /// The cursor's center in points, when the grid shows one.
    cursor: Option<egui::Pos2>,
    /// What is drawn over the grid, in points.
    holes: Vec<egui::Rect>,
    /// True for the pass over the whole window (`task-2612`).
    whole: bool,
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
                whole: None,
                drew_whole: false,
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
        let pixels = |rect: &egui::Rect| -> Pixels {
            let to = |points: f32| (points * info.pixels_per_point).round().max(0.0) as u32;
            [to(rect.left()), to(rect.top()), to(rect.right()), to(rect.bottom())]
        };
        let job = Job {
            look: self.look,
            x: left as u32,
            y: top as u32,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
            cursor,
            holes: self.holes.iter().map(pixels).collect(),
            whole: self.whole,
        };
        jobs.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(job);
    }
}

thread_local! {
    /// The grid [`shade`] was last asked for and what it kept the shader out of, for a test to read.
    static LAST_SHADED: std::cell::RefCell<Option<(egui::Rect, Vec<egui::Rect>)>> =
        const { std::cell::RefCell::new(None) };
}

/// The rectangle of the grid last shaded on this thread and the parts of it the shader was kept out of, and forget
/// it, so a test can ask what the next frame did.
pub fn take_the_last_shaded() -> Option<(egui::Rect, Vec<egui::Rect>)> {
    LAST_SHADED.with(|last| last.borrow_mut().take())
}

/// What the terminals are drawn with just now, or nothing when there is neither a shader nor a cursor effect.
pub fn look_in_use() -> Option<Look> {
    let look = Look { shader: in_use(), effect: crate::services::terminal_cursor_effect::in_use() };
    (look.shader.is_some() || look.effect.is_some()).then_some(look)
}

/// Draw the shader and cursor effect in use over `rect`, a terminal grid that has just been drawn, with its cursor
/// at `cursor`. Nothing while neither is in use. One that moves asks for the next frame.
///
/// **Not over anything drawn above the grid** (`task-2601`). The hook draws after egui has drawn everything, so
/// without this a shader previewed from the appearance picker was drawn over the picker itself, and a right click
/// menu or a tooltip over a terminal took the terminal's glow.
///
/// @param ui - the grid's ui, whose layer is the grid's
/// @param rect - the grid
/// @param cursor - the center of the cursor of the pane with the keyboard, in points
pub fn shade(ui: &egui::Ui, rect: egui::Rect, cursor: Option<egui::Pos2>) {
    // The window's shader is drawn over the terminals instead, unless they keep their own (`task-2612`).
    if crate::services::window_shader::covers_the_terminals() {
        return;
    }
    let Some(look) = look_in_use() else { return };
    let holes = drawn_over(ui.ctx(), ui.layer_id(), rect);
    LAST_SHADED.with(|last| *last.borrow_mut() = Some((rect, holes.clone())));
    register(ui.painter(), rect, look, cursor, holes, false);
    if look.moves() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
    }
}

/// Add the empty paint callback that records `rect` for the hook to draw `look` over.
///
/// @param painter - a painter on the layer the rectangle is drawn in
/// @param rect - the rectangle, in points
/// @param look - the shader and cursor effect
/// @param cursor - the cursor's center in points, when there is one
/// @param holes - what is drawn over the rectangle, in points
/// @param whole - true for the pass over the whole window
pub(crate) fn register(
    painter: &egui::Painter,
    rect: egui::Rect,
    look: Look,
    cursor: Option<egui::Pos2>,
    holes: Vec<egui::Rect>,
    whole: bool,
) {
    painter.add(egui_wgpu::Callback::new_paint_callback(
        rect,
        Register { look, cursor, holes, whole },
    ));
}

/// The rectangles of everything drawn in a layer above `mine` that overlaps `rect`, in window points.
///
/// A modal's layer holds a backdrop the size of the window as well as the dialog, so its area is the whole window
/// and the dialog's own rectangle is asked of `modal::drawn_by_id` instead, the rule the title bar already keeps.
///
/// @param ctx - the window's context
/// @param mine - the grid's layer
/// @param rect - the grid
pub fn drawn_over(ctx: &egui::Context, mine: egui::LayerId, rect: egui::Rect) -> Vec<egui::Rect> {
    let above: Vec<(egui::LayerId, Option<egui::Rect>)> = ctx.memory(|memory| {
        let order: Vec<egui::LayerId> = memory.layer_ids().collect();
        let start = order.iter().position(|layer| *layer == mine).map_or(order.len(), |at| at + 1);
        order[start..]
            .iter()
            .filter(|layer| memory.areas().is_visible(layer))
            .map(|layer| (*layer, memory.area_rect(layer.id)))
            .collect()
    });
    above
        .into_iter()
        .filter_map(|(layer, area)| {
            let drawn = crate::components::modal::drawn_by_id(ctx, layer.id).or(area)?;
            let drawn = match ctx.layer_transform_to_global(layer) {
                Some(transform) => transform * drawn,
                None => drawn,
            };
            let over = drawn.intersect(rect);
            (over.width() > 0.0 && over.height() > 0.0).then_some(over)
        })
        .collect()
}

/// What is left of `whole` once every hole is cut out of it, as rectangles that do not overlap.
///
/// @param whole - the rectangle, in pixels
/// @param holes - the parts to leave out, in pixels
pub fn uncovered(whole: Pixels, holes: &[Pixels]) -> Vec<Pixels> {
    let mut pieces = vec![whole];
    for &[hole_left, hole_top, hole_right, hole_bottom] in holes {
        let mut kept = Vec::new();
        for piece @ [left, top, right, bottom] in pieces {
            let (cut_left, cut_top) = (hole_left.max(left), hole_top.max(top));
            let (cut_right, cut_bottom) = (hole_right.min(right), hole_bottom.min(bottom));
            if cut_left >= cut_right || cut_top >= cut_bottom {
                kept.push(piece);
                continue;
            }
            // Above the hole, below it, and the two sides between.
            kept.push([left, top, right, cut_top]);
            kept.push([left, cut_bottom, right, bottom]);
            kept.push([left, cut_top, cut_left, cut_bottom]);
            kept.push([cut_right, cut_top, right, cut_bottom]);
        }
        pieces = kept.into_iter().filter(|[l, t, r, b]| l < r && t < b).collect();
    }
    pieces
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
            let text = Look { shader: Some(builtin), effect: None }.source();
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

    /// Every cursor effect with no shader, and with each shader, parses and validates the way the hook compiles it.
    #[test]
    fn every_cursor_effect_compiles_alone_and_with_every_shader() {
        use crate::services::terminal_cursor_effect::EFFECTS;
        let check = |look: Look| {
            let text = look.source();
            let module = naga::front::wgsl::parse_str(&text).unwrap_or_else(|problem| {
                panic!("{} does not parse: {}", look.key(), problem.emit_to_string(&text))
            });
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::default(),
            )
            .validate(&module)
            .unwrap_or_else(|problem| panic!("{} does not validate: {problem:?}", look.key()));
        };
        for effect in EFFECTS {
            check(Look { shader: None, effect: Some(effect) });
            for builtin in BUILTINS {
                check(Look { shader: Some(builtin), effect: Some(effect) });
            }
        }
    }

    #[test]
    fn a_hole_is_cut_out_of_the_rectangle_and_nothing_else_is() {
        let area = |pieces: &[Pixels]| -> u32 {
            pieces.iter().map(|[l, t, r, b]| (r - l) * (b - t)).sum()
        };
        assert_eq!(uncovered([0, 0, 100, 50], &[]), vec![[0, 0, 100, 50]]);
        let cut = uncovered([0, 0, 100, 50], &[[20, 10, 60, 30]]);
        assert_eq!(area(&cut), 100 * 50 - 40 * 20);
        assert!(
            cut.iter().all(|[l, t, r, b]| r <= &20 || l >= &60 || b <= &10 || t >= &30),
            "{cut:?}"
        );
        assert!(uncovered([0, 0, 100, 50], &[[0, 0, 200, 200]]).is_empty(), "covered whole");
        assert_eq!(uncovered([0, 0, 100, 50], &[[300, 300, 400, 400]]), vec![[0, 0, 100, 50]]);
        let two = uncovered([0, 0, 100, 100], &[[0, 0, 50, 50], [25, 25, 75, 75]]);
        assert_eq!(
            area(&two),
            100 * 100 - 50 * 50 - (50 * 50 - 25 * 25),
            "the overlap is cut once"
        );
    }

    /// The window pass comes first, once, with every grid's own pass cut out of it, and a menu over a grid is left
    /// to the window pass (`task-2612`).
    #[test]
    fn the_window_pass_comes_first_and_leaves_each_grid_to_its_own_pass() {
        let look = Look { shader: named("crt"), effect: None };
        let job = |x, y, width, height, holes: Vec<Pixels>, whole| Job {
            look,
            x,
            y,
            width,
            height,
            cursor: [-1.0, -1.0],
            holes,
            whole,
        };
        let ordered = order_the_jobs(vec![
            job(10, 10, 100, 100, vec![[20, 20, 40, 40]], false),
            job(0, 0, 400, 300, Vec::new(), true),
            job(200, 0, 50, 50, Vec::new(), false),
        ]);
        assert_eq!(ordered.len(), 3);
        assert!(ordered[0].whole, "the window pass is drawn first");
        assert!(!ordered[1].whole && !ordered[2].whole);
        let window = &ordered[0];
        let cut = uncovered([0, 0, 400, 300], &window.holes);
        let area: u32 = cut.iter().map(|[l, t, r, b]| (r - l) * (b - t)).sum();
        assert_eq!(
            area,
            400 * 300 - (100 * 100 - 20 * 20) - 50 * 50,
            "the menu over the first grid is the window's"
        );
        let only_grids = order_the_jobs(vec![job(0, 0, 10, 10, Vec::new(), false)]);
        assert_eq!(only_grids.len(), 1, "no window pass when none was recorded");
        let two = order_the_jobs(vec![
            job(0, 0, 10, 10, Vec::new(), true),
            job(0, 0, 10, 10, Vec::new(), true),
        ]);
        assert_eq!(two.len(), 1, "one window pass a frame");
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
