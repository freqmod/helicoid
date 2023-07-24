struct RenderPoint {
    v_pos: vec2<f32>,
    s_pos: vec2<f32>,
};

@group(0) @binding(0) var<storage> render_points: array<RenderPoint>;

struct VertexOutput {
    @location(0) t_position: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

@vertex
fn main(@builtin(vertex_index) render_idx: u32) -> VertexOutput {
    var render_point = render_points[render_idx];
    var v_position = vec4<f32>((render_point.v_pos/40.0) - 0.9, 0.0000002, 1.0);
    return VertexOutput(
        render_point.s_pos,
        v_position
    );
}
