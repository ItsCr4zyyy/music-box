<script lang="ts">
	import { nameColor } from './types';
	let { name, url = null, class: cls = '' }: { name: string; url?: string | null; class?: string } = $props();
	let failed = $state(false);
	$effect(() => {
		url;
		failed = false;
	});
</script>

{#if url && !failed}
	<img src={url} alt="" class="inline-block shrink-0 rounded-full object-cover {cls}" onerror={() => (failed = true)} />
{:else}
	<span class="inline-grid shrink-0 place-items-center rounded-full font-display font-bold text-[#1b1036] uppercase {cls}" style="background: {nameColor(name)}">
		{name.slice(0, 1)}
	</span>
{/if}
