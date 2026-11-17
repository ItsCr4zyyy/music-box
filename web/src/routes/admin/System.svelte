<script lang="ts">
	import { t } from '#lib/i18n.svelte.ts';
	import { CircleCheck, Download, FolderOpen, LoaderCircle, RefreshCw, TriangleAlert, CirclePlay, Speaker } from '@lucide/svelte';
	import type { AudioDevice, Settings } from '#lib/types.ts';
	import { api } from '#lib/api.ts';
	import { toast, toastError } from '#lib/toasts.svelte.ts';

	interface Status {
		ytdlp_version: string | null;
		ytdlp_error: string | null;
		ffmpeg: boolean;
		local_tracks: number;
		join_url: string;
		data_dir: string;
		music_dir: string;
	}

	let status = $state<Status | null>(null);
	let busy = $state<Record<string, boolean>>({});

	async function load() {
		try {
			status = await api<Status>('/admin/status');
		} catch (e) {
			toastError(e);
		}
	}
	load();

	let devices = $state<AudioDevice[] | null>(null);
	let currentDevice = $state('');
	let devBusy = $state(false);

	async function loadDevices() {
		devBusy = true;
		try {
			const [list, s] = await Promise.all([api<AudioDevice[]>('/admin/audio/devices'), api<Settings>('/admin/settings')]);
			devices = list;
			currentDevice = s.audio_device;
		} catch (e) {
			toastError(e);
		} finally {
			devBusy = false;
		}
	}
	loadDevices();

	async function setDevice(id: string) {
		try {
			const s = await api<Settings>('/admin/settings');
			s.audio_device = id;
			await api('/admin/settings', { method: 'PUT', body: s });
			currentDevice = id;
			toast(t('system.audioSaved'), 'success');
		} catch (e) {
			toastError(e);
		}
	}

	const defaultName = $derived(devices?.find((d) => d.is_default)?.name ?? '');
	const missing = $derived(!!devices && !!currentDevice && !devices.some((d) => d.id === currentDevice));

	async function action(key: string, path: string, done: (r: any) => string) {
		busy[key] = true;
		try {
			const r = await api(path, { method: 'POST' });
			toast(done(r), 'success');
			await load();
		} catch (e) {
			toastError(e);
		} finally {
			busy[key] = false;
		}
	}
</script>

{#if status}
	<div class="flex flex-col gap-4">
		<section class="card flex flex-col items-center text-center">
			<img src="/api/qr.svg?u={encodeURIComponent(status.join_url)}" alt="QR" class="size-44 rounded-2xl bg-white p-1" />
			<div class="mt-2 text-sm text-white/50">{t('system.join')}</div>
			<a class="font-display text-lg font-bold break-all text-mint" href={status.join_url}>{status.join_url}</a>
		</section>

		<section class="card flex flex-col gap-3">
			<h2 class="section-title"><Speaker class="size-5 text-pink" strokeWidth={2.5} /> {t('system.audio')}</h2>
			<p class="text-sm text-white/60">{t('system.audioHint')}</p>
			{#if devices}
				<select class="input" value={currentDevice} onchange={(e) => setDevice(e.currentTarget.value)}>
					<option value="">{t('system.audioDefault')}{defaultName ? ` (${t('system.audioDefaultNow', { name: defaultName })})` : ''}</option>
					{#each devices as d (d.id)}<option value={d.id}>{d.name}</option>{/each}
					{#if missing}<option value={currentDevice}>{currentDevice}</option>{/if}
				</select>
				{#if missing}<p class="flex items-center gap-2 text-sm text-sun"><TriangleAlert class="size-4 shrink-0" /> {t('system.audioMissing')}</p>{/if}
			{/if}
			<button class="btn btn-ghost" disabled={devBusy} onclick={loadDevices}>
				<RefreshCw class="size-5 {devBusy ? 'animate-spin' : ''}" /> {t('system.audioRefresh')}
			</button>
		</section>

		<section class="card flex flex-col gap-3">
			<h2 class="section-title"><FolderOpen class="size-5 text-pink" strokeWidth={2.5} /> {t('system.library')}</h2>
			<p>{t('system.localTracks', { n: status.local_tracks })}</p>
			<div class="rounded-2xl bg-ink-3 p-3 font-mono text-sm break-all text-mint">{status.music_dir}</div>
			<button class="btn btn-ghost" disabled={busy.scan} onclick={() => action('scan', '/admin/library/rescan', (r) => t('system.rescanned', { n: r.count }))}>
				<RefreshCw class="size-5 {busy.scan ? 'animate-spin' : ''}" /> {t('system.rescan')}
			</button>
		</section>

		<section class="card flex flex-col gap-3">
			<h2 class="section-title"><CirclePlay class="size-5 text-pink" strokeWidth={2.5} /> YouTube (yt-dlp)</h2>
			{#if status.ytdlp_version}
				<p class="flex items-center gap-2"><CircleCheck class="size-5 text-mint" /> {t('system.version')} <b class="font-mono">{status.ytdlp_version}</b></p>
			{:else}
				<p class="flex items-center gap-2 text-pink"><TriangleAlert class="size-5" /> {status.ytdlp_error}</p>
			{/if}
			<p class="flex items-center gap-2 text-sm text-white/60">
				{#if status.ffmpeg}<CircleCheck class="size-4 text-mint" /> {t('system.ffmpegOk')}{:else}<TriangleAlert class="size-4 text-sun" /> {t('system.ffmpegMissing')}{/if}
			</p>
			<button class="btn btn-ghost" disabled={busy.yt} onclick={() => action('yt', '/admin/ytdlp/update', (r) => r.output || t('system.done'))}>
				{#if busy.yt}<LoaderCircle class="size-5 animate-spin" />{:else}<Download class="size-5" />{/if} {t('system.updateYtdlp')}
			</button>
		</section>

		<p class="text-xs break-all text-white/40">{t('system.data')} <span class="font-mono">{status.data_dir}</span></p>
	</div>
{:else}
	<div class="flex justify-center py-10 text-pink"><LoaderCircle class="size-8 animate-spin" /></div>
{/if}
