<script lang="ts">
	import { Music } from '@lucide/svelte';
	import { nameColor } from '#lib/types.ts';

	let {
		url,
		seed = '',
		class: cls = 'size-12'
	}: { url: string | null; seed?: string; class?: string } = $props();
	let failed = $state(false);
	$effect(() => {
		url;
		failed = false;
	});
</script>

{#if url && !failed}
	<img
		src={url}
		alt=""
		loading="lazy"
		referrerpolicy="no-referrer"
		class="shrink-0 rounded-xl object-cover {cls}"
		onerror={() => (failed = true)}
	/>
{:else}
	<div class="grid shrink-0 place-items-center rounded-xl text-ink {cls}" style="background: {nameColor(seed)}">
		<Music class="size-1/2" strokeWidth={2.5} />
	</div>
{/if}
