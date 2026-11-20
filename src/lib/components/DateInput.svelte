<script lang="ts">
	// A date field plus the chosen day written in the app language: the native picker follows the system
	// locale (WebKitGTK shows the US format), so this line is the unambiguous reading of the value.
	import { fmtDay } from '$lib/i18n/index.svelte';

	interface Props {
		value: string;
		oninput?: () => void;
	}
	let { value = $bindable(), oninput }: Props = $props();
</script>

<input type="date" bind:value {oninput} />
{#if value}<span class="muted small">{fmtDay(value)}</span>{/if}
