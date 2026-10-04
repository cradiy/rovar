fn sample_canvas_backdrop(input: BackdropInput, params: BackdropParams, offset: vec2<f32>) -> vec4<f32> {
    let low = params.slots[3].xy + vec2<f32>(0.5);
    let high = max(low, params.slots[3].zw - vec2<f32>(0.5));
    let position = clamp(input.position + offset, low, high);
    return sample_blurred_backdrop(input, position - input.position);
}

fn backdrop_effect(input: BackdropInput, params: BackdropParams) -> vec4<f32> {
    let half_size = params.slots[0].xy * 0.5;
    let centered = (input.uv - vec2<f32>(0.5)) * input.size;
    let c = params.slots[0].z;
    let s = params.slots[0].w;
    let p = vec2<f32>(c * centered.x + s * centered.y, -s * centered.x + c * centered.y);
    var distance: f32;
    if params.slots[2].x > 0.5 {
        let k0 = length(p / half_size);
        let k1 = length(p / (half_size * half_size));
        distance = select(-min(half_size.x, half_size.y), k0 * (k0 - 1.0) / max(k1, 0.00001), k1 > 0.00001);
    } else {
        let radii = params.slots[1];
        let left = select(radii.w, radii.x, p.y < 0.0);
        let right = select(radii.z, radii.y, p.y < 0.0);
        let r = select(right, left, p.x < 0.0);
        let q = abs(p) - half_size + r;
        distance = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    }
    let coverage = 1.0 - smoothstep(-0.5, 0.5, distance);
    var color = sample_canvas_backdrop(input, params, vec2<f32>(0.0));
    // The built-in prefilter caps its device radius at 64. Continue sampling in
    // device coordinates above that limit so zoom never caps the authored blur.
    if params.slots[2].y > 64.0 {
        let sigma = sqrt(max(params.slots[2].y * params.slots[2].y - 4096.0, 0.0)) * 0.5;
        var accumulated = vec4<f32>(0.0);
        var total = 0.0;
        for (var y = -4; y <= 4; y += 1) {
            for (var x = -4; x <= 4; x += 1) {
                let offset = vec2<f32>(f32(x), f32(y)) * 0.75;
                let weight = exp(-0.5 * dot(offset, offset));
                let sample = sample_canvas_backdrop(input, params, offset * sigma);
                accumulated += vec4<f32>(sample.rgb * sample.a, sample.a) * weight;
                total += weight;
            }
        }
        let average = accumulated / total;
        color = vec4<f32>(average.rgb / max(average.a, 0.000001), average.a);
    }
    // The editor canvas is opaque. Replace its pixels inside the outline;
    // filtered alpha only normalizes RGB, it is not an overlay opacity. Large
    // kernels can include transparent window margins, otherwise the unblurred
    // image shows through when their reduced alpha is composited source-over.
    return vec4<f32>(color.rgb, coverage);
}
