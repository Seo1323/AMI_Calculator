import { confirm } from '@tauri-apps/plugin-dialog';
import { t } from '$lib/i18n/index.svelte';

/** Warning confirmation with buttons in the app language (the plugin's default labels are English). */
export const ask = (message: string) =>
	confirm(message, { kind: 'warning', okLabel: t('common.ok'), cancelLabel: t('common.cancel') });
