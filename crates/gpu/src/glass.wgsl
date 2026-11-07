// Current thick-lens geometry supplies displacement and additive light. Backdrop sampling,
// dispersion and premultiplied compositing match the reference renderer.

struct Params {
    cw: f32,
    ch: f32,
    m: f32,
    pad: f32,
    depth: f32,
    bend_max: f32,
    fringe: f32,
    power: f32,
    band: f32,
    reach: f32,
    floor: f32,
    fall: f32,
    intensity: f32,
    light_x: f32,
    light_y: f32,
    unused: f32,
};

@group(0) @binding(0) var frosted: texture_2d<f32>;
@group(0) @binding(1) var frosted_sampler: sampler;
@group(0) @binding(2) var coverage: texture_2d<f32>;
@group(0) @binding(3) var distance: texture_2d<f32>;
@group(0) @binding(4) var normals: texture_2d<f32>;
@group(0) @binding(5) var<uniform> p: Params;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let q = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(q * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let out = vec2<i32>(i32(pos.x), i32(pos.y));
    let at = out + vec2<i32>(i32(p.m), i32(p.m));
    let c = textureLoad(coverage, at, 0).r;
    if (c <= 0.0) {
        return vec4<f32>(0.0);
    }
    let add = textureLoad(distance, at, 0).r / 255.0;
    let displacement = textureLoad(normals, at, 0).rg;
    let b = vec2<f32>(f32(out.x) + p.pad + 0.5, f32(out.y) + p.pad + 0.5);
    let base = dispersed(b, displacement, p.fringe);
    let rgb = base.rgb + vec3<f32>(add);
    let alpha = clamp(max(base.a, max(rgb.r, max(rgb.g, rgb.b))), 0.0, 1.0);
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(alpha)), alpha) * c;
}

fn pixel(at: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(frosted));
    if (any(at < vec2<i32>(0)) || any(at >= size)) { return vec4<f32>(0.0); }
    return textureLoad(frosted, at, 0);
}
fn sample_at(at: vec2<f32>) -> vec4<f32> {
    let q = at - vec2<f32>(0.5);
    let origin = vec2<i32>(floor(q));
    let fraction = q - floor(q);
    let upper = mix(pixel(origin), pixel(origin + vec2<i32>(1, 0)), fraction.x);
    let lower = mix(pixel(origin + vec2<i32>(0, 1)), pixel(origin + vec2<i32>(1, 1)), fraction.x);
    return mix(upper, lower, fraction.y);
}
fn dispersed(at: vec2<f32>, displacement: vec2<f32>, k: f32) -> vec4<f32> {
    if (k < 0.001 || all(displacement == vec2<f32>(0.0))) {
        let s = sample_at(at + displacement);
        return vec4<f32>(min(s.rgb, vec3<f32>(s.a)), s.a);
    }
    var total = vec4<f32>(0.0); var sum = 0.0;
    for (var i = 1; i < 7; i += 1) {
        let t = f32(i) / 7.0 - 0.5; let weight = 1.0 - abs(2.0 * t);
        let r = sample_at(at + displacement * (1.0 + (t + 0.5) * k));
        let g = sample_at(at + displacement * (1.0 + t * k));
        let b = sample_at(at + displacement * (1.0 + (t - 0.5) * k));
        total += vec4<f32>(r.r, g.g, b.b, max(r.a, max(g.a, b.a))) * weight; sum += weight;
    }
    let s = total / sum;
    return vec4<f32>(min(s.rgb, vec3<f32>(s.a)), s.a);
}
