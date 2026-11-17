<script lang="ts">
	import { Camera, Check, Hourglass, KeyRound, Languages, LoaderCircle, LogOut, Trash2, UserPen } from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { api } from '#lib/api.ts';
	import { logout, session } from '#lib/session.svelte.ts';
	import { toast, toastError } from '#lib/toasts.svelte.ts';
	import type { User } from '#lib/types.ts';
	import { i18n, languages, setUserLang, t, type Lang } from '#lib/i18n.svelte.ts';
	import Avatar from '#lib/components/Avatar.svelte';
	import Poll from '../Poll.svelte';

	$effect(() => {
		if (session.loaded && !session.user) goto('/login');
	});

	let nickname = $state(session.user?.display_name ?? '');
	let nickBusy = $state(false);

	async function saveNickname(e: SubmitEvent) {
		e.preventDefault();
		nickBusy = true;
		try {
			session.user = (await api<{ user: User }>('/me', { method: 'PATCH', body: { display_name: nickname } })).user;
			toast(t('account.nicknameSaved'), 'success');
		} catch (err) {
			toastError(err);
		} finally {
			nickBusy = false;
		}
	}

	let oldPw = $state('');
	let newPw = $state('');
	let busy = $state(false);

	async function changePassword(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		try {
			await api('/me/password', { body: { old: oldPw, new: newPw } });
			oldPw = newPw = '';
			toast(t('account.passwordChanged'), 'success');
		} catch (err) {
			toastError(err);
		} finally {
			busy = false;
		}
	}

	let avatarBusy = $state(false);
	let fileInput = $state<HTMLInputElement>();

	/** Ořízne fotku na čtverec 400×400 (menší soubor, bez metadat jako GPS). */
	async function squareImage(file: File): Promise<Blob> {
		const bmp = await createImageBitmap(file);
		const side = Math.min(bmp.width, bmp.height);
		const canvas = document.createElement('canvas');
		canvas.width = canvas.height = 400;
		canvas.getContext('2d')!.drawImage(bmp, (bmp.width - side) / 2, (bmp.height - side) / 2, side, side, 0, 0, 400, 400);
		return new Promise((res, rej) => canvas.toBlob((b) => (b ? res(b) : rej(new Error(t('account.photoError')))), 'image/jpeg', 0.88));
	}

	async function uploadAvatar(file: File) {
		avatarBusy = true;
		try {
			const blob = await squareImage(file);
			const res = await fetch('/api/me/avatar', { method: 'POST', headers: { 'Content-Type': 'image/jpeg', 'X-Lang': i18n.lang }, body: blob });
			const data = await res.json().catch(() => null);
			if (!res.ok) throw new Error(data?.error ?? t('common.error', { code: res.status }));
			session.user = (data as { user: User }).user;
			toast(t('account.photoUploaded'), 'success');
		} catch (e) {
			toastError(e);
		} finally {
			avatarBusy = false;
		}
	}

	async function removeAvatar() {
		try {
			session.user = (await api<{ user: User }>('/me/avatar', { method: 'DELETE' })).user;
		} catch (e) {
			toastError(e);
		}
	}

	async function doLogout() {
		await logout();
		goto('/');
	}


</script>

{#if session.user}
	{@const u = session.user}
	<div class="card mb-4 flex flex-col items-center text-center">
		<button class="relative" title={t('account.changePhoto')} disabled={u.status !== 'active' || avatarBusy} onclick={() => fileInput?.click()}>
			<Avatar name={u.display_name} url={u.avatar_url} class="size-24 text-5xl" />
			{#if u.status === 'active'}
				<span class="absolute -right-1 -bottom-1 grid size-9 place-items-center rounded-full bg-pink text-on-accent ring-4 ring-ink-2">
					{#if avatarBusy}<LoaderCircle class="size-4 animate-spin" />{:else}<Camera class="size-4" strokeWidth={2.5} />{/if}
				</span>
			{/if}
		</button>
		<input
			bind:this={fileInput}
			type="file"
			accept="image/*"
			class="hidden"
			onchange={(e) => {
				const f = e.currentTarget.files?.[0];
				if (f) uploadAvatar(f);
				e.currentTarget.value = '';
			}}
		/>
		{#if u.avatar_url}
			<button class="mt-2 flex items-center gap-1 text-xs font-bold text-white/40" onclick={removeAvatar}><Trash2 class="size-3.5" /> {t('account.removePhoto')}</button>
		{/if}
		<h1 class="mt-2 text-2xl font-bold">{u.display_name}</h1>
		<div class="text-sm text-white/50">@{u.username}</div>
		{#if u.status === 'pending'}
			<div class="mt-3 flex items-center gap-2 rounded-2xl bg-sun/15 px-3 py-2 text-sm font-bold text-sun">
				<Hourglass class="size-4" /> {t('account.pending')}
			</div>
			<Poll />
		{:else}
			<span class="chip mt-3 bg-pink px-3 py-1 text-sm text-on-accent">{t(`role.${u.role}`)}</span>
			<p class="mt-2 text-sm text-white/60">{t(`role.info.${u.role}`)}</p>
		{/if}
	</div>

	{#if u.status === 'active'}
		<form class="card mb-4 flex flex-col gap-3" onsubmit={saveNickname}>
			<h2 class="section-title"><UserPen class="size-5 text-pink" strokeWidth={2.5} /> {t('account.nickname')}</h2>
			<p class="-mt-1 text-sm text-white/50">{t('account.nicknameHint')}</p>
			<div class="flex gap-2">
				<input class="input" bind:value={nickname} maxlength="40" required />
				<button class="btn btn-pink btn-icon size-12" title={t('common.save')} disabled={nickBusy || !nickname.trim() || nickname.trim() === u.display_name}>
					{#if nickBusy}<LoaderCircle class="size-5 animate-spin" />{:else}<Check class="size-5" strokeWidth={3} />{/if}
				</button>
			</div>
		</form>
	{/if}

	<form class="card mb-4 flex flex-col gap-3" onsubmit={changePassword}>
		<h2 class="section-title"><KeyRound class="size-5 text-pink" strokeWidth={2.5} /> {t('account.password')}</h2>
		<input class="input" type="password" placeholder={t('account.currentPassword')} autocomplete="current-password" bind:value={oldPw} required />
		<input class="input" type="password" placeholder={t('account.newPassword')} autocomplete="new-password" minlength="6" bind:value={newPw} required />
		<button class="btn btn-ghost" disabled={busy}>{t('account.changePassword')}</button>
	</form>

	<section class="card mb-4 flex flex-col gap-3">
		<h2 class="section-title"><Languages class="size-5 text-pink" strokeWidth={2.5} /> {t('account.language')}</h2>
		<select
			class="input"
			value={i18n.userChoice ?? ''}
			onchange={(e) => setUserLang((e.currentTarget.value || null) as Lang | null)}
		>
			<option value="">{t('account.languageAuto')}</option>
			{#each languages as l (l.id)}<option value={l.id}>{l.name}</option>{/each}
		</select>
	</section>

	<button class="btn btn-ghost w-full" onclick={doLogout}><LogOut class="size-5" /> {t('account.logout')}</button>
{/if}
