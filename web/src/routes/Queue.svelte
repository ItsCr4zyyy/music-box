<script lang="ts">
	import { ChevronDown, ChevronUp, Dices, History, ListMusic, Trash2, X, Zap } from '@lucide/svelte';
	import { api } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { session } from '#lib/session.svelte.ts';
	import { toastError } from '#lib/toasts.svelte.ts';
	import { t } from '#lib/i18n.svelte.ts';
	import { fmtTime, hasRole, type QueueEntry } from '#lib/types.ts';
	import Avatar from '#lib/components/Avatar.svelte';
	import TrackRow from '#lib/components/TrackRow.svelte';
	import { flip } from 'svelte/animate';
	import { slide } from 'svelte/transition';

	const queue = $derived(live.state?.queue ?? []);
	const isAdmin = $derived(hasRole(session.user, 'admin'));
	const total = $derived(queue.reduce((s, e) => s + e.track.duration_sec, 0));
	/** Admin: rozbalená položka s ovládáním. */
	let open = $state<number | null>(null);

	async function call(path: string, method = 'POST', body?: unknown) {
		try {
			await api(path, { method, body });
		} catch (e) {
			toastError(e);
		}
	}

	const canRemove = (e: QueueEntry) => isAdmin || (!!e.added_by && e.added_by.id === session.user?.id);
</script>

<section class="card">
	<div class="mb-1 flex items-center gap-2">
		<h2 class="section-title"><ListMusic class="size-5 text-pink" strokeWidth={2.5} /> {t('queue.title')}</h2>
		<span class="mb-label text-sm text-white/50">{queue.length} · {fmtTime(total)}</span>
		<div class="flex-1"></div>
		{#if isAdmin && queue.length}
			<button
				class="flex items-center gap-1 text-sm font-bold text-white/40 active:text-pink"
				onclick={() => confirm(t('queue.clearConfirm')) && call('/queue', 'DELETE')}
				><Trash2 class="size-4" /> {t('queue.clear')}</button
			>
		{/if}
	</div>

	{#if queue.length}
		<ol class="flex flex-col divide-y divide-white/5">
			{#each queue as e, i (e.id)}
				<li animate:flip={{ duration: 250 }}>
					<TrackRow track={e.track} number={i + 1}>
						{#snippet meta()}
							<div class="mt-0.5 flex items-center gap-1.5 text-xs text-white/50">
								{#if e.priority}<span class="chip bg-sun/20 text-sun"><Zap class="size-3" strokeWidth={3} /> {t('queue.priority')}</span>{/if}
								{#if e.added_by}
									<Avatar name={e.added_by.name} url={e.added_by.avatar_url} class="size-4 text-[9px]" />
									<span class="truncate">{e.added_by.name}</span>
								{:else}
									<Dices class="size-3.5" /> music-box
								{/if}
							</div>
						{/snippet}
						{#if isAdmin}
							<button
								class="grid size-11 place-items-center rounded-xl text-white/50 transition active:bg-ink-3 {open === e.id ? 'bg-ink-3 text-white' : ''}"
								title={t('queue.edit')}
								onclick={() => (open = open === e.id ? null : e.id)}
							>
								<ChevronDown class="size-5 transition {open === e.id ? 'rotate-180' : ''}" strokeWidth={2.5} />
							</button>
						{:else if canRemove(e)}
							<button class="btn btn-ghost btn-icon" title={t('common.remove')} onclick={() => call(`/queue/${e.id}`, 'DELETE')}>
								<X class="size-5" strokeWidth={2.5} />
							</button>
						{/if}
					</TrackRow>
					{#if isAdmin && open === e.id}
						<div transition:slide={{ duration: 150 }} class="mb-2 grid grid-cols-4 gap-2">
							<button class="btn btn-sun px-0" title={t('queue.promote')} disabled={i === 0} onclick={() => call(`/queue/${e.id}/promote`)}>
								<Zap class="size-5" strokeWidth={2.5} />
							</button>
							<button class="btn btn-ghost px-0" title={t('queue.up')} disabled={i === 0} onclick={() => call(`/queue/${e.id}/move`, 'POST', { dir: -1 })}>
								<ChevronUp class="size-5" strokeWidth={2.5} />
							</button>
							<button class="btn btn-ghost px-0" title={t('queue.down')} disabled={i === queue.length - 1} onclick={() => call(`/queue/${e.id}/move`, 'POST', { dir: 1 })}>
								<ChevronDown class="size-5" strokeWidth={2.5} />
							</button>
							<button class="btn btn-pink px-0" title={t('common.remove')} onclick={() => call(`/queue/${e.id}`, 'DELETE')}>
								<X class="size-5" strokeWidth={2.5} />
							</button>
						</div>
					{/if}
				</li>
			{/each}
		</ol>
	{:else}
		<p class="py-6 text-center text-white/50">{t('queue.empty')}</p>
	{/if}

	{#if live.state?.history.length}
		<details class="mt-2 border-t border-white/5 pt-3">
			<summary class="flex cursor-pointer list-none items-center gap-1.5 text-sm font-bold text-white/50">
				<History class="size-4" /> {t('queue.history')}
			</summary>
			<div class="mt-1 flex flex-col opacity-70">
				{#each live.state.history as e (e.id)}
					<TrackRow track={e.track}>
						{#if e.added_by}<span class="max-w-20 truncate text-xs text-white/50">{e.added_by.name}</span>{/if}
					</TrackRow>
				{/each}
			</div>
		</details>
	{/if}
</section>
