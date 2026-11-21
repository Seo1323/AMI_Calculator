// The accent used to default to this blue and the app saved it on first start, so every existing install
// has it stored. Once (accentVersion < 1) it counts as "never chosen" and gives way to the new default;
// after that a deliberate pick of the same blue sticks.
export const LEGACY_ACCENT = '#2f6fed';
export const ACCENT_VERSION = 1;

export function keepAccent(accent: string | undefined, version: number | undefined): boolean {
	return accent !== undefined && !(accent === LEGACY_ACCENT && (version ?? 0) < ACCENT_VERSION);
}
