// Match the canvas's one-logical-pixel square dots, including fractional DPI.
fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let grid = params.slots[0];
    let local = input.uv * input.size;
    if params.slots[1].x > 0.5 {
        // Integer document coordinates become pixel boundaries, not dot centers.
        let cell = floor((local - grid.yz) / grid.x + vec2<f32>(0.5));
        let distance = abs(local - (grid.yz + cell * grid.x));
        let coverage = clamp((grid.w + 1.0) * 0.5 - min(distance.x, distance.y), 0.0, 1.0);
        return vec4<f32>(vec3<f32>(0.5), coverage * 0.24 * params.slots[1].y);
    }
    let center = grid.yz + vec2<f32>(grid.w * 0.5);
    let cell = floor((local - center) / grid.x + vec2<f32>(0.5));
    let distance = abs(local - (center + cell * grid.x));
    let coverage = clamp(vec2<f32>((grid.w + 1.0) * 0.5) - distance,
                         vec2<f32>(0.0), vec2<f32>(1.0));
    let visible = select(0.0, 1.0, all(cell >= vec2<f32>(0.0)));
    return vec4<f32>(vec3<f32>(43.0, 46.0, 54.0) / 255.0,
                     coverage.x * coverage.y * visible);
}
