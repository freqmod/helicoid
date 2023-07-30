struct Output {
    @location(0) color: vec4<f32>,
};

@group(0)@binding(0)
var atlas_sampler: sampler;

@group(0)@binding(1)
var atlas_texture: texture_2d<f32>;

struct VertexOutput {
    @location(0) t_position: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};


@fragment
fn main(vo: VertexOutput) -> @location(0) vec4<f32>{
    var col = textureSample(atlas_texture, atlas_sampler, vo.t_position);
//    return vec4(col.x,col.y, col.z, col.w);
    var a = col.x + col.y + col.z;
//    return vec4(1.0,1.0,1.0,1.0);
    return vec4(col.x,col.y,col.z,a);
//    return vec4(vo.t_position.x*5.0, vo.t_position.y*20.0, col.x,col.y);
}
