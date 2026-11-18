<script lang="ts">
	import { Check, Clock, FolderOpen, ListMusic, LoaderCircle, Plus, Radio, Save, Search, X } from '@lucide/svelte';
	import { api } from '#lib/api.ts';
	import { toast, toastError } from '#lib/toasts.svelte.ts';
	import { t } from '#lib/i18n.svelte.ts';
	import type { RadioStation, Settings } from '#lib/types.ts';
	import Artwork from '#lib/components/Artwork.svelte';
	import Toggle from './Toggle.svelte';

	interface StationHit {
		name: string;
		url: string;
		logo: string | null;
		codec: string;
		bitrate: number;
		country: string;
	}

	let s = $state<Settings | null>(null);
	let musicDir = $state('');
	let saving = $state(false);
	let newFolder = $state('');

	let radioQ = $state('');
	let radioHits = $state<StationHit[] | null>(null);
	let radioLoading = $state(false);
	let manualName = $state('');
	let manualUrl = $state('');

	api<Settings>('/admin/settings').then((r) => (s = r), toastError);
	api<{ music_dir: string }>('/admin/status').then((r) => (musicDir = r.music_dir), () => {});

	async function save() {
		if (!s) return;
		saving = true;
		try {
			s = await api<Settings>('/admin/settings', { method: 'PUT', body: $state.snapshot(s) });
			toast(t('common.saved'), 'success');
		} catch (e) {
			toastError(e);
		} finally {
			saving = false;
		}
	}

	function addFolder() {
		const f = newFolder.trim();
		if (s && f && !s.local_folders.includes(f)) s.local_folders.push(f);
		newFolder = '';
	}

	async function searchRadio() {
		if (radioQ.trim().length < 2) return;
		radioLoading = true;
		try {
			radioHits = await api<StationHit[]>(`/admin/radio/search?q=${encodeURIComponent(radioQ.trim())}`);
		} catch (e) {
			toastError(e);
		} finally {
			radioLoading = false;
		}
	}

	function addStation(st: Omit<RadioStation, 'id'>) {
		if (!s) return;
		if (s.radio_stations.some((r) => r.url === st.url)) return;
		const id = Math.random().toString(36).slice(2, 10);
		s.radio_stations.push({ id, ...st });
		if (!s.radio_station) s.radio_station = id;
		toast(t('radio.added', { name: st.name }), 'success');
	}
</script>

{#if s}
	<div class="flex flex-col gap-4">
		<section class="card flex flex-col gap-4">
			<h2 class="section-title">{t('rules.people')}</h2>
			<Toggle bind:checked={s.allow_registration} label={t('rules.registration')} hint={t('rules.registrationHint')} />
			<Toggle bind:checked={s.prevent_duplicates} label={t('rules.duplicates')} hint={t('rules.duplicatesHint')} />
			<label>
				<span class="label">{t('rules.maxQueued')}</span>
				<input class="input" type="number" inputmode="numeric" min="0" bind:value={s.max_queued_per_user} />
				<span class="text-xs text-white/40">{t('common.noLimit')}</span>
			</label>
			<label>
				<span class="label">{t('rules.maxMinutes')}</span>
				<input class="input" type="number" inputmode="numeric" min="0" bind:value={s.max_track_minutes} />
				<span class="text-xs text-white/40">{t('rules.maxMinutesHint')}</span>
			</label>
			<label>
				<span class="label">{t('rules.idle')}</span>
				<select class="input" bind:value={s.idle_mode}>
					<option value="random_local">{t('rules.idleRandom')}</option>
					<option value="radio">{t('rules.idleRadio')}</option>
					<option value="silence">{t('rules.idleSilence')}</option>
				</select>
			</label>
		</section>

		<section class="card flex flex-col gap-4">
			<div>
				<h2 class="section-title">{t('rules.djSources')}</h2>
				<p class="text-sm text-white/50">{t('rules.djSourcesHint')}</p>
			</div>
			<Toggle bind:checked={s.dj_sources.youtube} label={t('rules.djYoutube')} />
			<Toggle bind:checked={s.dj_sources.links} label={t('rules.djLinks')} />
			<Toggle bind:checked={s.dj_sources.local} label={t('rules.djLocal')} />
		</section>

		<!-- Fronta: férovost a prolínání -->
		<section class="card flex flex-col gap-4">
			<h2 class="section-title"><ListMusic class="size-5 text-pink" strokeWidth={2.5} /> {t('rules.queue')}</h2>
			<Toggle bind:checked={s.fair_queue} label={t('rules.fair')} hint={t('rules.fairHint')} />
			<Toggle
				checked={s.crossfade_sec > 0}
				onchange={(on) => (s!.crossfade_sec = on ? 5 : 0)}
				label={t('rules.crossfade')}
				hint={t('rules.crossfadeHint')}
			/>
			{#if s.crossfade_sec > 0}
				<label>
					<span class="label">{t('rules.crossfadeLength', { n: s.crossfade_sec })}</span>
					<input type="range" min="1" max="15" step="1" bind:value={s.crossfade_sec} class="w-full accent-pink" />
				</label>
			{/if}
		</section>

		<!-- Hlasitost podle času -->
		<section class="card flex flex-col gap-3">
			<h2 class="section-title"><Clock class="size-5 text-pink" strokeWidth={2.5} /> {t('rules.zones')}</h2>
			<p class="text-sm text-white/60">{t('rules.zonesHint')}</p>
			{#each s.volume_zones as z, i (i)}
				<div class="flex flex-col gap-3 rounded-2xl bg-ink-3 p-3">
					<div class="flex items-center gap-2">
						<input type="checkbox" class="size-5 shrink-0 accent-pink" bind:checked={z.enabled} />
						<input class="input min-h-10 py-2" bind:value={z.name} placeholder={t('rules.zoneName')} maxlength="40" />
						<button class="grid size-10 shrink-0 place-items-center text-white/50" title={t('common.remove')} onclick={() => s!.volume_zones.splice(i, 1)}><X class="size-5" /></button>
					</div>
					<div class="grid grid-cols-2 gap-2">
						<label><span class="label">{t('rules.zoneFrom')}</span><input class="input min-h-10 py-2" type="time" bind:value={z.from} /></label>
						<label><span class="label">{t('rules.zoneTo')}</span><input class="input min-h-10 py-2" type="time" bind:value={z.to} /></label>
					</div>
					<label>
						<span class="label">{t('rules.zoneVolume', { n: Math.round(z.volume * 100) })}</span>
						<input type="range" min="0" max="1" step="0.05" bind:value={z.volume} class="w-full accent-pink" />
					</label>
				</div>
			{/each}
			<button
				class="btn btn-ghost"
				onclick={() => s!.volume_zones.push({ name: t('rules.zoneDefaultName'), from: '22:00', to: '07:00', volume: 0.3, enabled: true })}
			>
				<Plus class="size-5" strokeWidth={3} /> {t('rules.zoneAdd')}
			</button>
		</section>

		<!-- Rádio -->
		<section class="card flex flex-col gap-3">
			<h2 class="section-title"><Radio class="size-5 text-pink" strokeWidth={2.5} /> {t('radio.title')}</h2>
			<p class="text-sm text-white/60">{t('radio.hint')}</p>
			<div class="flex flex-col divide-y divide-white/5">
				{#each s.radio_stations as st, i (st.id)}
					{@const on = s.radio_station === st.id}
					<div class="flex items-center gap-3 py-2">
						<Artwork url={st.logo} seed={st.name} class="size-11 bg-white object-contain" />
						<div class="min-w-0 flex-1">
							<div class="truncate font-bold">{st.name}</div>
							<div class="truncate font-mono text-xs text-white/40">{st.url}</div>
						</div>
						{#if on}
							<span class="grid size-11 place-items-center rounded-xl bg-mint/20 text-mint" title={t('radio.active')}><Check class="size-5" strokeWidth={3} /></span>
						{:else}
							<button class="btn btn-ghost px-3 text-sm" onclick={() => (s!.radio_station = st.id)}>{t('radio.choose')}</button>
						{/if}
						<button class="grid size-10 place-items-center text-white/40" title={t('common.remove')} onclick={() => s!.radio_stations.splice(i, 1)}><X class="size-5" /></button>
					</div>
				{/each}
			</div>

			<form
				class="flex gap-2"
				onsubmit={(e) => {
					e.preventDefault();
					searchRadio();
				}}
			>
				<input class="input" type="search" bind:value={radioQ} placeholder={t('radio.search')} />
				<button class="btn btn-ghost btn-icon size-12" title={t('common.search')}>
					{#if radioLoading}<LoaderCircle class="size-5 animate-spin" />{:else}<Search class="size-5" />{/if}
				</button>
			</form>
			{#if radioHits}
				<div class="flex max-h-80 flex-col divide-y divide-white/5 overflow-y-auto rounded-2xl bg-ink-3/60 px-2">
					{#each radioHits as h (h.url)}
						<div class="flex items-center gap-3 py-2">
							<Artwork url={h.logo} seed={h.name} class="size-10 bg-white object-contain" />
							<div class="min-w-0 flex-1">
								<div class="truncate font-bold">{h.name}</div>
								<div class="text-xs text-white/40">{h.country} · {h.codec} {h.bitrate ? `${h.bitrate} kb/s` : ''}</div>
							</div>
							<button class="btn btn-pink btn-icon" title={t('radio.add')} onclick={() => addStation({ name: h.name, url: h.url, logo: h.logo })}>
								<Plus class="size-5" strokeWidth={3} />
							</button>
						</div>
					{:else}
						<p class="py-4 text-center text-sm text-white/50">{t('add.nothing')}</p>
					{/each}
				</div>
			{/if}
			<details>
				<summary class="cursor-pointer text-sm font-bold text-mint">{t('radio.manual')}</summary>
				<div class="mt-2 flex flex-col gap-2">
					<input class="input" bind:value={manualName} placeholder={t('radio.name')} />
					<input class="input font-mono" bind:value={manualUrl} placeholder={t('radio.url')} autocapitalize="none" />
					<button
						class="btn btn-ghost"
						disabled={!manualName.trim() || !/^https?:\/\//.test(manualUrl.trim())}
						onclick={() => {
							addStation({ name: manualName.trim(), url: manualUrl.trim(), logo: null });
							manualName = manualUrl = '';
						}}><Plus class="size-5" /> {t('radio.add')}</button
					>
				</div>
			</details>
		</section>

		<section class="card flex flex-col gap-3">
			<h2 class="section-title"><FolderOpen class="size-5 text-pink" strokeWidth={2.5} /> {t('rules.music')}</h2>
			<div class="rounded-2xl bg-ink-3 p-3 text-sm">
				<div class="text-white/60">{t('rules.mainFolder')}</div>
				<div class="font-mono break-all text-mint">{musicDir || '…'}</div>
				<div class="mt-1 text-white/50">{t('rules.folderHint')}</div>
			</div>
			<span class="label mt-1 mb-0">{t('rules.otherFolders')}</span>
			{#each s.local_folders as f, i (f)}
				<div class="flex items-center gap-2 rounded-2xl bg-ink-3 py-1 pr-1 pl-3">
					<span class="min-w-0 flex-1 font-mono text-sm break-all">{f}</span>
					<button class="grid size-10 place-items-center text-white/50" title={t('common.remove')} onclick={() => s!.local_folders.splice(i, 1)}><X class="size-5" /></button>
				</div>
			{/each}
			<div class="flex gap-2">
				<input
					class="input font-mono"
					bind:value={newFolder}
					placeholder={t('rules.folderPlaceholder')}
					autocapitalize="none"
					onkeydown={(e) => {
						if (e.key === 'Enter') {
							e.preventDefault();
							addFolder();
						}
					}}
				/>
				<button class="btn btn-ghost btn-icon size-12" title={t('rules.addFolder')} onclick={addFolder}><Plus class="size-5" strokeWidth={3} /></button>
			</div>
		</section>

		<button class="btn btn-pink sticky bottom-24 min-h-13 text-lg" disabled={saving} onclick={save}>
			{#if saving}<LoaderCircle class="size-5 animate-spin" />{:else}<Save class="size-5" />{/if} {t('common.save')}
		</button>
	</div>
{:else}
	<div class="flex justify-center py-10 text-pink"><LoaderCircle class="size-8 animate-spin" /></div>
{/if}
