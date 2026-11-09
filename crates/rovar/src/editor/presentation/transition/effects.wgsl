fn blend_colors(a: vec4<f32>, b: vec4<f32>, t: f32) -> vec4<f32> {
    let alpha = mix(a.a, b.a, t);
    return vec4<f32>(mix(a.rgb * a.a, b.rgb * b.a, t) / max(alpha, 0.000001), alpha);
}

fn over_colors(a: vec4<f32>, b: vec4<f32>) -> vec4<f32> {
    let alpha = b.a + a.a * (1.0 - b.a);
    return vec4<f32>((b.rgb * b.a + a.rgb * a.a * (1.0 - b.a)) / max(alpha, 0.000001), alpha);
}

fn first_at(input: EffectInput, uv: vec2<f32>) -> vec4<f32> {
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) { return vec4<f32>(0.0); }
    return sample_effect_image(input, uv);
}

fn second_at(input: EffectInput, uv: vec2<f32>) -> vec4<f32> {
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) { return vec4<f32>(0.0); }
    return sample_effect_second_image(input, uv);
}

fn rotate_uv(input: EffectInput, angle: f32, scale: f32) -> vec2<f32> {
    let aspect = vec2<f32>(input.size.x / max(input.size.y, 1.0), 1.0);
    let p = (input.uv - 0.5) * aspect / scale;
    let c = cos(angle);
    let s = sin(angle);
    return vec2<f32>(c * p.x - s * p.y, s * p.x + c * p.y) / aspect + 0.5;
}

fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let t = clamp(params.slots[0].x, 0.0, 1.0);
    if (t == 0.0) { return sample_effect_image(input, input.uv); }
    if (t == 1.0) { return sample_effect_second_image(input, input.uv); }
    let mode = i32(params.slots[0].y);
    let axis = params.slots[1].xy;
    let a = sample_effect_image(input, input.uv);
    let b = sample_effect_second_image(input, input.uv);
    if (mode == 0) {
        return over_colors(a, second_at(input, input.uv + axis * (1.0 - t)));
    }
    if (mode == 1) {
        return over_colors(first_at(input, input.uv - axis * t), second_at(input, input.uv + axis * (1.0 - t)));
    }
    if (mode == 2) {
        var incoming = second_at(input, (input.uv - 0.5) / mix(0.65, 1.0, t) + 0.5);
        incoming.a *= t;
        return over_colors(a, incoming);
    }
    if (mode == 3) {
        var outgoing = first_at(input, (input.uv - 0.5) / mix(1.0, 0.65, t) + 0.5);
        outgoing.a *= 1.0 - t;
        return over_colors(b, outgoing);
    }
    if (mode == 4) {
        let outgoing = first_at(input, rotate_uv(input, -t * 0.25, 1.0 + t * 0.15));
        let incoming = second_at(input, rotate_uv(input, (1.0 - t) * 0.25, 0.85 + t * 0.15));
        return blend_colors(outgoing, incoming, t);
    }
    if (mode == 5) {
        let width = max(abs(cos(t * 3.14159265)), 0.001);
        let p = input.uv - 0.5;
        let component = dot(p, axis);
        let projection = max(width - sin(t * 6.2831853) * component * 0.55, 0.001);
        let uv = axis * component / projection + (p - axis * component) * width / projection + 0.5;
        var color = first_at(input, uv);
        if (t >= 0.5) { color = second_at(input, uv); }
        return vec4<f32>(color.rgb * (0.7 + width * 0.3), color.a);
    }
    if (mode == 6) {
        let aspect = input.size / max(min(input.size.x, input.size.y), 1.0);
        let radius = length(aspect * 0.5) * t;
        let distance = length((input.uv - 0.5) * aspect);
        let edge = 1.5 / max(min(input.size.x, input.size.y), 1.0);
        return blend_colors(a, b, 1.0 - smoothstep(radius - edge, radius + edge, distance));
    }
    if (mode == 7) {
        let coordinate = dot(input.uv, max(axis, vec2<f32>(0.0))) + dot(1.0 - input.uv, max(-axis, vec2<f32>(0.0)));
        let stripe = fract(coordinate * 10.0);
        let coverage = 1.0 - smoothstep(t - 0.015, t + 0.015, stripe);
        return blend_colors(a, b, coverage);
    }
    let cell = 1.0 + sin(t * 3.14159265) * 40.0;
    let grid = max(input.size / cell, vec2<f32>(1.0));
    let uv = (floor(input.uv * grid) + 0.5) / grid;
    return blend_colors(first_at(input, uv), second_at(input, uv), t);
}
