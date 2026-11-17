<script lang="ts">
	import { t } from '#lib/i18n.svelte.ts';
	import { ArrowLeft, Check, Disc3, FolderOpen, ListPlus, LoaderCircle, Plus, WandSparkles, Zap } from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { api } from '#lib/api.ts';
	import { session } from '#lib/session.svelte.ts';
	import { toast, toastError } from '#lib/toasts.svelte.ts';
	import { fmtTime, hasRole, type Album, type Library, type Track } from '#lib/types.ts';
	import Artwork from '#lib/components/Artwork.svelte';
	import TrackRow from '#lib/components/TrackRow.svelte';
	import IdentifySheet from '#lib/components/IdentifySheet.svelte';

	$effect(() => {
		if (session.loaded && !hasRole(session.user, 'dj')) goto('/');
	});

	let lib = $state<Library | null>(null);
	let album = $state<Album | null>(null);
	let filter = $state('');
	let busy = $state<Record<string, boolean>>({});
	let added = $state<Record<number, boolean>>({});
	const isAdmin = $derived(hasRole(session.user, 'admin'));

	let identify = $state<{ kind: 'album' | 'track'; target: string; query: string } | null>(null);

	async function load() {
		try {
			lib = await api<Library>('/library');
			// otevřené album obnovit novými údaji
			if (album) album = lib.albums.find((a) => a.key === album!.key) ?? null;
		} catch (e) {
			toastError(e);
		}
	}
	load();

	const unknown = (artist: string) => ['Neznámý interpret', 'Různí interpreti', ''].includes(artist);
	function identifyAlbum(a: Album) {
		const folder = a.key.split('/').pop() ?? a.name;
		identify = { kind: 'album', target: a.key, query: unknown(a.artist) ? folder : `${a.artist} ${a.name}` };
	}
	function identifyTrack(t: Track) {
		identify = { kind: 'track', target: String(t.id), query: unknown(t.artist) ? t.title : `${t.artist} ${t.title}` };
	}

	const f = $derived(filter.trim().toLowerCase());
	const albums = $derived(
		(lib?.albums ?? []).filter((a) => !f || `${a.name} ${a.artist}`.toLowerCase().includes(f))
	);
	const singles = $derived(
		(lib?.singles ?? []).filter((t) => !f || `${t.title} ${t.artist}`.toLowerCase().includes(f))
	);

	async function add(body: { track_id?: number; album?: string; next?: boolean }, key: string) {
		busy[key] = true;
		try {
			const r = await api<{ added: number; tracks: Track[] }>('/queue', { body });
			r.tracks.forEach((t) => (added[t.id] = true));
			toast(r.added === 1 ? t('add.addedOne', { title: r.tracks[0].title }) : t('add.addedMany', { n: r.added }), 'success');
		} catch (e) {
			toastError(e);
		} finally {
			busy[key] = false;
		}
	}

	const albumLength = (a: Album) => a.tracks.reduce((s, t) => s + t.duration_sec, 0);
</script>

{#snippet addButtons(tr: Track)}
	{#if isAdmin}
		<button class="grid size-9 place-items-center rounded-xl text-white/40 active:bg-ink-3" title={t('lib.identify')} onclick={() => identifyTrack(tr)}>
			<WandSparkles class="size-4.5" />
		</button>
	{/if}
	{#if added[tr.id]}
		<span class="grid size-11 place-items-center rounded-xl bg-mint/20 text-mint"><Check class="size-5" strokeWidth={3} /></span>
	{:else}
		{#if isAdmin}
			<button class="btn btn-sun btn-icon" title={t('queue.promote')} disabled={busy[tr.id]} onclick={() => add({ track_id: tr.id, next: true }, String(tr.id))}>
				<Zap class="size-5" strokeWidth={2.5} />
			</button>
		{/if}
		<button class="btn btn-pink btn-icon" title={t('common.add')} disabled={busy[tr.id]} onclick={() => add({ track_id: tr.id }, String(tr.id))}>
			<Plus class="size-6" strokeWidth={3} />
		</button>
	{/if}
{/snippet}

{#if album}
	<!-- Detail alba -->
	<button class="mb-3 flex items-center gap-1 font-bold text-white/60" onclick={() => (album = null)}>
		<ArrowLeft class="size-5" /> {t('lib.back')}
	</button>
	<div class="card mb-4 flex flex-col items-center text-center">
		<Artwork url={album.artwork_url} seed={album.name} class="size-44 rounded-2xl" />
		<h1 class="mt-3 text-2xl font-bold">{album.name}</h1>
		<div class="text-white/60">{album.artist}</div>
		<div class="text-sm text-white/40">{t('lib.tracks', { n: album.tracks.length })} · {fmtTime(albumLength(album))}</div>
		<button class="btn btn-pink mt-3 w-full" disabled={busy[album.key]} onclick={() => add({ album: album!.key }, album!.key)}>
			{#if busy[album.key]}<LoaderCircle class="size-5 animate-spin" />{:else}<ListPlus class="size-5" strokeWidth={2.5} />{/if}
			{t('lib.addAlbum')}
		</button>
		{#if isAdmin}
			<button class="btn btn-ghost mt-2 w-full" onclick={() => identifyAlbum(album!)}>
				<WandSparkles class="size-5" /> {t('lib.identifyAlbum')}
			</button>
		{/if}
	</div>
	<div class="flex flex-col divide-y divide-white/5">
		{#each album.tracks as t, i (t.id)}
			<TrackRow track={t} number={i + 1}>{@render addButtons(t)}</TrackRow>
		{/each}
	</div>
{:else}
	<h1 class="mb-3 text-3xl font-bold">{t('lib.title')}</h1>
	{#if !lib}
		<div class="flex justify-center py-10 text-pink"><LoaderCircle class="size-8 animate-spin" /></div>
	{:else if !lib.albums.length && !lib.singles.length}
		<div class="card flex flex-col items-center gap-2 py-8 text-center text-white/60">
			<FolderOpen class="size-10 text-white/30" />
			{t('lib.empty')}
		</div>
	{:else}
		<input class="input mb-4" type="search" placeholder={t('lib.filter')} bind:value={filter} />
		{#if albums.length}
			<h2 class="section-title mb-2"><Disc3 class="size-5 text-pink" strokeWidth={2.5} /> {t('lib.albums')}</h2>
			<div class="mb-5 grid grid-cols-2 gap-3">
				{#each albums as a (a.key)}
					<button class="card p-2.5 text-left transition active:scale-95" onclick={() => (album = a)}>
						<Artwork url={a.artwork_url} seed={a.name} class="aspect-square w-full rounded-2xl" />
						<div class="mt-2 truncate font-bold">{a.name}</div>
						<div class="truncate text-xs text-white/50">{a.artist} · {a.tracks.length}</div>
					</button>
				{/each}
			</div>
		{/if}
		{#if singles.length}
			<h2 class="section-title mb-1">{t('lib.singles')}</h2>
			<div class="flex flex-col divide-y divide-white/5">
				{#each singles as t (t.id)}
					<TrackRow track={t}>{@render addButtons(t)}</TrackRow>
				{/each}
			</div>
		{/if}
	{/if}
{/if}

{#if identify}
	<IdentifySheet
		{...identify}
		onclose={() => (identify = null)}
		ondone={() => {
			identify = null;
			load();
		}}
	/>
{/if}
