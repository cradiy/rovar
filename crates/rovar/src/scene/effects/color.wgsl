fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let shifted = sample_effect_image(input, input.uv - params.slots[0].xy / input.size);
    var alpha = shifted.a;
    if params.slots[0].w > 0.5 {
        alpha = sample_effect_image(input, input.uv).r * (1.0 - shifted.g);
    }
    return vec4<f32>(params.slots[1].rgb, alpha * params.slots[1].a);
}
