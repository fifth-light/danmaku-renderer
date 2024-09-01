struct ShadowConfigUniform {
    shadow_width: u32,
    shadow_weight: f32,
    texture_width: u32,
    texture_height: u32,
    pixel_width: f32,
    pixel_height: f32
};

struct VertexInput {
    @location(0) position: vec2u
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) tex_coords: vec2f
}

@group(0) @binding(0)
var texture: texture_2d<f32>;
@group(0) @binding(1)
var texture_sampler: sampler;
@group(0) @binding(2)
var<uniform> config: ShadowConfigUniform;

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    let tex_coords = vec2f(
        f32(model.position.x) / f32(config.texture_width),
        f32(model.position.y) / f32(config.texture_height)
    );
    out.tex_coords = tex_coords;
    out.clip_position = vec4f((tex_coords.xy * 2.0 - 1.0) * vec2(1.0, -1.0), 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) f32 {
    var output: f32 = 0.0;
    let shadow_width = i32(config.shadow_width);
    let shadow_width_float = f32(config.shadow_width);
    for (var x: i32 = -shadow_width; x <= shadow_width; x++) {
        for (var y: i32 = -shadow_width; y <= shadow_width; y++) {
            let distance = length(vec2f(f32(x), f32(y)));
            let weight = 1.0 - (distance / shadow_width_float);
            let offset = vec2(f32(x) * config.pixel_width, f32(y) * config.pixel_height);
            let sampled = textureSample(texture, texture_sampler, in.tex_coords + offset);
            let shadow = sampled.r * weight;
            let weighted_shadow = config.shadow_weight * shadow;
            output = max(output, weighted_shadow);
        }
    }
    return clamp(output, 0.0, 1.0);
}