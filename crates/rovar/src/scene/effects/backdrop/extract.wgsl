fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let footprint = params.slots[2].zw;
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
