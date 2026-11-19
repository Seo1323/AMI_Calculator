import { isTauri } from '$lib/tauri';

export const UI_SCALE_MIN = 0.5;
export const UI_SCALE_MAX = 2;
export const UI_SCALE_PRESETS = [0.75, 1, 1.25, 1.5, 2];

export const clampUiScale = (v: number) =>
	Number.isFinite(v) ? Math.min(UI_SCALE_MAX, Math.max(UI_SCALE_MIN, v)) : 1;

/**
 * Scales the whole interface. In the desktop app this is a real webview zoom, so media queries
 * and breakpoints react like in a browser. Where that is unavailable (plain browser, Android)
 * it falls back to CSS `zoom`. Returns the CSS zoom in effect (1 when the webview zoom worked),
 * which the caller needs to compute the effective viewport width.
 */
export async function applyUiScale(scale: number): Promise<number> {
	const s = clampUiScale(scale);
	const root = document.documentElement;
	if (isTauri()) {
		try {
			const { getCurrentWebview } = await import('@tauri-apps/api/webview');
			await getCurrentWebview().setZoom(s);
			root.style.removeProperty('zoom');
			return 1;
		} catch {
			/* not supported on this platform: use CSS zoom below */
		}
	}
	root.style.zoom = String(s);
	return s;
}
