struct Output {
    @location(0) color: vec4<f32>,
};

@group(0)@binding(1)
var atlas_sampler: sampler;

@group(0)@binding(2)
var atlas_texture: texture_2d<f32>;

@group(0)@binding(3)
var palette_sampler: sampler;

@group(0)@binding(4)
var palette_texture: texture_2d<f32>;

struct VertexOutput {
    @location(0) t_position: vec2<f32>,
    @location(1) c_position: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

@fragment
fn main(vo: VertexOutput) -> @location(0) vec4<f32> {
    var font_col = textureSample(atlas_texture, atlas_sampler, vo.t_position);
    var palette_col = textureSample(palette_texture, palette_sampler, vec2<f32>(vo.c_position.x, 1.0));
//    var a = max(max(font_col.r, font_col.g), font_col.b);
var a = font_col.r;
    return vec4(
        palette_col.r * a,
        palette_col.g * a,
        palette_col.b * a,
        palette_col.a * a);
}
