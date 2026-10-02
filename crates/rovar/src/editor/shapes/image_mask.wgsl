fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let mask = sample_effect_image(input, input.uv);
    let image = sample_effect_second_image(input, input.uv);
    return vec4<f32>(image.rgb, image.a * mask.a);
}
