struct RenderPoint {
    v_pos: vec2<f32>,
    s_pos: vec2<f32>,
};
struct Globals {
    resolution: vec2<f32>,
    offset: vec2<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

struct VertexOutput {
    @location(0) t_position: vec2<f32>,
    @location(1) c_position: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

@vertex
fn main(@location(0) v_pos: vec2<f32>,
 @location(1) t_pos: vec2<f32>,
 @location(2) color_idx: f32) -> VertexOutput {
    var x = (2.0 * (globals.offset.x + v_pos.x) / globals.resolution.x) - 1.0;
    var y = (-2.0 * ((globals.offset.y + v_pos.y) / globals.resolution.y)) + 1.0;
    var v_position = vec4<f32>(x, y, 0.0000002, 1.0);
    return VertexOutput(
        vec2<f32>(t_pos.x, t_pos.y),
        vec2<f32>(color_idx, color_idx),
        v_position,
    );
}
