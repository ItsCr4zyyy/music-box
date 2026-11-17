<script lang="ts">
	import { Check, ImageUp, Languages, LoaderCircle, QrCode, Sparkles, Undo2 } from '@lucide/svelte';
	import { api } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { toast, toastError } from '#lib/toasts.svelte.ts';
	import { i18n, languages, local, t } from '#lib/i18n.svelte.ts';
	import { logoUrl, type Corner, type Settings, type Size, type ThemeInfo } from '#lib/types.ts';
	import Toggle from './Toggle.svelte';

	let s = $state<Settings | null>(null);
	let themes = $state<ThemeInfo[]>([]);
	let status = $state<'idle' | 'saving' | 'saved'>('idle');
	/** Co se umísťuje ťuknutím do rohu náhledu. */
	let placing = $state<'qr' | 'logo'>('qr');
	let uploading = $state(false);
	let fileInput = $state<HTMLInputElement>();

	api<Settings>('/admin/settings').then((r) => (s = r), toastError);
	api<{ themes: ThemeInfo[] }>('/themes').then((r) => (themes = r.themes), () => {});
	const logoRev = $derived(live.state?.display.logo_rev ?? s?.display.logo_rev ?? 0);
	const theme = $derived(themes.find((th) => th.id === s?.theme));

	// Automatické uložení – displej se překreslí hned.
	let timer: ReturnType<typeof setTimeout>;
	let first = true;
	$effect(() => {
		if (!s) return;
		JSON.stringify(s); // sledovat všechny změny
		if (first) {
			first = false;
			return;
		}
		clearTimeout(timer);
		status = 'saving';
		timer = setTimeout(save, 500);
	});

	async function save() {
		if (!s) return;
		try {
			await api('/admin/settings', { method: 'PUT', body: $state.snapshot(s) });
			status = 'saved';
		} catch (e) {
			toastError(e);
			status = 'idle';
		}
	}

	async function uploadLogo(file: File) {
		if (file.size > 5 * 1024 * 1024) {
			toast(t('display.logoTooBig'), 'error');
			return;
		}
		uploading = true;
		try {
			const res = await fetch('/api/admin/logo', { method: 'POST', headers: { 'Content-Type': file.type, 'X-Lang': i18n.lang }, body: file });
			if (!res.ok) throw new Error((await res.json().catch(() => null))?.error ?? t('common.error', { code: res.status }));
			toast(t('display.logoUploaded'), 'success');
		} catch (e) {
			toastError(e);
		} finally {
			uploading = false;
		}
	}

	async function resetLogo() {
		try {
			await api('/admin/logo', { method: 'DELETE' });
			toast(t('display.logoReset'), 'success');
		} catch (e) {
			toastError(e);
		}
	}

	const corners: Corner[] = ['top_left', 'top_right', 'bottom_left', 'bottom_right'];
	const cornerClass: Record<Corner, string> = {
		top_left: 'top-1.5 left-1.5',
		top_right: 'top-1.5 right-1.5',
		bottom_left: 'bottom-1.5 left-1.5',
		bottom_right: 'bottom-1.5 right-1.5'
	};
	const sizes = $derived<{ id: Size; label: string }[]>([
		{ id: 'small', label: t('common.small') },
		{ id: 'medium', label: t('common.medium') },
		{ id: 'large', label: t('common.large') }
	]);
	const qrPx: Record<Size, string> = { small: 'size-7', medium: 'size-9', large: 'size-12' };
	const logoPx: Record<Size, string> = { small: 'h-5', medium: 'h-6', large: 'h-8' };
</script>

{#if s}
	{@const d = s.display}
	<div class="mb-2 flex h-5 items-center justify-end gap-1 text-xs font-bold text-white/50">
		{#if status === 'saving'}<LoaderCircle class="size-3.5 animate-spin" /> {t('common.saving')}{:else if status === 'saved'}<Check class="size-3.5 text-mint" /> {t('display.savedLive')}{/if}
	</div>

	<!-- Náhled displeje -->
	<section class="card mb-4">
		<h2 class="section-title mb-2">{t('display.layout')}</h2>
		<div class="mb-2 grid grid-cols-2 gap-1 rounded-2xl bg-ink-3 p-1">
			<button class="rounded-xl py-2 text-sm font-bold transition {placing === 'qr' ? 'bg-pink text-on-accent' : 'text-white/70'}" onclick={() => (placing = 'qr')}>{t('display.placeQr')}</button>
			<button class="rounded-xl py-2 text-sm font-bold transition {placing === 'logo' ? 'bg-pink text-on-accent' : 'text-white/70'}" onclick={() => (placing = 'logo')}>{t('display.placeLogo')}</button>
		</div>
		<p class="mb-3 text-sm text-white/60">{t('display.tapCorner')}</p>
		<div class="relative aspect-video w-full overflow-hidden rounded-2xl bg-ink ring-2 ring-white/10">
			{#if d.queue_length > 0}
				<div class="absolute top-[22%] bottom-[22%] w-[34%] rounded-lg bg-ink-3 p-1.5 {d.queue_side === 'right' ? 'right-[3%]' : 'left-[3%]'}">
					<div class="mb-1 h-1.5 w-1/2 rounded bg-white/40"></div>
					{#each Array(Math.min(d.queue_length, 4)) as _, i (i)}
						<div class="mt-1 flex items-center gap-1"><div class="size-2.5 rounded-sm bg-lilac/60"></div><div class="h-1 flex-1 rounded bg-white/20"></div></div>
					{/each}
				</div>
			{/if}
			<div class="absolute top-1/2 flex -translate-y-1/2 items-center gap-2 {d.queue_length > 0 ? (d.queue_side === 'right' ? 'left-[8%]' : 'right-[8%]') : 'left-1/2 -translate-x-1/2'}">
				<div class="size-11 rounded-full bg-pink/70 ring-4 ring-black/40"></div>
				<div class="flex flex-col gap-1"><div class="h-2 w-14 rounded bg-white/70"></div><div class="h-1.5 w-9 rounded bg-white/30"></div></div>
			</div>
			{#each corners as c (c)}
				{@const hasQr = d.qr_enabled && d.qr_position === c}
				{@const hasLogo = d.show_logo && d.logo_position === c}
				<button
					class="absolute flex items-center gap-1 {cornerClass[c]} {c.endsWith('right') ? 'flex-row-reverse' : ''}"
					title={t('display.here')}
					onclick={() => {
						if (placing === 'qr') {
							d.qr_position = c;
							d.qr_enabled = true;
						} else {
							d.logo_position = c;
							d.show_logo = true;
						}
					}}
				>
					{#if hasLogo}
						<span class="flex items-center gap-1 rounded-md bg-white/10 p-0.5 pr-1.5 {logoPx[d.logo_size]}">
							<img src={logoUrl(logoRev)} alt="" class="h-full" />
							<span class="max-w-16 truncate text-[9px] font-bold">{s.box_name}</span>
						</span>
					{/if}
					{#if hasQr}
						<span class="grid place-items-center rounded-md bg-white text-ink {qrPx[d.qr_size]}"><QrCode class="size-4/5" strokeWidth={2.5} /></span>
					{/if}
					{#if !hasQr && !hasLogo}
						<span class="grid size-7 place-items-center rounded-md border-2 border-dashed border-white/25 text-white/30">+</span>
					{/if}
				</button>
			{/each}
		</div>
		<div class="mt-3 grid grid-cols-2 gap-2">
			<button class="btn {d.queue_side === 'left' ? 'btn-pink' : 'btn-ghost'}" onclick={() => (d.queue_side = 'left')}>{t('display.queueLeft')}</button>
			<button class="btn {d.queue_side === 'right' ? 'btn-pink' : 'btn-ghost'}" onclick={() => (d.queue_side = 'right')}>{t('display.queueRight')}</button>
		</div>
	</section>

	<!-- Speciální efekty aktivního motivu -->
	<section class="card mb-4 flex flex-col gap-4">
		<div>
			<h2 class="section-title"><Sparkles class="size-5 text-pink" strokeWidth={2.5} /> {t('display.effects')}</h2>
			{#if theme}<p class="text-sm text-white/50">{t('display.effectsOf', { name: theme.name })}</p>{/if}
		</div>
		{#if theme?.effects.length}
			{#each theme.effects as fx (fx.id)}
				<Toggle
					checked={d.effects[fx.id] ?? fx.default}
					onchange={(v) => (d.effects = { ...d.effects, [fx.id]: v })}
					label={`${local(fx.name, fx.name_en)} (${fx.animated ? t('display.animated') : t('display.static')})`}
				/>
			{/each}
		{:else}
			<p class="text-sm text-white/50">{t('display.effectsNone')}</p>
		{/if}
	</section>

	<section class="card mb-4 flex flex-col gap-4">
		<h2 class="section-title">{t('display.logo')}</h2>
		<Toggle bind:checked={d.show_logo} label={t('display.showLogo')} />
		<div class="flex items-center gap-3 rounded-2xl bg-ink-3 p-3">
			<img src={logoUrl(logoRev)} alt="Logo" class="size-16 shrink-0 rounded-xl object-contain" />
			<div class="flex min-w-0 flex-1 flex-col gap-2">
				<button class="btn btn-pink" disabled={uploading} onclick={() => fileInput?.click()}>
					{#if uploading}<LoaderCircle class="size-5 animate-spin" />{:else}<ImageUp class="size-5" />{/if} {t('display.uploadLogo')}
				</button>
				<button class="btn btn-ghost text-sm" onclick={resetLogo}><Undo2 class="size-4" /> {t('display.defaultLogo')}</button>
			</div>
		</div>
		<input
			bind:this={fileInput}
			type="file"
			accept="image/png,image/jpeg,image/webp,image/svg+xml"
			class="hidden"
			onchange={(e) => {
				const f = e.currentTarget.files?.[0];
				if (f) uploadLogo(f);
				e.currentTarget.value = '';
			}}
		/>
		<p class="-mt-2 text-xs text-white/40">{t('display.logoHint')}</p>
		<label>
			<span class="label">{t('display.boxName')}</span>
			<input class="input" bind:value={s.box_name} maxlength="40" />
		</label>
		<div>
			<span class="label">{t('display.logoSize')}</span>
			<div class="grid grid-cols-3 gap-2">
				{#each sizes as z (z.id)}
					<button class="btn {d.logo_size === z.id ? 'btn-pink' : 'btn-ghost'}" onclick={() => (d.logo_size = z.id)}>{z.label}</button>
				{/each}
			</div>
		</div>
	</section>

	<section class="card mb-4 flex flex-col gap-4">
		<h2 class="section-title"><QrCode class="size-5 text-pink" strokeWidth={2.5} /> {t('display.qr')}</h2>
		<Toggle bind:checked={d.qr_enabled} label={t('display.showQr')} />
		{#if d.qr_enabled}
			<div>
				<span class="label">{t('display.size')}</span>
				<div class="grid grid-cols-3 gap-2">
					{#each sizes as z (z.id)}
						<button class="btn {d.qr_size === z.id ? 'btn-pink' : 'btn-ghost'}" onclick={() => (d.qr_size = z.id)}>{z.label}</button>
					{/each}
				</div>
			</div>
			<label>
				<span class="label">{t('display.qrCaption')}</span>
				<input class="input" bind:value={d.qr_caption} maxlength="80" placeholder={t('display.qrCaptionNone')} />
			</label>
			<label>
				<span class="label">{t('display.qrUrl')}</span>
				<input class="input" bind:value={s.public_url} placeholder={t('display.qrUrlAuto')} autocapitalize="none" />
			</label>
		{/if}
	</section>

	<section class="card mb-4 flex flex-col gap-4">
		<h2 class="section-title">{t('display.content')}</h2>
		<div>
			<span class="label">{t('display.queueLength')}: <b class="text-white">{d.queue_length === 0 ? t('display.queueHidden') : d.queue_length}</b></span>
			<input type="range" min="0" max="15" step="1" bind:value={d.queue_length} class="w-full accent-pink" />
		</div>
		<Toggle bind:checked={d.show_added_by} label={t('display.showAddedBy')} />
		<Toggle bind:checked={s.fullscreen} label={t('display.fullscreen')} />
	</section>

	<section class="card mb-4 flex flex-col gap-3">
		<h2 class="section-title"><Languages class="size-5 text-pink" strokeWidth={2.5} /> {t('account.language')}</h2>
		<span class="label mb-0">{t('display.language')}</span>
		<div class="grid grid-cols-2 gap-2">
			{#each languages as l (l.id)}
				<button class="btn {s.language === l.id ? 'btn-pink' : 'btn-ghost'}" onclick={() => (s!.language = l.id)}>{l.name}</button>
			{/each}
		</div>
	</section>
{:else}
	<div class="flex justify-center py-10 text-pink"><LoaderCircle class="size-8 animate-spin" /></div>
{/if}
