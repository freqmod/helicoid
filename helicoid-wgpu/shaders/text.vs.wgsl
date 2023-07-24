struct RenderPoint {
    v_pos: vec2<f32>,
    s_pos: vec2<f32>,
};

@group(0) @binding(0) var<uniform> render_points: RenderPoint;

struct VertexOutput {
    @location(0) v_position: vec2<f32>,
    @location(1) t_position: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

@vertex
fn main(@location(0) a_position: vec2<f32>) -> VertexOutput {
    var position = vec4<f32>(a_position, 0.0000001, 1.0);
    var t_position = vec2<f32>(a_position);
    return VertexOutput(
        a_position,
        t_position,
        position
    );
}
