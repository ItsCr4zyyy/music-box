<script lang="ts">
	import type { Snippet } from 'svelte';
	import { HardDrive } from '@lucide/svelte';
	import type { Track } from '#lib/types.ts';
	import { fmtTime } from '#lib/types.ts';
	import Artwork from './Artwork.svelte';

	let {
		track,
		children,
		meta,
		number
	}: { track: Track; children?: Snippet; meta?: Snippet; number?: number } = $props();
</script>

<div class="flex items-center gap-3 rounded-2xl py-2">
	{#if number !== undefined}
		<span class="w-5 shrink-0 text-center font-display font-bold text-white/30">{number}</span>
	{/if}
	<Artwork url={track.artwork_url} seed={track.artist + track.title} />
	<div class="min-w-0 flex-1">
		<div class="truncate font-bold leading-snug">{track.title}</div>
		<div class="flex items-center gap-1.5 text-sm text-white/60">
			{#if track.source === 'local'}<HardDrive class="size-3.5 shrink-0 text-mint" strokeWidth={2.5} />{/if}
			<span class="truncate">{track.artist}</span>
			{#if track.duration_sec}<span class="shrink-0 text-white/40">· {fmtTime(track.duration_sec)}</span>{/if}
		</div>
		{@render meta?.()}
	</div>
	{#if children}<div class="flex shrink-0 items-center gap-1.5">{@render children()}</div>{/if}
</div>
