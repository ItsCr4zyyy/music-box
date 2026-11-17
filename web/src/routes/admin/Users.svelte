<script lang="ts">
	import { Ban, Check, Eye, ImageOff, KeyRound, RotateCcw, Trash2, UserPlus, X } from '@lucide/svelte';
	import { api } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { session } from '#lib/session.svelte.ts';
	import { toast, toastError } from '#lib/toasts.svelte.ts';
	import type { Role, User, UserStatus } from '#lib/types.ts';
	import { dateLocale, t } from '#lib/i18n.svelte.ts';
	import Avatar from '#lib/components/Avatar.svelte';
	import { slide } from 'svelte/transition';

	let users = $state<User[]>([]);
	let loaded = $state(false);
	let open = $state<number | null>(null);

	async function load() {
		try {
			users = await api<User[]>('/admin/users');
		} catch (e) {
			toastError(e);
		}
		loaded = true;
	}
	load();
	$effect(() =>
		live.on((type) => {
			if (type === 'users') load();
		})
	);

	const pending = $derived(users.filter((u) => u.status === 'pending'));
	const others = $derived(users.filter((u) => u.status !== 'pending'));

	async function patch(u: User, body: { role?: Role; status?: UserStatus; password?: string }) {
		try {
			await api(`/admin/users/${u.id}`, { method: 'PATCH', body });
			await load();
		} catch (e) {
			toastError(e);
		}
	}

	async function remove(u: User) {
		if (!confirm(t('users.deleteConfirm', { name: u.display_name }))) return;
		try {
			await api(`/admin/users/${u.id}`, { method: 'DELETE' });
			await load();
		} catch (e) {
			toastError(e);
		}
	}

	async function removeAvatar(u: User) {
		try {
			await api(`/admin/users/${u.id}/avatar`, { method: 'DELETE' });
			await load();
		} catch (e) {
			toastError(e);
		}
	}

	async function resetPassword(u: User) {
		const password = prompt(t('users.newPassword', { name: u.display_name }));
		if (!password) return;
		await patch(u, { password });
		toast(t('users.passwordChanged'), 'success');
	}

	const roles: Role[] = ['guest', 'dj', 'admin'];
	const date = (ts: number) => new Date(ts * 1000).toLocaleDateString(dateLocale());
</script>

{#if pending.length}
	<section class="card mb-4 ring-2 ring-sun/40">
		<h2 class="section-title mb-2"><UserPlus class="size-5 text-sun" strokeWidth={2.5} /> {t('users.pending', { n: pending.length })}</h2>
		<div class="flex flex-col gap-2">
			{#each pending as u (u.id)}
				<div class="rounded-2xl bg-ink-3 p-3">
					<div class="flex items-center gap-3">
						<Avatar name={u.display_name} url={u.avatar_url} class="size-10 text-lg" />
						<div class="min-w-0 flex-1">
							<div class="truncate font-bold">{u.display_name}</div>
							<div class="truncate text-xs text-white/50">@{u.username} · {date(u.created_at)}</div>
						</div>
						<button class="grid size-10 place-items-center rounded-xl text-white/50" title={t('users.reject')} onclick={() => remove(u)}><X class="size-5" /></button>
					</div>
					<div class="mt-3 grid grid-cols-2 gap-2">
						<button class="btn btn-mint" onclick={() => patch(u, { status: 'active', role: 'dj' })}><Check class="size-5" strokeWidth={3} /> {t('users.asDj')}</button>
						<button class="btn btn-ghost" onclick={() => patch(u, { status: 'active', role: 'guest' })}><Eye class="size-5" /> {t('users.asGuest')}</button>
					</div>
				</div>
			{/each}
		</div>
	</section>
{/if}

<section class="card">
	<h2 class="section-title mb-1">{t('users.all')}</h2>
	{#if !loaded}<p class="text-white/50">{t('common.loading')}</p>{/if}
	<div class="flex flex-col divide-y divide-white/5">
		{#each others as u (u.id)}
			{@const me = u.id === session.user?.id}
			<div class="py-2" class:opacity-50={u.status === 'banned'}>
				<button class="flex w-full items-center gap-3 text-left" disabled={me} onclick={() => (open = open === u.id ? null : u.id)}>
					<Avatar name={u.display_name} url={u.avatar_url} class="size-10 text-lg" />
					<div class="min-w-0 flex-1">
						<div class="flex items-center gap-1.5 font-bold">
							<span class="truncate">{u.display_name}</span>
							{#if me}<span class="chip bg-pink/20 text-pink">{t('users.you')}</span>{/if}
							{#if u.status === 'banned'}<Ban class="size-4 text-pink" />{/if}
						</div>
						<div class="truncate text-xs text-white/50">@{u.username} · {t('users.since', { date: date(u.created_at) })}</div>
					</div>
					<span class="chip shrink-0 px-2.5 py-1 {u.role === 'admin' ? 'bg-pink text-on-accent' : u.role === 'dj' ? 'bg-mint text-on-accent' : 'bg-ink-4 text-white'}">{t(`role.${u.role}`)}</span>
				</button>
				{#if open === u.id && !me}
					<div transition:slide={{ duration: 150 }} class="mt-3 flex flex-col gap-2">
						<div class="grid grid-cols-3 gap-1 rounded-2xl bg-ink-3 p-1">
							{#each roles as r (r)}
								<button
									class="rounded-xl py-2 text-sm font-bold transition {u.role === r ? 'bg-pink text-on-accent' : 'text-white/70'}"
									onclick={() => u.role !== r && patch(u, { role: r })}>{t(`role.${r}`)}</button
								>
							{/each}
						</div>
						<div class="grid grid-cols-3 gap-2">
							<button class="btn btn-ghost px-0 text-sm" onclick={() => resetPassword(u)}><KeyRound class="size-4" /> {t('users.password')}</button>
							{#if u.status === 'banned'}
								<button class="btn btn-ghost px-0 text-sm" onclick={() => patch(u, { status: 'active' })}><RotateCcw class="size-4" /> {t('users.unblock')}</button>
							{:else}
								<button class="btn btn-ghost px-0 text-sm" onclick={() => patch(u, { status: 'banned' })}><Ban class="size-4" /> {t('users.block')}</button>
							{/if}
							<button class="btn btn-pink px-0 text-sm" onclick={() => remove(u)}><Trash2 class="size-4" /> {t('common.delete')}</button>
						</div>
						{#if u.avatar_url}
							<button class="btn btn-ghost text-sm" onclick={() => removeAvatar(u)}><ImageOff class="size-4" /> {t('users.removePhoto')}</button>
						{/if}
					</div>
				{/if}
			</div>
		{/each}
	</div>
</section>
