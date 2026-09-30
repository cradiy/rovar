let database;
let flushing = false;
let inFlight = false;
let releaseLock;

export function reportError(message) {
    if (!document.getElementById('shell').hidden) {
        document.getElementById('loading').textContent = message;
        document.getElementById('retry').hidden = false;
        return;
    }
    const element = document.getElementById('storage-status');
    element.textContent = message;
    element.hidden = false;
}

export function clearStorageError() {
    const element = document.getElementById('storage-status');
    element.hidden = true;
    element.textContent = '';
}

let loadedFonts = [];
export function fontData() { return loadedFonts; }

export function editorReady() { document.getElementById('shell').hidden = true; }

const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
export function loginMotionAllowed() { return !document.hidden && !reducedMotion.matches; }

export async function initialize() {
    if (!navigator.gpu) throw new Error('Rovar needs WebGPU. Open it in a supported browser on HTTPS or localhost.');
    if (!navigator.locks) throw new Error('This browser does not support workspace locking.');
    // One writer per origin prevents two browser tabs overwriting the same workspace.
    await new Promise((resolve, reject) => {
        navigator.locks.request('rovar-workspace', { ifAvailable: true }, lock => {
            if (!lock) { reject(new Error('Rovar is already open in another tab. Close it, then reload this page.')); return; }
            resolve();
            return new Promise(release => { releaseLock = release; });
        }).catch(reject);
    });
    try {
        database = await new Promise((resolve, reject) => {
            const request = indexedDB.open('rovar-workspace', 1);
            request.onupgradeneeded = () => request.result.createObjectStore('files');
            request.onsuccess = () => resolve(request.result);
            request.onerror = () => reject(request.error);
            request.onblocked = () => reject(new Error('Close other Rovar tabs to open this workspace.'));
        });
        database.onversionchange = () => { database.close(); reportError('Workspace changed in another tab. Export your work and reload.'); };
        const files = await new Promise((resolve, reject) => {
            const tx = database.transaction('files', 'readonly');
            const store = tx.objectStore('files');
            const keys = store.getAllKeys(), values = store.getAll();
            tx.oncomplete = () => resolve(keys.result.map((key, i) => [key, values.result[i]]));
            tx.onabort = () => reject(tx.error);
            tx.onerror = () => reject(tx.error);
        });
        const fonts = await Promise.all(['IBMPlexSans-Regular.ttf', 'IBMPlexSans-SemiBold.ttf', 'Lilex-Regular.ttf', 'NotoSansCJKsc-Regular.otf', 'NotoSansSymbols2-Regular.ttf'].map(async name => {
            const response = await fetch(new URL(`fonts/${name}`, document.baseURI));
            if (!response.ok) throw new Error(`Could not load font: ${name}`);
            return new Uint8Array(await response.arrayBuffer());
        }));
        loadedFonts = fonts;
        document.getElementById('storage-status').hidden = true;
        return { files, fonts };
    } catch (error) { releaseLock?.(); throw error; }
}

export async function persist(changes) {
    inFlight = true;
    try {
        await new Promise((resolve, reject) => {
            const tx = database.transaction('files', 'readwrite');
            const store = tx.objectStore('files');
            for (const [path, bytes] of changes) {
                if (bytes === null) store.delete(path); else store.put(bytes, path);
            }
            tx.oncomplete = resolve;
            tx.onabort = () => reject(tx.error || new Error('Storage transaction aborted'));
            tx.onerror = () => reject(tx.error);
        });
    } finally { inFlight = false; }
}

export function watch(dirty) {
    const flush = async () => {
        if (flushing || !dirty()) return;
        flushing = true;
        try { await window.wasmBindings.flush_workspace(); }
        finally { flushing = false; }
    };
    setInterval(flush, 500);
    document.addEventListener('visibilitychange', () => { if (document.hidden) flush(); });
    window.addEventListener('beforeunload', event => {
        if (dirty() || inFlight) { event.preventDefault(); event.returnValue = ''; }
    });
}

export function downloadBytes(name, bytes) {
    const url = URL.createObjectURL(new Blob([bytes]));
    const link = document.createElement('a'); link.href = url; link.download = name;
    document.body.append(link); link.click(); link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 60000);
}
