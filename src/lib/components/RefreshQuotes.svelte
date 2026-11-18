<script lang="ts">
	import { t, fmtDate, errorText } from '$lib/i18n/index.svelte';
	import { quotes, refreshQuotes } from '$lib/quotes.svelte';
</script>

<div class="refresh">
	<button class="btn" disabled={quotes.refreshing} onclick={refreshQuotes}>
		<span class:spin={quotes.refreshing}>⟳</span>
		{quotes.refreshing ? t('quotes.refreshing') : t('quotes.refresh')}
	</button>
	{#if quotes.lastAt && quotes.lastReport}
		<span class="muted small">
			{fmtDate(quotes.lastAt)} · {t('quotes.done', { quotes: quotes.lastReport.quotesUpdated, fx: quotes.lastReport.fxUpdated })}
		</span>
	{/if}
</div>
{#if quotes.lastReport?.failures.length}
	<div class="notice warn small">
		{t('quotes.failed', { list: quotes.lastReport.failures.map((f) => `${f.symbol} (${f.reason})`).join('; ') })}
	</div>
{/if}
{#if quotes.error}
	<div class="notice error small">{errorText(quotes.error)}</div>
{/if}

<style>
	.refresh {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.6rem;
	}
	.small {
		font-size: 0.85rem;
	}
	.spin {
		display: inline-block;
		animation: spin 1s linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
