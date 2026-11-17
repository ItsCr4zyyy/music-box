<script lang="ts">
	import { t } from '#lib/i18n.svelte.ts';
	import { Headphones, Sparkles } from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { api } from '#lib/api.ts';
	import { session } from '#lib/session.svelte.ts';
	import type { User } from '#lib/types.ts';

	let username = $state('');
	let password = $state('');
	let error = $state('');
	let busy = $state(false);

	$effect(() => {
		if (session.user) goto('/');
	});

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			session.user = (await api<{ user: User }>('/auth/login', { body: { username, password } })).user;
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
		<div class="grid size-16 place-items-center rounded-3xl bg-pink text-on-accent" style="box-shadow: 0 5px 0 0 rgb(0 0 0 / 0.3)">
			<Headphones class="size-8" strokeWidth={2.5} />
		</div>
		<h1 class="mt-3 text-3xl font-bold">{t('login.title')}</h1>
		<p class="text-white/60">{t('login.subtitle')}</p>
	</div>

	{#if session.info?.needs_setup}
		<a href="/register" class="mb-4 flex items-start gap-2 rounded-2xl bg-sun/15 p-4 text-sm text-sun">
			<Sparkles class="size-5 shrink-0" />
			<span>{t('login.setup')}</span>
		</a>
	{/if}

	<form class="card flex flex-col gap-4" onsubmit={submit}>
		<label>
			<span class="label">{t('login.username')}</span>
			<input class="input" bind:value={username} autocomplete="username" autocapitalize="none" required />
		</label>
		<label>
			<span class="label">{t('login.password')}</span>
			<input class="input" type="password" bind:value={password} autocomplete="current-password" required />
		</label>
		{#if error}<p class="rounded-xl bg-pink/15 px-3 py-2 text-sm font-bold text-pink">{error}</p>{/if}
		<button class="btn btn-pink min-h-13 text-lg" disabled={busy}>{busy ? t('common.moment') : t('login.submit')}</button>
	</form>

	{#if session.info?.allow_registration !== false}
		<p class="mt-5 text-center text-white/60">
			{t('login.wantDj')} <a class="font-bold text-mint" href="/register">{t('login.register')}</a>
		</p>
	{/if}
</div>
