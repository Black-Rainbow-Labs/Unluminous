// The modal backdrop's blur, on the GPU (`task-2288`). See `scene_blur.rs`.
//
// Two passes share one triangle that covers the target: `fs_shrink` samples the source with a linear filter,
// which averages the texels under each target pixel, and `fs_blur` is one direction of a Gaussian. The values
// are the framebuffer's own, gamma encoded and premultiplied, and they are averaged as they are, which is what
// `backdrop_blur::blurred` did on the processor.

struct Blur {
    // One texel of the source, in texture coordinates, along the direction of this pass.
    step: vec2<f32>,
    // The Gaussian's standard deviation, in texels of the source along that direction.
    deviation: f32,
    unused: f32,
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
@group(0) @binding(2) var<uniform> blur: Blur;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> Out {
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    var out: Out;
    out.position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

@fragment
fn fs_shrink(in: Out) -> @location(0) vec4<f32> {
    return textureSample(source, linear_sampler, in.uv);
}

@fragment
fn fs_blur(in: Out) -> @location(0) vec4<f32> {
    let two_sigma_squared = 2.0 * blur.deviation * blur.deviation;
    var sum = vec4<f32>(0.0);
    var weights = 0.0;
    // Three deviations either side, which leaves out less than a third of a percent of the curve.
    let reach = i32(ceil(3.0 * blur.deviation));
    for (var at = -reach; at <= reach; at = at + 1) {
        let offset = f32(at);
        let weight = exp(-(offset * offset) / two_sigma_squared);
        sum = sum + textureSample(source, linear_sampler, in.uv + blur.step * offset) * weight;
        weights = weights + weight;
    }
    return sum / weights;
}
