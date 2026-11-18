<script lang="ts">
	import { Clock, Dices, LoaderCircle, Pause, Play, Radio, SkipForward, Volume2 } from '@lucide/svelte';
	import { api } from '#lib/api.ts';
	import { live, positionSec } from '#lib/live.svelte.ts';
	import { session } from '#lib/session.svelte.ts';
	import { toastError } from '#lib/toasts.svelte.ts';
	import { t } from '#lib/i18n.svelte.ts';
	import { fmtTime, hasRole, nameColor } from '#lib/types.ts';
	import Artwork from './Artwork.svelte';
	import Avatar from './Avatar.svelte';

	let now = $state(Date.now());
	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 500);
		return () => clearInterval(timer);
	});

	const np = $derived(live.state?.now_playing ?? null);
	const radio = $derived(np ? null : (live.state?.radio ?? null));
	const isAdmin = $derived(hasRole(session.user, 'admin'));
	const canSkip = $derived(
		isAdmin || (!!np && hasRole(session.user, 'dj') && np.entry.added_by?.id === session.user?.id)
	);
	const pos = $derived(np ? positionSec(now) : 0);
	const dur = $derived(np?.entry.track.duration_sec ?? 0);
	const accent = $derived(
		np ? nameColor(np.entry.track.artist + np.entry.track.title) : radio ? nameColor(radio.station) : '#ff5fa2'
	);
	const paused = $derived(np?.paused ?? radio?.paused ?? false);

	let volume = $state(0.8);
	let dragging = false;
	$effect(() => {
		if (!dragging && live.state) volume = live.state.volume;
	});

	async function call(path: string, body?: unknown) {
		try {
			await api(path, { method: 'POST', body });
		} catch (e) {
			toastError(e);
		}
	}
</script>

{#snippet controls(skip: boolean)}
	<div class="relative mt-3 flex items-center gap-2 border-t border-white/5 pt-3">
		{#if isAdmin}
			{#if paused}
				<button class="btn btn-mint btn-icon" title={t('np.play')} onclick={() => call('/player/resume')}><Play class="size-5" strokeWidth={2.5} /></button>
			{:else}
				<button class="btn btn-ghost btn-icon" title={t('np.pause')} onclick={() => call('/player/pause')}><Pause class="size-5" strokeWidth={2.5} /></button>
			{/if}
		{/if}
		{#if skip}
			<button class="btn btn-sun btn-icon" title={t('np.skip')} onclick={() => call('/player/skip')}><SkipForward class="size-5" strokeWidth={2.5} /></button>
		{/if}
		{#if isAdmin}
			<label class="flex min-w-0 flex-1 items-center gap-2 pl-1 text-sm font-bold text-white/60">
				<Volume2 class="size-5 shrink-0" strokeWidth={2.5} />
				<input
					type="range"
					min="0"
					max="1"
					step="0.01"
					bind:value={volume}
					class="min-w-0 flex-1 accent-pink"
					onpointerdown={() => (dragging = true)}
					onchange={() => {
						dragging = false;
						call('/player/volume', { volume });
					}}
				/>
				<span class="w-9 text-right">{Math.round(volume * 100)}%</span>
			</label>
		{/if}
	</div>
	{#if isAdmin && live.state?.volume_zone}
		<p class="relative mt-2 flex items-center gap-1.5 text-xs font-bold text-sun">
			<Clock class="size-3.5 shrink-0" />
			{t('np.zone', { name: live.state.volume_zone[0], n: Math.round(live.state.volume_zone[1] * 100) })}
		</p>
	{/if}
{/snippet}

<section class="card mb-accent-scope relative overflow-hidden" style="--mb-accent: {accent}">
	<div class="mb-glow pointer-events-none absolute -top-16 -right-16 size-48 rounded-full opacity-30 blur-3xl transition-colors duration-1000" style="background: var(--mb-accent)"></div>
	{#if np}
		<div class="relative flex items-center gap-4">
			<div class="relative shrink-0">
				<Artwork
					url={np.entry.track.artwork_url}
					seed={np.entry.track.artist + np.entry.track.title}
					class="mb-disc size-28 rounded-full! ring-4 ring-ink {np.paused || np.loading ? '' : 'animate-[spin_10s_linear_infinite]'}"
				/>
				<div class="mb-disc-hole absolute inset-0 m-auto size-5 rounded-full bg-ink ring-4 ring-ink-2"></div>
			</div>
			<div class="min-w-0 flex-1">
				<div class="mb-label mb-0.5 flex items-center gap-1.5 text-sm font-bold {!np.loading && !np.paused ? 'mb-live' : ''}" style="color: var(--mb-accent)">
					{#if np.loading}
						<LoaderCircle class="size-4 animate-spin" strokeWidth={2.5} /> {t('np.preparing')}
					{:else if np.paused}
						<Pause class="size-4" strokeWidth={2.5} /> {t('np.paused')}
					{:else}
						<Radio class="size-4" strokeWidth={2.5} /> {t('np.playing')}
					{/if}
				</div>
				<h2 class="line-clamp-2 text-2xl font-bold">{np.entry.track.title}</h2>
				<div class="truncate text-white/70">{np.entry.track.artist}</div>
			</div>
		</div>

		<div class="relative mt-3 flex items-center gap-2 text-sm text-white/60">
			{#if np.entry.added_by}
				<Avatar name={np.entry.added_by.name} url={np.entry.added_by.avatar_url} />
				<span>{t('np.wishFrom')} <b class="text-white">{np.entry.added_by.name}</b></span>
			{:else}
				<span class="grid size-6 place-items-center rounded-full bg-mint text-on-accent"><Dices class="size-3.5" strokeWidth={2.5} /></span>
				{t('np.pickedByBox')}
			{/if}
		</div>

		<div class="relative mt-3">
			<div class="h-2.5 overflow-hidden rounded-full bg-ink-3">
				<div class="mb-progress h-full rounded-full transition-[width] duration-500" style="width: {dur ? (pos / dur) * 100 : 0}%; background: linear-gradient(90deg, var(--mb-accent), var(--color-sun))"></div>
			</div>
			<div class="mt-1 flex justify-between text-xs font-bold text-white/50">
				<span>{fmtTime(pos)}</span><span>{fmtTime(dur)}</span>
			</div>
		</div>

		{#if isAdmin || canSkip}{@render controls(true)}{/if}
	{:else if radio}
		<!-- Rádio (fronta je prázdná) -->
		<div class="relative flex items-center gap-4">
			<div class="relative shrink-0">
				<Artwork url={radio.cover_url ?? radio.station_logo} seed={radio.station} class="mb-disc size-28 rounded-2xl object-cover" />
				{#if radio.station_logo && radio.cover_url}
					<img src={radio.station_logo} alt="" class="absolute -right-2 -bottom-2 size-9 rounded-full bg-white object-contain p-0.5 ring-4 ring-ink-2" />
				{/if}
			</div>
			<div class="min-w-0 flex-1">
				<div class="mb-label mb-0.5 flex items-center gap-1.5 text-sm font-bold {!radio.loading && !radio.paused ? 'mb-live' : ''}" style="color: var(--mb-accent)">
					{#if radio.loading}
						<LoaderCircle class="size-4 animate-spin" strokeWidth={2.5} /> {t('np.radioConnecting')}
					{:else if radio.paused}
						<Pause class="size-4" strokeWidth={2.5} /> {t('np.paused')}
					{:else}
						<Radio class="size-4" strokeWidth={2.5} /> {t('np.radioOnAir')}
					{/if}
				</div>
				<h2 class="line-clamp-2 text-2xl font-bold">{radio.title ?? radio.station}</h2>
				{#if radio.artist}<div class="truncate text-white/70">{radio.artist}</div>{/if}
				<div class="mt-1 flex items-center gap-1.5 text-sm text-white/50"><Radio class="size-3.5" /> {radio.station}</div>
			</div>
		</div>
		<p class="relative mt-3 text-xs text-white/50">{t('np.radioHint')}</p>
		{#if isAdmin}{@render controls(false)}{/if}
	{:else}
		<div class="relative flex flex-col items-center gap-1 py-6 text-center">
			<div class="mb-1 grid size-16 animate-wiggle place-items-center rounded-full bg-ink-3 text-pink">
				<Radio class="size-8" strokeWidth={2.5} />
			</div>
			<h2 class="text-2xl font-bold">{t('np.silence')}</h2>
			<p class="text-white/60">{t('np.nothingPlaying')}</p>
		</div>
	{/if}
</section>
