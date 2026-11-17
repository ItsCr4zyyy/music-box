<script lang="ts">
	import { t } from '#lib/i18n.svelte.ts';
	import { Lock, UserPlus } from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { api } from '#lib/api.ts';
	import { loadSession, session } from '#lib/session.svelte.ts';
	import type { User } from '#lib/types.ts';

	let username = $state('');
	let displayName = $state('');
	let password = $state('');
	let password2 = $state('');
	let error = $state('');
	let busy = $state(false);

	$effect(() => {
		if (session.user) goto('/');
	});

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		if (password !== password2) {
			error = t('reg.mismatch');
			return;
		}
		busy = true;
		try {
			const r = await api<{ user: User }>('/auth/register', {
				body: { username, password, display_name: displayName }
			});
			session.user = r.user;
			await loadSession();
			goto('/');
		} catch (err) {
			error = (err as Error).message;
		} finally {
			busy = false;
		}
	}
</script>

<div class="animate-pop pt-4">
	<div class="mb-5 flex flex-col items-center text-center">
		<div class="grid size-16 place-items-center rounded-3xl bg-mint text-on-accent" style="box-shadow: 0 5px 0 0 rgb(0 0 0 / 0.3)">
			<UserPlus class="size-8" strokeWidth={2.5} />
		</div>
		<h1 class="mt-3 text-3xl font-bold">{t('reg.title')}</h1>
		<p class="text-white/60">
			{#if session.info?.needs_setup}
				{t('reg.firstAdmin')}
			{:else}
				{t('reg.needsApproval')}
			{/if}
		</p>
	</div>

	{#if session.info && !session.info.allow_registration && !session.info.needs_setup}
		<div class="card flex flex-col items-center gap-2 py-8 text-center text-white/70">
			<Lock class="size-8 text-white/40" /> {t('reg.closed')}
		</div>
	{:else}
		<form class="card flex flex-col gap-4" onsubmit={submit}>
			<label>
				<span class="label">{t('reg.username')}</span>
				<input class="input" bind:value={username} autocomplete="username" autocapitalize="none" minlength="3" maxlength="32" required />
			</label>
			<label>
				<span class="label">{t('reg.displayName')} <span class="font-normal text-white/40">{t('reg.displayNameHint')}</span></span>
				<input class="input" bind:value={displayName} maxlength="40" placeholder={username || t('reg.displayNamePlaceholder')} />
			</label>
			<label>
				<span class="label">{t('reg.password')}</span>
				<input class="input" type="password" bind:value={password} autocomplete="new-password" minlength="6" required />
			</label>
			<label>
				<span class="label">{t('reg.password2')}</span>
				<input class="input" type="password" bind:value={password2} autocomplete="new-password" minlength="6" required />
			</label>
			{#if error}<p class="rounded-xl bg-pink/15 px-3 py-2 text-sm font-bold text-pink">{error}</p>{/if}
			<button class="btn btn-mint min-h-13 text-lg" disabled={busy}>{busy ? t('common.moment') : t('reg.submit')}</button>
		</form>
	{/if}

	<p class="mt-5 text-center text-white/60">
		{t('reg.haveAccount')} <a class="font-bold text-mint" href="/login">{t('reg.login')}</a>
	</p>
</div>
