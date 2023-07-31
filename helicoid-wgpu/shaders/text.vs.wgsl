struct RenderPoint {
    v_pos: vec2<f32>,
    s_pos: vec2<f32>,
};

//@group(0) @binding(0) var<uniform> render_points: texture_2d<f32>;

struct VertexOutput {
    @location(0) t_position: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

@vertex
fn main(@location(0) position: vec4<f32>) -> VertexOutput {
    var v_pos = position.xy;
    var t_pos = position.zw;
    var v_position = vec4<f32>((v_pos/300.0) - 0.9, 0.0000002, 1.0);
//    var v_position = vec4<f32>((render_point.v_pos/40.0) - 0.9, 0.0000002, 1.0);
    return VertexOutput(
        t_pos,
        v_position,
    );
}
