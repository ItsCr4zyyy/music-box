<script lang="ts">
	import { i18n, local, t } from '#lib/i18n.svelte.ts';
	import { Check, Download, FileUp, LoaderCircle, Palette, Trash2 } from '@lucide/svelte';
	import { api } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { toast, toastError } from '#lib/toasts.svelte.ts';
	import type { Settings, ThemeInfo } from '#lib/types.ts';

	let themes = $state<ThemeInfo[]>([]);
	let active = $state('');
	let busy = $state(false);
	let fileInput = $state<HTMLInputElement>();
	let showHelp = $state(false);

	async function load() {
		try {
			const r = await api<{ active: string; themes: ThemeInfo[] }>('/themes');
			themes = r.themes;
			active = r.active;
		} catch (e) {
			toastError(e);
		}
	}
	load();
	$effect(() =>
		live.on((type) => {
			if (type === 'settings') load();
		})
	);

	async function activate(id: string) {
		try {
			const s = await api<Settings>('/admin/settings');
			s.theme = id;
			await api('/admin/settings', { method: 'PUT', body: s });
			active = id;
			toast(t('themes.applied'), 'success');
		} catch (e) {
			toastError(e);
		}
	}

	async function upload(file: File) {
		busy = true;
		try {
			const res = await fetch('/api/admin/themes', { method: 'POST', body: file });
			const data = await res.json().catch(() => null);
			if (!res.ok) throw new Error(data?.error ?? t('common.error', { code: res.status }));
			toast(t('themes.uploaded'), 'success');
			await load();
		} catch (e) {
			toastError(e);
		} finally {
			busy = false;
		}
	}

	async function remove(th: ThemeInfo) {
		if (!confirm(t('themes.deleteConfirm', { name: th.name }))) return;
		try {
			await api(`/admin/themes/${th.id}`, { method: 'DELETE' });
			await load();
		} catch (e) {
			toastError(e);
		}
	}

	const swatches = (th: ThemeInfo) => [th.colors.primary, th.colors.secondary, th.colors.success, th.colors.info, th.colors.extra];
</script>

<div class="flex flex-col gap-3">
	{#each themes as th (th.id)}
		{@const on = th.id === active}
		<div class="card overflow-hidden p-0 {on ? 'ring-2 ring-pink' : ''}">
			<!-- Náhled v barvách a fontech motivu -->
			<div class="p-4" style="background: {th.colors.background}; color: {th.colors.text}">
				<div class="flex items-center gap-3 rounded-xl p-3" style="background: {th.colors.surface}">
					<div class="flex -space-x-1.5">
						{#each swatches(th) as c, i (i)}
							<span class="size-6 rounded-full ring-2" style="background: {c}; --tw-ring-color: {th.colors.surface}"></span>
						{/each}
					</div>
					<div class="min-w-0 flex-1">
						<div class="truncate text-xl leading-tight font-bold" style="font-family: '{th.fonts.heading}'">{th.name}</div>
						<div class="truncate text-sm opacity-70" style="font-family: '{th.fonts.body}'">{t('themes.sample')}</div>
					</div>
				</div>
			</div>
			<div class="p-4 pt-3">
				{#if th.description}<p class="text-sm text-white/70">{local(th.description, th.description_en)}</p>{/if}
				<div class="mt-1 text-xs text-white/40">
					{th.builtin ? t('themes.builtin') : t('themes.custom')}{th.author ? ` · ${th.author}` : ''} · {th.fonts.heading} / {th.fonts.body}
					{#if th.effects.length}<br />{t('themes.effects', { list: th.effects.map((e) => local(e.name, e.name_en)).join(', ') })}{/if}
				</div>
				<div class="mt-3 flex gap-2">
					{#if on}
						<span class="btn btn-mint flex-1"><Check class="size-5" strokeWidth={3} /> {t('themes.inUse')}</span>
					{:else}
						<button class="btn btn-pink flex-1" onclick={() => activate(th.id)}><Palette class="size-5" /> {t('themes.use')}</button>
					{/if}
					<a class="btn btn-ghost btn-icon" title={t('themes.download')} href="/api/themes/{th.id}/theme.json" download><Download class="size-5" /></a>
					{#if !th.builtin}
						<button class="btn btn-ghost btn-icon" title={t('common.delete')} onclick={() => remove(th)}><Trash2 class="size-5" /></button>
					{/if}
				</div>
			</div>
		</div>
	{/each}

	<section class="card flex flex-col gap-3">
		<h2 class="section-title"><FileUp class="size-5 text-pink" strokeWidth={2.5} /> {t('themes.customTitle')}</h2>
		<p class="text-sm text-white/60">
			{t('themes.customText')}
		</p>
		<button class="btn btn-pink" disabled={busy} onclick={() => fileInput?.click()}>
			{#if busy}<LoaderCircle class="size-5 animate-spin" />{:else}<FileUp class="size-5" />{/if} {t('themes.upload')}
		</button>
		<input
			bind:this={fileInput}
			type="file"
			accept=".json,.zip,application/json,application/zip"
			class="hidden"
			onchange={(e) => {
				const f = e.currentTarget.files?.[0];
				if (f) upload(f);
				e.currentTarget.value = '';
			}}
		/>
		<button class="text-left text-sm font-bold text-mint" onclick={() => (showHelp = !showHelp)}>{showHelp ? t('themes.helpHide') : t('themes.helpShow')}</button>
		{#if showHelp}
			{#if i18n.lang === 'en'}
				<div class="flex min-w-0 flex-col gap-2 overflow-hidden rounded-2xl bg-ink-3 p-3 text-sm break-words text-white/80">
					<p><b>colors</b> – background, surface, surface_2, surface_3 (background and cards), text, primary (main buttons), secondary, success, info, extra, on_accent (text on coloured buttons).</p>
					<p><b>fonts</b> – heading and body. Built-in: Baloo 2 Variable, Nunito Variable, Archivo Black, Courier Prime, Inter Variable, IBM Plex Mono. Or your own via <b>font_files</b>:
						<code class="block text-xs break-all whitespace-pre-wrap">{`[{"family":"Mine","file":"mine.woff2","weight":"400"}]`}</code> (file in the ZIP).</p>
					<p><b>radius</b> – roundness (0 = square, 1 = default, 2 = rounder).</p>
					<p><b>shadow</b> – chunky, soft, hard, none.</p>
					<p><b>pattern</b> – background pattern: dots, grid, lines, none.</p>
					<p><b>animations</b> – true / false (UI animations; special effects have their own switches).</p>
					<p><b>effects</b> – special effects: <code class="block text-xs break-all whitespace-pre-wrap">{`[{"id":"notes","name":"Flying notes","animated":true,"default":true,"css":""}]`}</code>. Built-in: notes, keys, ticker, pulse; any other ID = CSS only (field css).</p>
					<p><b>css</b> – extra CSS. Classes: .card, .btn, .input, .chip, .mb-brand, .mb-label, .mb-panel, .mb-progress, .mb-glow, .mb-accent-scope, .mb-vinyl-record, .mb-vinyl-cover, .mb-disc.</p>
				</div>
			{:else}
			<div class="flex min-w-0 flex-col gap-2 overflow-hidden rounded-2xl bg-ink-3 p-3 text-sm break-words text-white/80">
				<p><b>colors</b> – background, surface, surface_2, surface_3 (pozadí a karty), text, primary (hlavní tlačítka), secondary, success, info, extra.</p>
				<p><b>fonts</b> – heading a body. Vestavěné: Baloo 2 Variable, Nunito Variable, Archivo Black, Courier Prime, Inter Variable, IBM Plex Mono. Nebo vlastní přes <b>font_files</b>:
					<code class="block text-xs break-all whitespace-pre-wrap">{`[{"family":"Můj","file":"muj.woff2","weight":"400"}]`}</code> (soubor v ZIPu).</p>
				<p><b>radius</b> – zaoblení (0 = hranaté, 1 = výchozí, 2 = víc kulaté).</p>
				<p><b>shadow</b> – chunky, soft, hard, none.</p>
				<p><b>pattern</b> – vzor pozadí: dots, grid, lines, none.</p>
				<p><b>animations</b> – true / false.</p>
				<p><b>effects</b> – speciální efekty: <code class="block text-xs break-all whitespace-pre-wrap">{`[{"id":"notes","name":"Poletující noty","animated":true,"default":true,"css":""}]`}</code>. Vestavěné: notes, keys, ticker, pulse; jiné ID = jen CSS v poli css.</p>
				<p><b>colors.on_accent</b> – barva textu na barevných tlačítkách.</p>
				<p><b>css</b> – vlastní CSS navíc. Třídy: .card, .btn, .input, .chip, .mb-brand, .mb-label, .mb-panel, .mb-progress, .mb-glow, .mb-accent-scope, .mb-vinyl-record, .mb-vinyl-cover, .mb-disc.</p>
			</div>
			{/if}
		{/if}
	</section>
</div>
