struct Output {
    @location(0) color: vec4<f32>,
};

@fragment
fn main(
    @location(0) v_position: vec2<f32>,
    @location(1) t_position: vec2<f32>,
    @location(2) atlas_texture: sampler,
) -> Output {
    var color = sample(atlas_texture, t_position);
    return Output(color);
}
