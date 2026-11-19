<script lang="ts">
	// A dropdown with search: type to filter by label or id (case and diacritics ignored).
	// Hand-made because <datalist> shows no list in WebKitGTK (Linux).
	interface Option {
		id: string;
		label: string;
	}
	interface Props {
		options: Option[];
		value: string;
		placeholder?: string;
		ariaLabel: string;
		/** Always shown at the end, never filtered out (e.g. "custom"). */
		pinned?: Option;
		onchange?: (id: string) => void;
	}
	let { options, value = $bindable(''), placeholder = '', ariaLabel, pinned, onchange }: Props = $props();

	const uid = $props.id();
	const norm = (s: string) => s.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();

	let open = $state(false);
	let query = $state('');
	let active = $state(0);
	let input: HTMLInputElement | undefined = $state();

	const all = $derived(pinned ? [...options, pinned] : options);
	const selected = $derived(all.find((o) => o.id === value));
	const shown = $derived.by(() => {
		const q = norm(query.trim());
		if (!q) return all;
		const hits = options.filter((o) => norm(o.label).includes(q) || norm(o.id).includes(q));
		return pinned ? [...hits, pinned] : hits;
	});

	function choose(o: Option) {
		value = o.id;
		open = false;
		query = '';
		onchange?.(o.id);
	}

	function onKey(e: KeyboardEvent) {
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			open = true;
			active = Math.min(active + 1, shown.length - 1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			active = Math.max(active - 1, 0);
		} else if (e.key === 'Enter' && open && shown[active]) {
			e.preventDefault();
			choose(shown[active]);
		} else if (e.key === 'Escape') {
			open = false;
			query = '';
		}
	}

	$effect(() => {
		// Keep the highlighted row inside the visible part of the list.
		if (open) document.getElementById(`${uid}-opt-${active}`)?.scrollIntoView({ block: 'nearest' });
	});
</script>

<div class="ss">
	<input
		bind:this={input}
		type="text"
		role="combobox"
		autocomplete="off"
		aria-label={ariaLabel}
		aria-expanded={open}
		aria-controls="{uid}-list"
		aria-activedescendant={open && shown[active] ? `${uid}-opt-${active}` : undefined}
		{placeholder}
		value={open ? query : (selected?.label ?? '')}
		onfocus={(e) => {
			open = true;
			active = 0;
			e.currentTarget.select();
		}}
		oninput={(e) => {
			query = e.currentTarget.value;
			open = true;
			active = 0;
		}}
		onblur={() => {
			open = false;
			query = '';
		}}
		onkeydown={onKey}
	/>
	{#if open}
		<ul id="{uid}-list" role="listbox" aria-label={ariaLabel}>
			{#each shown as o, i (o.id)}
				<!-- mousedown, not click: the input blurs (and closes the list) before a click lands -->
				<li
					id="{uid}-opt-{i}"
					role="option"
					aria-selected={o.id === value}
					class:active={i === active}
					class:chosen={o.id === value}
					onmousedown={(e) => {
						e.preventDefault();
						choose(o);
					}}
					onmousemove={() => (active = i)}
				>
					{o.label}
				</li>
			{:else}
				<li class="none" role="presentation">—</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.ss {
		position: relative;
		min-width: 14rem;
	}
	input {
		width: 100%;
	}
	ul {
		position: absolute;
		z-index: 20;
		inset-inline: 0;
		top: calc(100% + 0.25rem);
		margin: 0;
		padding: 0.25rem;
		list-style: none;
		max-height: 16rem;
		overflow-y: auto;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: calc(var(--radius) * 0.7);
		box-shadow: var(--shadow);
	}
	li {
		padding: 0.4rem 0.6rem;
		border-radius: calc(var(--radius) * 0.5);
		cursor: pointer;
	}
	li.active {
		background: var(--accent-soft);
	}
	li.chosen {
		font-weight: 600;
		color: var(--accent-text);
	}
	li.none {
		color: var(--text-muted);
		cursor: default;
	}
</style>
