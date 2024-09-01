struct GlyphConfigUniform {
    texture_width: u32,
    texture_height: u32
};

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) color: vec3f,
    @location(1) tex_coords: vec2f,
};

@group(1) @binding(0)
var texture: texture_2d<f32>;
@group(1) @binding(1)
var shadow_texture: texture_2d<f32>;
@group(1) @binding(2)
var texture_sampler: sampler;
@group(1) @binding(3)
var<uniform> config: GlyphConfigUniform;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let tex_coords = vec2f(
        in.tex_coords.x / f32(config.texture_width),
        in.tex_coords.y / f32(config.texture_height)
    );
    let sampled = textureSample(texture, texture_sampler, tex_coords);
    let shadow_sampled = textureSample(shadow_texture, texture_sampler, tex_coords);
    let alpha = sampled.r;
    let text = vec4(in.color * alpha, alpha);
    let shadow = vec4(vec3(0.0), shadow_sampled.r);
    return shadow + text * alpha;
}
