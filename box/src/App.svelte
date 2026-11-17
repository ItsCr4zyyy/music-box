<script lang="ts">
	import { Dices, ListMusic, LoaderCircle, Pause, Radio, WifiOff, Zap } from '@lucide/svelte';
	import { live, positionSec } from './lib/live.svelte';
	import { fmtTime, logoUrl, nameColor, type Corner, type DisplaySettings, type Track } from './lib/types';
	import Cover from './lib/Cover.svelte';
	import Notes from './lib/Notes.svelte';
	import Avatar from './lib/Avatar.svelte';
	import Keys from './lib/Keys.svelte';
	import Ticker from './lib/Ticker.svelte';
	import { translate, type Key } from './lib/i18n';

	live.start();

	let now = $state(Date.now());
	$effect(() => {
		const t = setInterval(() => (now = Date.now()), 250);
		return () => clearInterval(t);
	});

	// Kurzor schovat, když se myš nehýbe.
	let idle = $state(true);
	let idleTimer: ReturnType<typeof setTimeout>;
	function onMove() {
		idle = false;
		clearTimeout(idleTimer);
		idleTimer = setTimeout(() => (idle = true), 2500);
	}

	// F11 = přepnout celou obrazovku (jen dočasně, trvalé nastavení je v administraci).
	async function onKey(e: KeyboardEvent) {
		if (e.key === 'F11' && '__TAURI_INTERNALS__' in window) {
			e.preventDefault();
			const { invoke } = await import('@tauri-apps/api/core');
			invoke('toggle_fullscreen');
		}
	}

	const defaults: DisplaySettings = {
		qr_enabled: true,
		qr_position: 'bottom_right',
		qr_size: 'medium',
		qr_caption: 'Přidej svoji písničku!',
		queue_side: 'right',
		queue_length: 6,
		show_added_by: true,
		effects: {},
		show_logo: true,
		logo_position: 'top_left',
		logo_size: 'medium',
		logo_rev: 0
	};
	const d = $derived(live.state?.display ?? defaults);
	const np = $derived(live.state?.now_playing ?? null);
	const radio = $derived(np ? null : (live.state?.radio ?? null));
	const tt = (key: Key, params?: Record<string, string | number>) => translate(live.state?.language, key, params);
	/** Zapnuté speciální efekty aktivního motivu. */
	const fx = $derived(new Set(live.state?.effects ?? ['notes', 'wiggle-logo']));
	const playingNow = $derived((np && !np.paused && !np.loading) || (radio && !radio.paused && !radio.loading));
	const tickerItems = $derived.by(() => {
		const items: string[] = [];
		if (np) items.push(`${tt('playing')}: ${np.entry.track.title} – ${np.entry.track.artist}`);
		else if (radio) items.push(`${radio.station}: ${radio.stream_title ?? ''}`);
		for (const e of queue.slice(0, 5)) items.push(`${tt('next')}: ${e.track.title} – ${e.track.artist}`);
		if (d.qr_enabled && joinUrl) items.push(joinUrl.replace(/^https?:\/\//, '').replace(/\/$/, ''));
		return items;
	});
	const queue = $derived(live.state?.queue ?? []);
	const upNext = $derived(queue.slice(0, d.queue_length));
	const pos = $derived(np ? positionSec(now) : 0);
	const dur = $derived(np?.entry.track.duration_sec ?? 0);
	const accent = $derived(
		np ? nameColor(np.entry.track.artist + np.entry.track.title) : radio ? nameColor(radio.station) : '#ff5fa2'
	);
	const queueTotal = $derived(queue.reduce((s, e) => s + e.track.duration_sec, 0));
	const joinUrl = $derived(live.state?.join_url ?? '');
	const qrUrl = $derived(live.base ? `${live.base}/api/qr.svg?u=${encodeURIComponent(joinUrl)}` : '');
	const logoSrc = $derived(live.base ? logoUrl(d.logo_rev, live.base) : '/favicon.svg');

	// ---------- Rozložení: fronta + logo a QR kód v rozích ----------
	type Item = 'logo' | 'qr';
	type Column = 'main' | 'aside';
	const showQueue = $derived(d.queue_length > 0);
	const items = $derived(
		[
			d.show_logo ? { id: 'logo' as Item, corner: d.logo_position } : null,
			d.qr_enabled && qrUrl ? { id: 'qr' as Item, corner: d.qr_position } : null
		].filter((x) => x !== null)
	);
	const hSide = (c: Corner) => (c.endsWith('left') ? 'left' : 'right');
	/** Roh na straně fronty patří do sloupce s frontou, jinak do hlavního sloupce. */
	const columnOf = (c: Corner): Column => (showQueue && hSide(c) === d.queue_side ? 'aside' : 'main');
	function slot(col: Column, top: boolean, side?: 'left' | 'right') {
		return items.filter(
			(i) => columnOf(i.corner) === col && i.corner.startsWith('top') === top && (!side || hSide(i.corner) === side)
		);
	}
	const qrSize = $derived({ small: '6em', medium: '8.5em', large: '12em' }[d.qr_size]);
	const logoSize = $derived({ small: 0.75, medium: 1, large: 1.5 }[d.logo_size]);

	const art = (t: Track) => live.url(t.artwork_url);
	const themeHref = $derived(live.base ? live.base + (live.state?.theme_css ?? '/api/theme.css') : '');
</script>

<svelte:head>
	{#if themeHref}<link rel="stylesheet" href={themeHref} />{/if}
</svelte:head>

<svelte:window onmousemove={onMove} onkeydown={onKey} />

{#snippet qrCard()}
	{@const left = hSide(d.qr_position) === 'left'}
	<div
		class="mb-panel flex shrink-0 items-center justify-between gap-[1em] rounded-[calc(1.4em*var(--mb-radius))] bg-white p-[0.8em] text-ink {left ? 'flex-row-reverse text-right' : 'flex-row'}"
		style="box-shadow: var(--mb-shadow-card); max-width: 100%"
	>
		{#if d.qr_caption}
			<div class="min-w-0">
				<div class="font-display text-[1.6em] leading-[1.1] font-bold">{d.qr_caption}</div>
				<div class="mt-[0.2em] truncate text-[1em] font-bold text-ink/60">{joinUrl.replace(/^https?:\/\//, '').replace(/\/$/, '')}</div>
			</div>
		{/if}
		<img src={qrUrl} alt="QR" class="shrink-0 rounded-[calc(0.4em*var(--mb-radius))]" style="width: {qrSize}; height: {qrSize}" />
	</div>
{/snippet}

{#snippet logo()}
	{@const right = hSide(d.logo_position) === 'right'}
	<div class="flex shrink-0 items-center gap-[0.6em] {right ? 'flex-row-reverse' : ''}" style="font-size: {logoSize}em">
		<img src={logoSrc} alt="" class="mb-logo-img mb-fx size-[2.6em] object-contain" />
		<span class="mb-brand font-display text-[2.2em] leading-none font-bold tracking-tight">{live.state?.box_name ?? 'music-box'}</span>
	</div>
{/snippet}

{#snippet item(id: Item)}
	{#if id === 'logo'}{@render logo()}{:else}{@render qrCard()}{/if}
{/snippet}

<!-- Řádek s položkami v rozích hlavního sloupce (vlevo … vpravo). -->
{#snippet cornerRow(top: boolean)}
	{@const l = slot('main', top, 'left')}
	{@const r = slot('main', top, 'right')}
	{#if l.length || r.length}
		<div class="flex gap-[2em] {top ? 'items-start' : 'items-end'}">
			<div class="flex min-w-0 flex-col items-start gap-[1.5vmin]">{#each l as i (i.id)}{@render item(i.id)}{/each}</div>
			<div class="flex-1"></div>
			<div class="flex min-w-0 flex-col items-end gap-[1.5vmin]">{#each r as i (i.id)}{@render item(i.id)}{/each}</div>
		</div>
	{/if}
{/snippet}

{#snippet nowPlaying()}
	<section class="relative flex min-w-0 flex-[1.5] flex-col gap-[2vmin]">
		{@render cornerRow(true)}

		{#if np}
			<div class="flex min-h-0 flex-1 items-center gap-[4vmin]">
				<Cover src={art(np.entry.track)} seed={np.entry.track.artist + np.entry.track.title} spinning={!np.paused && !np.loading} />
				<div class="min-w-0 flex-1">
					<div class="mb-label mb-[0.5em] flex items-center gap-[0.5em] font-display text-[1.3em] font-bold {!np.loading && !np.paused ? 'mb-live' : ''}" style="color: var(--mb-accent)">
						{#if np.loading}
							<LoaderCircle class="size-[1.1em] animate-spin" strokeWidth={2.5} /> {tt('preparing')}
						{:else if np.paused}
							<Pause class="size-[1.1em]" strokeWidth={2.5} /> {tt('paused')}
						{:else}
							<span class="mb-eq flex h-[1em] items-end gap-[0.15em]">
								{#each [0, 1, 2, 3] as i (i)}
									<span class="h-full w-[0.22em] origin-bottom rounded-full" style="background: var(--mb-accent); animation: eq {0.7 + i * 0.15}s ease-in-out {i * 0.1}s infinite"></span>
								{/each}
							</span>
							{tt('playing')}
						{/if}
					</div>
					<h1 class="line-clamp-2 font-display text-[4.2em] leading-[1.05] font-bold">{np.entry.track.title}</h1>
					<div class="mt-[0.3em] truncate text-[2em] text-white/70">{np.entry.track.artist}</div>

					{#if d.show_added_by}
						<div class="mt-[1.2em] inline-flex items-center gap-[0.6em] rounded-full bg-white/10 py-[0.4em] pr-[1.2em] pl-[0.4em] text-[1.4em]">
							{#if np.entry.added_by}
								<Avatar name={np.entry.added_by.name} url={live.url(np.entry.added_by.avatar_url)} class="size-[1.8em]" />
								<span><span class="text-white/60">{tt('wishFrom')}</span> <b>{np.entry.added_by.name}</b></span>
							{:else}
								<span class="grid size-[1.8em] place-items-center rounded-full bg-mint text-on-accent"><Dices class="size-[1em]" strokeWidth={2.5} /></span>
								<span class="text-white/70">{tt('pickedByBox')}</span>
							{/if}
						</div>
					{/if}

					<div class="mt-[1.5em] max-w-[40em]">
						<div class="h-[0.7em] overflow-hidden rounded-full bg-white/10">
							<div class="mb-progress h-full rounded-full transition-[width] duration-300 ease-linear" style="width: {dur ? (pos / dur) * 100 : 0}%; background: linear-gradient(90deg, var(--mb-accent), var(--color-sun))"></div>
						</div>
						<div class="mt-[0.4em] flex justify-between font-display text-[1.2em] text-white/50">
							<span>{fmtTime(pos)}</span><span>{fmtTime(dur)}</span>
						</div>
					</div>
				</div>
			</div>
		{:else if radio}
			<!-- Rádio (fronta je prázdná) -->
			<div class="flex min-h-0 flex-1 items-center gap-[4vmin]">
				<Cover src={live.url(radio.cover_url ?? radio.station_logo)} seed={radio.station} spinning={!radio.paused && !radio.loading} />
				<div class="min-w-0 flex-1">
					<div class="mb-label mb-[0.5em] flex items-center gap-[0.5em] font-display text-[1.3em] font-bold {!radio.loading && !radio.paused ? 'mb-live' : ''}" style="color: var(--mb-accent)">
						{#if radio.loading}
							<LoaderCircle class="size-[1.1em] animate-spin" strokeWidth={2.5} /> {tt('radioConnecting')}
						{:else if radio.paused}
							<Pause class="size-[1.1em]" strokeWidth={2.5} /> {tt('paused')}
						{:else}
							<Radio class="size-[1.1em]" strokeWidth={2.5} /> {tt('radioOnAir')}
						{/if}
					</div>
					<h1 class="line-clamp-2 font-display text-[4.2em] leading-[1.05] font-bold">{radio.title ?? radio.station}</h1>
					{#if radio.artist}<div class="mt-[0.3em] truncate text-[2em] text-white/70">{radio.artist}</div>{/if}
					<div class="mt-[1.2em] inline-flex items-center gap-[0.6em] rounded-full bg-white/10 py-[0.4em] pr-[1.2em] pl-[0.4em] text-[1.4em]">
						{#if radio.station_logo}
							<img src={radio.station_logo} alt="" class="size-[1.8em] rounded-full bg-white object-contain p-[0.1em]" />
						{:else}
							<span class="grid size-[1.8em] place-items-center rounded-full bg-mint text-on-accent"><Radio class="size-[1em]" strokeWidth={2.5} /></span>
						{/if}
						<b>{radio.station}</b>
					</div>
					{#if d.qr_enabled}<p class="mt-[1.2em] text-[1.3em] text-white/50">{tt('radioHint')}</p>{/if}
				</div>
			</div>
		{:else}
			<div class="flex flex-1 flex-col items-center justify-center text-center">
				<div class="mb-fx grid size-[8em] place-items-center rounded-full bg-ink-3 text-pink" style="animation: wiggle 2s ease-in-out infinite">
					<Radio class="size-[4em]" strokeWidth={2.2} />
				</div>
				<h1 class="mt-[0.4em] font-display text-[4em] font-bold">{tt('silence')}</h1>
				{#if d.qr_enabled}<p class="mt-[0.2em] text-[1.8em] text-white/60">{tt('scanQr')}</p>{/if}
			</div>
		{/if}

		{@render cornerRow(false)}
	</section>
{/snippet}

{#snippet aside()}
	{@const alignEnd = d.queue_side === 'right'}
	<aside class="relative flex w-[34%] max-w-[36em] min-w-[18em] flex-col gap-[3vmin] {alignEnd ? 'items-end' : 'items-start'}">
		{#each slot('aside', true) as i (i.id)}{@render item(i.id)}{/each}
		<div class="mb-panel flex min-h-0 w-full flex-1 flex-col rounded-[calc(1.6em*var(--mb-radius))] bg-ink-2/80 p-[1.4em] ring-1 ring-white/5 backdrop-blur" style="box-shadow: var(--mb-shadow-card)">
			<div class="mb-[0.8em] flex items-baseline justify-between">
				<h2 class="font-display text-[1.8em] font-bold">{tt('upNext')}</h2>
				{#if queue.length}<span class="mb-label text-[1.1em] text-white/50">{queue.length} · {fmtTime(queueTotal)}</span>{/if}
			</div>
			{#if upNext.length}
				<ol class="flex min-h-0 flex-col gap-[0.6em] overflow-hidden">
					{#each upNext as e, i (e.id)}
						<li class="flex items-center gap-[0.8em]" style="animation: slide-in 0.4s ease-out {i * 0.05}s both">
							<span class="w-[1.2em] text-center font-display text-[1.3em] font-bold text-white/30">{i + 1}</span>
							{#if art(e.track)}
								<img src={art(e.track)} alt="" referrerpolicy="no-referrer" class="size-[3.2em] shrink-0 rounded-[calc(0.7em*var(--mb-radius))] object-cover" />
							{:else}
								<div class="size-[3.2em] shrink-0 rounded-[calc(0.7em*var(--mb-radius))]" style="background: {nameColor(e.track.artist + e.track.title)}"></div>
							{/if}
							<div class="min-w-0 flex-1">
								<div class="flex items-center gap-[0.3em] text-[1.25em] font-bold">
									{#if e.priority}<Zap class="size-[0.9em] shrink-0 text-sun" strokeWidth={3} />{/if}
									<span class="truncate">{e.track.title}</span>
								</div>
								<div class="truncate text-[1em] text-white/60">{e.track.artist}</div>
							</div>
							{#if d.show_added_by}
								{#if e.added_by}
									<span class="flex max-w-[8.5em] shrink-0 items-center gap-[0.35em] rounded-full py-[0.15em] pr-[0.7em] pl-[0.15em] text-[0.95em] font-bold text-[#1b1036]" style="background: {nameColor(e.added_by.name)}">
										<Avatar name={e.added_by.name} url={live.url(e.added_by.avatar_url)} class="size-[1.4em] text-[0.8em]" />
										<span class="truncate">{e.added_by.name}</span>
									</span>
								{:else}
									<Dices class="size-[1.3em] shrink-0 text-mint" strokeWidth={2.5} />
								{/if}
							{/if}
						</li>
					{/each}
				</ol>
				{#if queue.length > upNext.length}
					<div class="mt-auto pt-[0.6em] text-center text-[1.1em] text-white/40">{tt('andMore', { n: queue.length - upNext.length })}</div>
				{/if}
			{:else}
				<div class="flex flex-1 flex-col items-center justify-center gap-[0.4em] text-center text-white/50">
					<ListMusic class="size-[3em]" strokeWidth={2} />
					<div class="text-[1.3em]">{tt('queueEmpty')}</div>
				</div>
			{/if}
		</div>
		{#each slot('aside', false) as i (i.id)}{@render item(i.id)}{/each}
	</aside>
{/snippet}

<main
	class="mb-accent-scope relative flex h-full gap-[3vmin] p-[4vmin] {idle ? 'hide-cursor' : ''} {fx.has('ticker') ? 'pb-[calc(4vmin+2.6em)]' : ''}"
	style="--mb-accent: {accent}"
>
	<!-- Barevná záře podle skladby -->
	<div class="mb-glow pointer-events-none absolute -top-1/3 -left-1/4 size-[90vmin] rounded-full opacity-25 blur-[12vmin] transition-colors duration-[2s]" style="background: var(--mb-accent)"></div>
	<div class="mb-glow pointer-events-none absolute -right-1/4 -bottom-1/3 size-[70vmin] rounded-full bg-lilac opacity-15 blur-[12vmin]"></div>
	<!-- Speciální efekty motivu -->
	{#if fx.has('notes') && playingNow}<Notes />{/if}
	{#if fx.has('keys')}<Keys />{/if}
	{#if fx.has('ticker')}<Ticker items={tickerItems} />{/if}

	{#if showQueue && d.queue_side === 'left'}{@render aside()}{/if}
	{@render nowPlaying()}
	{#if showQueue && d.queue_side === 'right'}{@render aside()}{/if}

	{#if !live.connected}
		<div class="absolute top-[2vmin] left-1/2 flex -translate-x-1/2 items-center gap-[0.5em] rounded-full bg-pink px-[1em] py-[0.3em] font-bold text-on-accent">
			<WifiOff class="size-[1.1em]" strokeWidth={2.5} /> {tt('connecting')}
		</div>
	{/if}

	<!-- Upozornění (např. skladba nejde přehrát) -->
	<div class="absolute bottom-[3vmin] left-1/2 z-30 flex -translate-x-1/2 flex-col items-center gap-[0.5em]">
		{#each live.toasts as t (t.id)}
			<div class="rounded-[1em] px-[1.2em] py-[0.6em] text-[1.2em] font-bold text-on-accent {t.level === 'error' ? 'bg-pink' : 'bg-sky'}">{t.message}</div>
		{/each}
	</div>
</main>
