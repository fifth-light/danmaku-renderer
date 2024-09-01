struct TimestampUniform {
    time_millis: u32,
};

struct ConfigUniform {
    screen_width: u32,
    screen_height: u32,
    line_height: u32,
    lifetime: u32
};

struct VertexInput {
    @location(0) time: u32,
    @location(1) track_type: u32,
    @location(2) track: u32,
    @location(3) line_width: u32,
    @location(4) offset: vec2i,
    @location(5) tex_coords: vec2u,
    @location(6) color: vec3f,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) color: vec3f,
    @location(1) tex_coords: vec2f,
};

@group(0) @binding(0)
var<uniform> timestamp: TimestampUniform;

@group(0) @binding(1)
var<uniform> config: ConfigUniform;

fn coordinates_conv(screen: vec2i) -> vec2f {
    let x = (f32(screen.x) / f32(config.screen_width)) * 2.0 - 1.0;
    let y = 1.0 - (f32(screen.y) / f32(config.screen_height)) * 2.0;
    return vec2f(x, y);
}

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    let progress = f32(timestamp.time_millis - model.time) / f32(config.lifetime);

    var offset_x: i32 = 0;
    var offset_y: i32 = 0;
    switch model.track_type {
        case 0u, default: {
            offset_x = i32(f32(config.screen_width) - f32(config.screen_width + model.line_width) * progress);
            offset_y = i32(config.line_height * (model.track + 1));
        }
        case 1u: {
            offset_x = (i32(config.screen_width) - i32(model.line_width)) / 2;
            offset_y = i32(config.line_height * (model.track + 1));
        }
        case 2u: {
            offset_x = (i32(config.screen_width) - i32(model.line_width)) / 2;
            offset_y = i32(config.screen_height) - i32(config.line_height * model.track);
        }
    }

    if progress < 0.0 || progress >= 1.0 {
        offset_y = -65536;
    }

    let output_x = model.offset.x + offset_x;
    let output_y = offset_y + model.offset.y;

    out.color = model.color;
    out.tex_coords = vec2f(model.tex_coords);
    out.clip_position = vec4f(coordinates_conv(vec2(output_x, output_y)), 0.0, 1.0);
    return out;
}
