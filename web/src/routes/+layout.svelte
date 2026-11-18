<script lang="ts">
	import '../app.css';
	import favicon from '#lib/assets/favicon.svg';
	import { LibraryBig, ListMusic, LogIn, Search, Settings, User } from '@lucide/svelte';
	import Avatar from '#lib/components/Avatar.svelte';
	import Toasts from '#lib/components/Toasts.svelte';
	import { live } from '#lib/live.svelte.ts';
	import { allowedSources, loadSession, session } from '#lib/session.svelte.ts';
	import { initLang, setBoxDefault, t } from '#lib/i18n.svelte.ts';
	import { hasRole, logoUrl } from '#lib/types.ts';
	import { page } from '$app/state';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	initLang();
	loadSession();
	// Fronta je veřejná – živé spojení má každý.
	live.connect();

	// Výchozí jazyk webu = jazyk music-boxu (uživatel si ho může přepnout v Účtu).
	$effect(() => setBoxDefault(live.state?.language ?? session.info?.language));

	// Admin změnil roli / schválil → načíst znovu vlastní účet.
	$effect(() =>
		live.on((type) => {
			if (type === 'users' || type === 'settings') loadSession();
		})
	);

	const logo = $derived(live.state ? logoUrl(live.state.display.logo_rev) : favicon);
	const boxName = $derived(live.state?.box_name ?? session.info?.box_name ?? 'music-box');
	const tabs = $derived(
		[
			{ href: '/', label: t('nav.queue'), icon: ListMusic, show: true },
			{ href: '/add', label: t('nav.add'), icon: Search, show: hasRole(session.user, 'dj') },
			{ href: '/library', label: t('nav.library'), icon: LibraryBig, show: hasRole(session.user, 'dj') && allowedSources().local },
			{ href: '/admin', label: t('nav.admin'), icon: Settings, show: hasRole(session.user, 'admin') },
			session.user
				? { href: '/account', label: t('nav.account'), icon: User, show: true }
				: { href: '/login', label: t('nav.login'), icon: LogIn, show: true }
		].filter((x) => x.show)
	);

	function active(href: string) {
		const p = page.url.pathname;
		return href === '/' ? p === '/' : p.startsWith(href) || (href === '/login' && p === '/register');
	}
</script>

<svelte:head>
	<link rel="icon" href={logo} />
	<title>{boxName}</title>
	<!-- Motiv vzhledu (barvy, fonty, efekty) – načte se znovu při změně -->
	<link rel="stylesheet" href={live.state?.theme_css ?? '/api/theme.css'} />
</svelte:head>

<div class="mx-auto flex min-h-dvh max-w-[480px] flex-col">
	<header class="sticky top-0 z-30 flex items-center gap-2.5 bg-ink/85 px-4 py-3 backdrop-blur">
		<a href="/" class="flex min-w-0 items-center gap-2">
			<img src={logo} alt="" class="mb-logo-img mb-fx size-9 shrink-0 rounded-lg object-contain" />
			<span class="mb-brand truncate font-display text-xl font-bold">{boxName}</span>
		</a>
		<div class="flex-1"></div>
		{#if !live.connected}
			<span class="chip animate-pulse bg-pink/20 text-pink">{t('common.offline')}</span>
		{/if}
		{#if session.user}
			<a href="/account" class="shrink-0"><Avatar name={session.user.display_name} url={session.user.avatar_url} class="size-9 text-base" /></a>
		{/if}
	</header>

	<main class="flex-1 px-4 pt-1 pb-28">
		{#if session.loaded}
			{@render children()}
		{:else}
			<div class="grid place-items-center py-20 text-white/50">{t('common.loading')}</div>
		{/if}
	</main>
</div>

<nav class="fixed inset-x-0 bottom-0 z-40 mx-auto max-w-[480px] px-3 pb-[max(0.75rem,env(safe-area-inset-bottom))]">
	<div class="mb-panel flex rounded-3xl bg-ink-2/95 p-1.5 ring-1 ring-white/10 backdrop-blur" style="box-shadow: 0 -2px 20px rgb(0 0 0 / 0.25)">
		{#each tabs as tab (tab.href)}
			{@const on = active(tab.href)}
			<a
				href={tab.href}
				class="flex flex-1 flex-col items-center gap-0.5 rounded-2xl py-1.5 text-[11px] font-bold transition {on ? 'bg-pink text-on-accent' : 'text-white/60'}"
			>
				<tab.icon class="size-6" strokeWidth={on ? 2.6 : 2.2} />
				{tab.label}
			</a>
		{/each}
	</div>
</nav>

<Toasts />
