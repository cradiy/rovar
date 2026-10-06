fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let backdrop = sample_effect_image(input, input.uv);
    let source = sample_effect_second_image(input, input.uv);
    let b = backdrop.rgb;
    let s = source.rgb;
    var blended = s;
    switch u32(params.slots[0].x) {
        case 1u: { blended = b * s; }
        case 2u: { blended = b + s - b * s; }
        case 3u: { blended = select(2.0 * b * s, 1.0 - 2.0 * (1.0 - b) * (1.0 - s), b > vec3<f32>(0.5)); }
        case 4u: { blended = min(b, s); }
        case 5u: { blended = max(b, s); }
        default: {}
    }
    let sa = source.a * params.slots[0].y;
    let ba = backdrop.a;
    let alpha = sa + ba * (1.0 - sa);
    let color = sa * (1.0 - ba) * s + sa * ba * blended + (1.0 - sa) * ba * b;
    return vec4<f32>(color / max(alpha, 0.000001), alpha);
}
