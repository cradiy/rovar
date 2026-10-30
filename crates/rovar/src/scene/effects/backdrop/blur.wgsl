fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let half_size = params.slots[0].xy * 0.5;
    let rotation = abs(params.slots[0].zw);
    let extent = vec2<f32>(dot(rotation, half_size), dot(rotation.yx, half_size));
    // Retain the convolution support and bilinear footprint of both passes.
    let texel = input.size / max(input.image_size, vec2<f32>(1.0));
    let margin = vec2<f32>(params.slots[5].x * 1.5 + 0.5) + texel * 3.0;
    if any(abs(input.uv * input.size - params.slots[3].xy) > extent + margin) {
        return vec4<f32>(0.0);
    }
    let axis = params.slots[2].xy;
    let pixels = max(dot(input.image_size, axis), 1.0);
    let sigma = max(params.slots[5].x * 0.5 * pixels / dot(input.size, axis), 0.0001);
    let support = ceil(3.0 * sigma);
    // Pair adjacent texels through premultiplied linear sampling, including
    // transparent edges. Reuse those weights for the Gaussian normalization.
    let exponent = -0.5 / (sigma * sigma);
    let center = sample_effect_image(input, input.uv);
    var total = vec4<f32>(center.rgb * center.a, center.a);
    var weights = 1.0;
    for (var i = 1.0; i <= support; i += 2.0) {
        let weight = exp(i * i * exponent);
        let next_weight = select(0.0, exp((i + 1.0) * (i + 1.0) * exponent), i + 1.0 <= support);
        let pair_weight = weight + next_weight;
        if pair_weight <= 0.0 {
            break;
        }
        let offset = axis * (i + next_weight / pair_weight) / pixels;
        let left = sample_effect_image(input, input.uv - offset);
        let right = sample_effect_image(input, input.uv + offset);
        total += (vec4<f32>(left.rgb * left.a, left.a) + vec4<f32>(right.rgb * right.a, right.a)) * pair_weight;
        weights += 2.0 * pair_weight;
    }
    total /= weights;
    return vec4<f32>(total.rgb / max(total.a, 0.000001), total.a);
}
