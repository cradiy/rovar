fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let footprint = params.slots[2].zw;
    let half_size = params.slots[0].xy * 0.5;
    let rotation = abs(params.slots[0].zw);
    let extent = vec2<f32>(dot(rotation, half_size), dot(rotation.yx, half_size));
    let margin = vec2<f32>(params.slots[5].x * 1.5 + 0.5) + footprint * 3.0;
    if any(abs(input.uv * input.size - params.slots[3].xy) > extent + margin) {
        return vec4<f32>(0.0);
    }
    let taps = vec2<i32>(ceil(footprint));
    var total = vec4<f32>(0.0);
    for (var y = 0; y < taps.y; y += 1) {
        for (var x = 0; x < taps.x; x += 1) {
            let offset = ((vec2<f32>(f32(x), f32(y)) + 0.5) / vec2<f32>(taps) - 0.5) * footprint / input.size;
            let uv = input.uv + offset;
            if all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0)) {
                let color = sample_effect_image(input, uv);
                total += vec4<f32>(color.rgb * color.a, color.a);
            }
        }
    }
    total /= f32(taps.x * taps.y);
    return vec4<f32>(total.rgb / max(total.a, 0.000001), total.a);
}
