<script lang="ts">
	import { nameColor } from '#lib/types.ts';
	let {
		name,
		url = null,
		class: cls = 'size-6 text-xs'
	}: { name: string; url?: string | null; class?: string } = $props();
	let failed = $state(false);
	$effect(() => {
		url;
		failed = false;
	});
</script>

{#if url && !failed}
	<img src={url} alt={name} title={name} class="inline-block shrink-0 rounded-full object-cover {cls}" onerror={() => (failed = true)} />
{:else}
	<span
		class="inline-grid shrink-0 place-items-center rounded-full font-display font-bold text-[#1b1036] uppercase {cls}"
		style="background: {nameColor(name)}"
		title={name}>{name.slice(0, 1)}</span
	>
{/if}
