<script lang="ts">
	import { t } from '#lib/i18n.svelte.ts';
	import { Check, Link, LoaderCircle, Plus, Search, Zap } from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { api } from '#lib/api.ts';
	import { session } from '#lib/session.svelte.ts';
	import { toast, toastError } from '#lib/toasts.svelte.ts';
	import { hasRole, type Track } from '#lib/types.ts';
	import TrackRow from '#lib/components/TrackRow.svelte';

	interface Results {
		local: Track[];
		youtube: Track[];
	}

	$effect(() => {
		if (session.loaded && !hasRole(session.user, 'dj')) goto('/');
	});

	let q = $state('');
	let loading = $state(false);
	let results = $state<Results | null>(null);
	let busy = $state<Record<string, boolean>>({});
	let added = $state<Record<number, boolean>>({});
	const isAdmin = $derived(hasRole(session.user, 'admin'));
	const isLink = $derived(/^(https?:\/\/|spotify:)/i.test(q.trim()));

	let timer: ReturnType<typeof setTimeout>;
	let seq = 0;
	function onInput() {
		clearTimeout(timer);
		if (isLink || q.trim().length < 2) {
			results = null;
			return;
		}
		timer = setTimeout(run, 450);
	}

	async function run() {
		const my = ++seq;
		loading = true;
		try {
			const r = await api<Results>(`/search?q=${encodeURIComponent(q.trim())}`);
			if (my === seq) results = r;
		} catch (e) {
			toastError(e);
		} finally {
			if (my === seq) loading = false;
		}
	}

	async function add(body: { track_id?: number; url?: string; next?: boolean }, key: string) {
		busy[key] = true;
		try {
			const r = await api<{ added: number; tracks: Track[] }>('/queue', { body });
			r.tracks.forEach((t) => (added[t.id] = true));
			toast(r.added === 1 ? t('add.addedOne', { title: r.tracks[0].title }) : t('add.addedMany', { n: r.added }), 'success');
			if (body.url) q = '';
		} catch (e) {
			toastError(e);
		} finally {
			busy[key] = false;
		}
	}
</script>

{#snippet addButtons(tr: Track)}
	{#if added[tr.id]}
		<span class="grid size-11 place-items-center rounded-xl bg-mint/20 text-mint"><Check class="size-5" strokeWidth={3} /></span>
	{:else}
		{#if isAdmin}
			<button class="btn btn-sun btn-icon" title={t('queue.promote')} disabled={busy[tr.id]} onclick={() => add({ track_id: tr.id, next: true }, String(tr.id))}>
				<Zap class="size-5" strokeWidth={2.5} />
			</button>
		{/if}
		<button class="btn btn-pink btn-icon" title={t('add.addToQueue')} disabled={busy[tr.id]} onclick={() => add({ track_id: tr.id }, String(tr.id))}>
			{#if busy[tr.id]}<LoaderCircle class="size-5 animate-spin" strokeWidth={2.5} />{:else}<Plus class="size-6" strokeWidth={3} />{/if}
		</button>
	{/if}
{/snippet}

<h1 class="mb-3 text-3xl font-bold">{t('add.title')}</h1>

<form
	class="sticky top-[60px] z-20 -mx-4 mb-2 bg-ink/85 px-4 pb-2 backdrop-blur"
	onsubmit={(e) => {
		e.preventDefault();
		if (isLink) add({ url: q.trim() }, 'url');
		else run();
	}}
>
	<div class="relative">
		<span class="pointer-events-none absolute top-1/2 left-4 -translate-y-1/2 text-white/40">
			{#if isLink}<Link class="size-5" />{:else}<Search class="size-5" />{/if}
		</span>
		<input class="input pl-12" type="search" enterkeyhint="search" placeholder={t('add.placeholder')} bind:value={q} oninput={onInput} />
	</div>
	{#if isLink}
		<div class="mt-2 flex gap-2">
			<button class="btn btn-pink flex-1" disabled={busy.url}>
				{#if busy.url}<LoaderCircle class="size-5 animate-spin" />{:else}<Plus class="size-5" strokeWidth={3} />{/if} {t('add.addLink')}
			</button>
			{#if isAdmin}
				<button type="button" class="btn btn-sun btn-icon" title={t('add.withPriority')} disabled={busy.url} onclick={() => add({ url: q.trim(), next: true }, 'url')}>
					<Zap class="size-5" strokeWidth={2.5} />
				</button>
			{/if}
		</div>
	{/if}
</form>

{#if loading}
	<div class="flex justify-center py-10 text-pink"><LoaderCircle class="size-8 animate-spin" strokeWidth={2.5} /></div>
{:else if results}
	{#if results.local.length}
		<h2 class="mt-2 mb-1 text-sm font-bold tracking-wide text-white/50 uppercase">{t('add.fromLibrary')}</h2>
		<div class="flex flex-col divide-y divide-white/5">
			{#each results.local as t (t.id)}
				<TrackRow track={t}>{@render addButtons(t)}</TrackRow>
			{/each}
		</div>
	{/if}
	{#if results.youtube.length}
		<h2 class="mt-4 mb-1 text-sm font-bold tracking-wide text-white/50 uppercase">{t('add.youtube')}</h2>
		<div class="flex flex-col divide-y divide-white/5">
			{#each results.youtube as t (t.id)}
				<TrackRow track={t}>{@render addButtons(t)}</TrackRow>
			{/each}
		</div>
	{/if}
	{#if !results.local.length && !results.youtube.length}
		<p class="py-10 text-center text-white/50">{t('add.nothing')}</p>
	{/if}
{:else if !isLink}
	<div class="card mt-2 text-sm text-white/60">
		{t('add.help')}
	</div>
{/if}
