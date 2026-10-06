fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let position = input.uv * input.size;
    let half_size = params.slots[0].xy * 0.5;
    let centered = position - params.slots[3].xy;
    let c = params.slots[0].z;
    let s = params.slots[0].w;
    let p = vec2<f32>(c * centered.x + s * centered.y, -s * centered.x + c * centered.y);
    var distance: f32;
    if params.slots[3].z > 0.5 {
        let k0 = length(p / max(half_size, vec2<f32>(0.00001)));
        let k1 = length(p / max(half_size * half_size, vec2<f32>(0.00001)));
        distance = select(-min(half_size.x, half_size.y), k0 * (k0 - 1.0) / max(k1, 0.00001), k1 > 0.00001);
    } else {
        let radii = params.slots[1];
        let left = select(radii.w, radii.x, p.y < 0.0);
        let right = select(radii.z, radii.y, p.y < 0.0);
        let r = select(right, left, p.x < 0.0);
        let q = abs(p) - half_size + r;
        distance = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    }
    let clip = params.slots[4];
    let inside = all(position >= clip.xy) && all(position < clip.zw);
    let coverage = (1.0 - smoothstep(-0.5, 0.5, distance)) * params.slots[3].w * select(0.0, 1.0, inside);
    let original = sample_effect_image(input, input.uv);
    let blurred = sample_effect_second_image(input, input.uv);
    let result = mix(vec4<f32>(original.rgb * original.a, original.a), vec4<f32>(blurred.rgb * blurred.a, blurred.a), coverage);
    return vec4<f32>(result.rgb / max(result.a, 0.000001), result.a);
}
