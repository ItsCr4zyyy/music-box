<script lang="ts">
	// Efekt „Běžící pás“ – černý pruh dole s tím, co hraje a co bude dál.
	let { items }: { items: string[] } = $props();
	const text = $derived(items.filter(Boolean).join('   ●   '));
	const duration = $derived(Math.max(20, text.length * 0.25));
</script>

{#if text}
	<div class="mb-fx mb-ticker absolute inset-x-0 bottom-0 z-20 h-[2.6em] overflow-hidden bg-white text-ink">
		<div class="flex h-full w-max items-center whitespace-nowrap" style="animation: mb-marquee {duration}s linear infinite">
			{#each [0, 1] as i (i)}
				<span class="mb-label px-[2em] text-[1.05em] font-bold tracking-[0.18em] uppercase">{text}&nbsp;&nbsp;&nbsp;●</span>
			{/each}
		</div>
	</div>
{/if}

<style>
	@keyframes -global-mb-marquee {
		from {
			transform: translateX(0);
		}
		to {
			transform: translateX(-50%);
		}
	}
</style>
