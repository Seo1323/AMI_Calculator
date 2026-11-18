<script lang="ts">
	import { t } from '$lib/i18n/index.svelte';
</script>

<div class="stack">
	<h1>{t('dashboard.welcome')}</h1>
	<p class="muted intro">{t('dashboard.intro')}</p>

	<div class="tiles">
		<section class="card">
			<h2>{t('dashboard.netWorth')}</h2>
			<p class="muted">{t('common.soon')}</p>
		</section>
		<section class="card">
			<h2>{t('dashboard.monthBudget')}</h2>
			<p class="muted">{t('common.soon')}</p>
		</section>
		<section class="card">
			<h2>{t('dashboard.allocation')}</h2>
			<p><a href="/rebalance">{t('nav.rebalance')} →</a></p>
		</section>
	</div>
</div>

<style>
	.intro {
		max-width: 60ch;
		margin: 0;
	}
	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
		gap: var(--space);
	}
</style>
