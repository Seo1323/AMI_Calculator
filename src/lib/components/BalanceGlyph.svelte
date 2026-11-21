<script lang="ts">
	import { onMount } from 'svelte';
	import { tilt } from '$lib/balance';

	// Little scale: level when the portfolio is within tolerance, leaning towards the position that is
	// furthest off otherwise. Decoration for the text next to it, so it is hidden from screen readers.
	let { deviation = 0, balanced = true, size = 28 }: { deviation?: number; balanced?: boolean; size?: number } = $props();

	// Starts level and settles into place once on load; the reduce-motion and performance switches end the transition.
	let shown = $state(false);
	onMount(() => requestAnimationFrame(() => (shown = true)));

	const BEAM = 11;
	const angle = $derived(shown ? tilt(deviation, balanced) : 0);
	const rad = $derived((angle * Math.PI) / 180);
	const end = $derived({ dx: BEAM * Math.cos(rad), dy: BEAM * Math.sin(rad) });
</script>

<svg class="glyph" class:off={!balanced} width={size} height={size * 0.78} viewBox="0 0 32 25" aria-hidden="true">
	<path class="post" d="M16 5.5V22M10.5 22h11" />
	<g class="beam" style:transform="rotate({angle}deg)">
		<path d="M4.5 5.5h23" />
	</g>
	{#each [-1, 1] as side (side)}
		<g class="pan" style:transform="translate({16 + side * end.dx}px, {5.5 + side * end.dy}px)">
			<path d="M0 0 -4.5 7M0 0 4.5 7M-5.5 7.2h11a5.5 3 0 0 1-11 0z" />
		</g>
	{/each}
	<circle class="pivot" cx="16" cy="5.5" r="1.6" />
</svg>

<style>
	.glyph {
		flex: none;
		color: var(--accent-text);
		overflow: visible;
	}
	.glyph.off {
		color: var(--warning);
	}
	path {
		fill: none;
		stroke: currentColor;
		stroke-width: 1.6;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	.pivot {
		fill: currentColor;
	}
	.beam {
		transform-origin: 16px 5.5px;
	}
	.beam,
	.pan {
		transition: transform 0.9s cubic-bezier(0.34, 1.56, 0.64, 1);
	}
	.pan path {
		stroke-width: 1.3;
	}
</style>
