// Original diagnostic shader. The full Horizon terrain renderer is a later milestone.
@group(0) @binding(0) var<uniform> camera: mat4x4<f32>;
struct Output { @builtin(position) position: vec4<f32>, @location(0) color: vec3<f32> };
@vertex fn vs(@location(0) position: vec3<f32>, @location(1) color: vec3<f32>) -> Output {
    var o: Output;
    o.position = camera * vec4<f32>(position, 1.0);
    o.color = color;
    return o;
}
@fragment fn fs(i: Output) -> @location(0) vec4<f32> { return vec4<f32>(i.color, 1.0); }
