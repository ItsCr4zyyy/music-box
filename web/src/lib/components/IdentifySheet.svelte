<script lang="ts">
	import { t } from '#lib/i18n.svelte.ts';
	import { Check, LoaderCircle, Search, Undo2, WandSparkles, X } from '@lucide/svelte';
	import { api } from '#lib/api.ts';
	import { toast, toastError } from '#lib/toasts.svelte.ts';
	import type { Candidate } from '#lib/types.ts';
	import Artwork from './Artwork.svelte';
	import { fade, fly } from 'svelte/transition';

	let {
		kind,
		target,
		query,
		onclose,
		ondone
	}: {
		kind: 'album' | 'track';
		target: string;
		query: string;
		onclose: () => void;
		ondone: () => void;
	} = $props();

	// svelte-ignore state_referenced_locally
	let q = $state(query); // výchozí hodnota, dál ji upravuje uživatel
	let results = $state<Candidate[] | null>(null);
	let loading = $state(false);
	let selected = $state<Candidate | null>(null);
	let renameTracks = $state(true);
	let applying = $state(false);

	async function search() {
		if (!q.trim()) return;
		loading = true;
		selected = null;
		try {
			results = await api<Candidate[]>(`/admin/identify?kind=${kind}&q=${encodeURIComponent(q.trim())}`);
		} catch (e) {
			toastError(e);
		} finally {
			loading = false;
		}
	}
	search();

	async function apply() {
		if (!selected) return;
		applying = true;
		try {
			const r = await api<{ updated: number; warning: string | null }>('/admin/identify', {
				body: { kind, target, candidate: $state.snapshot(selected), rename_tracks: kind === 'album' && renameTracks }
			});
			if (r.warning) toast(r.warning, 'error');
			else toast(kind === 'album' ? t('id.doneAlbum', { n: r.updated }) : t('id.doneTrack'), 'success');
			ondone();
		} catch (e) {
			toastError(e);
		} finally {
			applying = false;
		}
	}

	async function reset() {
		if (!confirm(t('id.resetConfirm'))) return;
		try {
			await api('/admin/identify/reset', { body: { kind, target } });
			toast(t('id.resetDone'), 'success');
			ondone();
		} catch (e) {
			toastError(e);
		}
	}

	const sourceLabel = { musicbrainz: 'MusicBrainz', deezer: 'Deezer' };
</script>

<div class="fixed inset-0 z-50 bg-black/60" transition:fade={{ duration: 150 }} onclick={onclose} role="presentation"></div>
<div
	class="fixed inset-x-0 bottom-0 z-50 mx-auto flex max-h-[88dvh] max-w-[480px] flex-col rounded-t-3xl bg-ink-2 ring-1 ring-white/10"
	transition:fly={{ y: 400, duration: 220 }}
>
	<div class="flex items-center gap-2 px-4 pt-4 pb-2">
		<WandSparkles class="size-5 text-pink" strokeWidth={2.5} />
		<h2 class="flex-1 font-display text-xl font-bold">{kind === 'album' ? t('id.titleAlbum') : t('id.titleTrack')}</h2>
		<button class="grid size-10 place-items-center rounded-xl text-white/60" title={t('common.close')} onclick={onclose}><X class="size-5" /></button>
	</div>

	<form
		class="flex gap-2 px-4 pb-3"
		onsubmit={(e) => {
			e.preventDefault();
			search();
		}}
	>
		<input class="input" type="search" enterkeyhint="search" bind:value={q} placeholder={t('id.placeholder')} />
		<button class="btn btn-ghost btn-icon size-12" title={t('common.search')}><Search class="size-5" /></button>
	</form>

	<div class="min-h-0 flex-1 overflow-y-auto px-4">
		{#if loading}
			<div class="flex justify-center py-10 text-pink"><LoaderCircle class="size-8 animate-spin" /></div>
		{:else if results && !results.length}
			<p class="py-8 text-center text-white/50">{t('id.nothing')}</p>
		{:else if results}
			<div class="flex flex-col gap-1.5 pb-3">
				{#each results as c (c.source + c.id)}
					{@const on = selected === c}
					<button
						class="flex items-center gap-3 rounded-2xl p-2 text-left transition {on ? 'bg-pink/20 ring-2 ring-pink' : 'bg-ink-3/60'}"
						onclick={() => (selected = c)}
					>
						<Artwork url={c.cover_url} seed={c.title} class="size-16" />
						<div class="min-w-0 flex-1">
							<div class="truncate font-bold">{c.title}</div>
							<div class="truncate text-sm text-white/70">{c.artist}</div>
							<div class="truncate text-xs text-white/40">
								{[c.album, c.year, c.track_count ? t('id.tracks', { n: c.track_count }) : null, sourceLabel[c.source]].filter(Boolean).join(' · ')}
							</div>
						</div>
						{#if on}<Check class="size-6 shrink-0 text-pink" strokeWidth={3} />{/if}
					</button>
				{/each}
			</div>
		{/if}
	</div>

	<div class="flex flex-col gap-3 border-t border-white/5 px-4 pt-3 pb-[max(1rem,env(safe-area-inset-bottom))]">
		{#if kind === 'album'}
			<label class="flex items-center gap-3 text-sm">
				<input type="checkbox" class="size-5 accent-pink" bind:checked={renameTracks} />
				{t('id.renameTracks')}
			</label>
		{/if}
		<div class="flex gap-2">
			<button class="btn btn-ghost btn-icon size-12" title={t('id.reset')} onclick={reset}><Undo2 class="size-5" /></button>
			<button class="btn btn-pink min-h-12 flex-1" disabled={!selected || applying} onclick={apply}>
				{#if applying}<LoaderCircle class="size-5 animate-spin" />{:else}<Check class="size-5" strokeWidth={3} />{/if}
				{t('id.apply')}
			</button>
		</div>
	</div>
</div>
