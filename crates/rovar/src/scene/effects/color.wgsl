fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let alpha = sample_effect_image(input, input.uv - params.slots[0].xy / input.size).a;
    return vec4<f32>(params.slots[1].rgb, alpha * params.slots[1].a);
}
