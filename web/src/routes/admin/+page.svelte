<script lang="ts">
	import { t as tr } from '#lib/i18n.svelte.ts';
	import { Monitor, Palette, SlidersHorizontal, Users as UsersIcon, Wrench } from '@lucide/svelte';
	import { goto } from '$app/navigation';
	import { session } from '#lib/session.svelte.ts';
	import { hasRole } from '#lib/types.ts';
	import Users from './Users.svelte';
	import DisplayForm from './DisplayForm.svelte';
	import SettingsForm from './SettingsForm.svelte';
	import System from './System.svelte';
	import ThemesForm from './ThemesForm.svelte';

	const tabs = [
		{ id: 'users', label: 'admin.tab.users', icon: UsersIcon },
		{ id: 'display', label: 'admin.tab.display', icon: Monitor },
		{ id: 'themes', label: 'admin.tab.themes', icon: Palette },
		{ id: 'settings', label: 'admin.tab.rules', icon: SlidersHorizontal },
		{ id: 'system', label: 'admin.tab.system', icon: Wrench }
	] as const;
	let tab = $state<(typeof tabs)[number]['id']>('users');

	$effect(() => {
		if (session.loaded && !hasRole(session.user, 'admin')) goto('/');
	});
</script>

{#if hasRole(session.user, 'admin')}
	<div class="sticky top-[60px] z-20 -mx-4 mb-3 bg-ink/85 px-4 pb-2 backdrop-blur">
		<div class="grid grid-cols-5 gap-1 rounded-2xl bg-ink-2 p-1">
			{#each tabs as item (item.id)}
				<button
					class="flex flex-col items-center gap-0.5 rounded-xl py-2 text-[11px] font-bold transition {tab === item.id ? 'bg-pink text-on-accent' : 'text-white/60'}"
					onclick={() => (tab = item.id)}
				>
					<item.icon class="size-5" strokeWidth={2.5} />
					{tr(item.label)}
				</button>
			{/each}
		</div>
	</div>
	{#if tab === 'users'}
		<Users />
	{:else if tab === 'display'}
		<DisplayForm />
	{:else if tab === 'themes'}
		<ThemesForm />
	{:else if tab === 'settings'}
		<SettingsForm />
	{:else}
		<System />
	{/if}
{/if}
