fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let axis = params.slots[2].xy;
    let pixels = max(dot(input.image_size, axis), 1.0);
    let sigma = max(params.slots[5].x * 0.5 * pixels / dot(input.size, axis), 0.0001);
    let support = ceil(3.0 * sigma);
    // One sample per reduced texel: increasing the radius never opens gaps.
    // Transparent samples beyond the capture still contribute to normalization.
    var weights = 1.0;
    for (var i = 1.0; i <= support; i += 1.0) {
        weights += 2.0 * exp(-0.5 * i * i / (sigma * sigma));
    }
    let coordinate = dot(input.uv, axis) * pixels;
    let first = max(-support, ceil(0.5 - coordinate));
    let last = min(support, floor(pixels - 0.5 - coordinate));
    var total = vec4<f32>(0.0);
    for (var i = first; i <= last; i += 1.0) {
        let weight = exp(-0.5 * i * i / (sigma * sigma));
        let color = sample_effect_image(input, input.uv + axis * i / pixels);
        total += vec4<f32>(color.rgb * color.a, color.a) * weight;
    }
    total /= weights;
    return vec4<f32>(total.rgb / max(total.a, 0.000001), total.a);
}
