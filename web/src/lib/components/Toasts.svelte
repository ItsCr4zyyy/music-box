<script lang="ts">
	import { CircleAlert, CircleCheck, Info } from '@lucide/svelte';
	import { toasts } from '#lib/toasts.svelte.ts';
	import { fly } from 'svelte/transition';
</script>

<div class="pointer-events-none fixed inset-x-0 top-3 z-50 mx-auto flex max-w-[480px] flex-col items-center gap-2 px-4">
	{#each toasts as t (t.id)}
		<div
			transition:fly={{ y: -20, duration: 200 }}
			class="pointer-events-auto flex w-full items-start gap-2 rounded-2xl px-4 py-3 font-bold text-ink shadow-lg"
			class:bg-pink={t.level === 'error'}
			class:bg-mint={t.level === 'success'}
			class:bg-sky={t.level === 'info'}
		>
			{#if t.level === 'error'}<CircleAlert class="mt-0.5 size-5 shrink-0" strokeWidth={2.5} />
			{:else if t.level === 'success'}<CircleCheck class="mt-0.5 size-5 shrink-0" strokeWidth={2.5} />
			{:else}<Info class="mt-0.5 size-5 shrink-0" strokeWidth={2.5} />{/if}
			<span>{t.message}</span>
		</div>
	{/each}
</div>
