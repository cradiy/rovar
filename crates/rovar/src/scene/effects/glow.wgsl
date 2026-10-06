// Only the light is emitted. The original artwork is painted once above it.
fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let field = sample_effect_second_image(input, input.uv);
    let radius = params.slots[1].x;
    let distance = abs(field.r);
    if (field.g < 0.5 || radius <= 0.0 || distance >= radius) {
        return vec4<f32>(0.0);
    }
    let core = 1.0 - smoothstep(0.0, max(params.slots[1].y, 0.5), distance);
    let spread = distance / radius;
    let halo = exp(-4.0 * spread * spread) * (1.0 - smoothstep(0.7, 1.0, spread));
    let alpha = (1.0 - exp(-params.slots[2].x * (core * 1.8 + halo * 0.45))) * params.slots[0].a;
    return vec4<f32>(params.slots[0].rgb, alpha);
}
