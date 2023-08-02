struct RenderPoint {
    v_pos: vec2<f32>,
    s_pos: vec2<f32>,
};
struct Globals {
    resolution: vec2<f32>,
    scroll_offset: vec2<f32>,
    zoom: f32,
};

@group(0) @binding(0) var<uniform> globals: Globals;

struct VertexOutput {
    @location(0) t_position: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

@vertex
fn main(@location(0) position: vec4<f32>) -> VertexOutput {
    var v_pos = position.xy;
    var t_pos = position.zw;
    var x = (2.0 * v_pos.x / globals.resolution.x) - 1.0; 
    var y = (-2.0 * (v_pos.y / globals.resolution.y)) + 1.0;
    var v_position = vec4<f32>(x, y, 0.0000002, 1.0);
//    var v_position = vec4<f32>((render_point.v_pos/40.0) - 0.9, 0.0000002, 1.0);
    return VertexOutput(
        t_pos,
        v_position,
    );
}
