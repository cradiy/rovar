fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let p = (input.uv - vec2<f32>(0.5)) * input.size;
    let s = params.slots[0].x;
    let c = params.slots[0].y;
    let source = vec2<f32>(c * p.x + s * p.y, -s * p.x + c * p.y);
    let uv = source / input.size + vec2<f32>(0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return vec4<f32>(0.0);
    }
    return sample_effect_image(input, uv);
}
