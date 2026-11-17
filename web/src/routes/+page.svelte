<script lang="ts">
	import { Headphones, Hourglass } from '@lucide/svelte';
	import { session } from '#lib/session.svelte.ts';
	import { t } from '#lib/i18n.svelte.ts';
	import NowPlaying from '#lib/components/NowPlaying.svelte';
	import Queue from './Queue.svelte';
	import Poll from './Poll.svelte';
</script>

<div class="flex flex-col gap-4">
	<NowPlaying />

	{#if !session.user}
		<a href="/register" class="card flex items-center gap-3 bg-gradient-to-br from-pink/25 to-lilac/20 ring-pink/30">
			<div class="grid size-12 shrink-0 place-items-center rounded-2xl bg-pink text-on-accent">
				<Headphones class="size-6" strokeWidth={2.5} />
			</div>
			<div class="min-w-0">
				<div class="font-display text-lg font-bold">{t('home.ctaTitle')}</div>
				<div class="text-sm text-white/70">{t('home.ctaText')}</div>
			</div>
		</a>
	{:else if session.user.status === 'pending'}
		<div class="card flex items-center gap-3 ring-sun/30">
			<div class="grid size-12 shrink-0 animate-wiggle place-items-center rounded-2xl bg-sun text-on-accent">
				<Hourglass class="size-6" strokeWidth={2.5} />
			</div>
			<div class="text-sm text-white/80">
				<b class="font-display text-base text-white">{t('home.pendingTitle')}</b>
				{t('home.pendingText')}
			</div>
		</div>
		<Poll />
	{/if}

	<Queue />
</div>
