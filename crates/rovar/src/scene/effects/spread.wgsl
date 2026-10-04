fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let radius = abs(params.slots[0].z);
    let dilate = params.slots[0].z > 0.0;
    var alpha = select(1.0, 0.0, dilate);
    // One sample per device pixel at ordinary zoom; bound work at extreme zoom.
    let steps = min(ceil(radius), 256.0);
    for (var i = -i32(steps); i <= i32(steps); i += 1) {
        let delta = params.slots[0].xy * f32(i) * radius / max(steps, 1.0);
        let sample = sample_effect_image(input, input.uv + delta / input.size);
        let a = select(sample.a, sample.g, params.slots[0].w > 0.5);
        alpha = select(min(alpha, a), max(alpha, a), dilate);
    }
    if params.slots[0].w > 0.5 {
        return vec4<f32>(sample_effect_image(input, input.uv).r, alpha, 0.0, 1.0);
    }
    return vec4<f32>(1.0, 1.0, 1.0, alpha);
}
