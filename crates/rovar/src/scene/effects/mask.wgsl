// Keep the original silhouette in red and the working mask in green.
// Opaque intermediate pixels preserve both channels through premultiplication.
fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let alpha = sample_effect_image(input, input.uv).a;
    return vec4<f32>(alpha, alpha, 0.0, 1.0);
}
