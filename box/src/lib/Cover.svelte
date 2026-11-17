<script lang="ts">
	import { Music } from '@lucide/svelte';
	import { nameColor } from './types';

	let { src, seed, spinning }: { src: string | null; seed: string; spinning: boolean } = $props();
	let failed = $state(false);
	$effect(() => {
		src; // nový obal → zkusit znovu
		failed = false;
	});
</script>

<!-- Gramofonová deska s obalem uprostřed (motiv ji může přestylovat přes .mb-vinyl-*) -->
<div class="mb-vinyl relative aspect-square h-[min(52vmin,100%)] shrink-0">
	<div
		class="mb-vinyl-record absolute inset-0 rounded-full shadow-[0_2vmin_0_0_rgb(0_0_0/0.3)]"
		style="background: repeating-radial-gradient(circle, #120a26 0 0.6%, #1f1440 0.6% 1.2%); animation: vinyl 6s linear infinite; animation-play-state: {spinning ? 'running' : 'paused'}"
	>
		<div class="mb-vinyl-cover absolute inset-[18%] overflow-hidden rounded-full ring-[0.6vmin] ring-ink">
			{#if src && !failed}
				<img {src} alt="" referrerpolicy="no-referrer" class="size-full object-cover" onerror={() => (failed = true)} />
			{:else}
				<div class="grid size-full place-items-center text-[#1b1036]" style="background: {nameColor(seed)}"><Music class="size-1/2" strokeWidth={2.5} /></div>
			{/if}
		</div>
		<div class="mb-vinyl-hole absolute inset-[47%] rounded-full bg-ink ring-[0.5vmin] ring-white/20"></div>
	</div>
	<!-- Ramínko gramofonu -->
	<div
		class="mb-vinyl-arm absolute -top-[4%] -right-[6%] h-[55%] w-[3%] origin-top rounded-full bg-gradient-to-b from-white/90 to-white/50 transition-transform duration-700"
		style="transform: rotate({spinning ? 28 : 12}deg)"
	>
		<div class="absolute -bottom-[2%] left-1/2 h-[8%] w-[260%] -translate-x-1/2 rounded-[0.5vmin] bg-white/80"></div>
	</div>
</div>
