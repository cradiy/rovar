export async function start(assets) {
    const chinese = (navigator.languages?.length ? navigator.languages : [navigator.language])
        .map(locale => locale?.toLowerCase().split(/[-_]/)[0])
        .find(language => language === 'zh' || language === 'en') === 'zh';
    document.documentElement.lang = chinese ? 'zh-CN' : 'en';
    const loading = document.getElementById('loading');
    const retry = document.getElementById('retry');
    loading.textContent = chinese ? '正在加载 Rovar…' : 'Loading Rovar…';
    retry.textContent = chinese ? '重试' : 'Try again';
    retry.addEventListener('click', () => location.reload());
    try {
        const bindings = await import(new URL(assets.js, document.baseURI).href);
        window.wasmBindings = bindings;
        await bindings.default({ module_or_path: new URL(assets.wasm, document.baseURI) });
    } catch (error) {
        console.error('Editor initialization failed', error);
        loading.textContent = chinese ? '无法加载应用，请重试。' : 'Could not load the app. Please try again.';
        retry.hidden = false;
    }
}
