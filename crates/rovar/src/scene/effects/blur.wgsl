fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let sigma = params.slots[0].z;
    let steps = min(ceil(3.0 * sigma), 64.0);
    var alpha = 0.0;
    var total = 0.0;
    for (var i = -i32(steps); i <= i32(steps); i += 1) {
        let distance = f32(i) * 3.0 * sigma / max(steps, 1.0);
        let weight = exp(-0.5 * distance * distance / max(sigma * sigma, 0.000001));
        alpha += sample_effect_image(input, input.uv + params.slots[0].xy * distance / input.size).a * weight;
        total += weight;
    }
    return vec4<f32>(1.0, 1.0, 1.0, alpha / total);
}
