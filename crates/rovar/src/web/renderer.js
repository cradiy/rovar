import { fontData } from './storage.js';

let renderer;
let loading;

export function ensureRenderer() {
    if (renderer) return Promise.resolve();
    return loading ??= (async () => {
        const module = await import(new URL('rovar-render.js', document.baseURI).href);
        await module.default({ module_or_path: new URL('rovar-render_bg.wasm', document.baseURI) });
        for (const bytes of fontData()) module.add_font(bytes);
        renderer = module;
    })().catch(error => { loading = undefined; throw error; });
}

export function render(svg, width, height, scale, preview, vector) {
    if (!renderer) throw new Error('Renderer has not been loaded');
    return renderer.render(svg, width, height, scale, preview, vector);
}
