<script lang="ts">
	import { Music, Music2, Music3, Music4 } from '@lucide/svelte';

	// Poletující noty v pozadí, když hraje hudba.
	const icons = [Music, Music2, Music3, Music4];
	const colors = ['#ff5fa2', '#ffd23f', '#5ce1b9', '#6ec8ff', '#b69cff'];
	const notes = Array.from({ length: 14 }, (_, i) => ({
		icon: icons[i % icons.length],
		color: colors[i % colors.length],
		left: (i * 37) % 100,
		size: 1.5 + ((i * 7) % 5) * 0.5,
		duration: 14 + ((i * 5) % 10),
		delay: -((i * 3.3) % 20)
	}));
</script>

<div class="mb-fx mb-notes pointer-events-none absolute inset-0 overflow-hidden">
	{#each notes as n, i (i)}
		<span
			class="absolute -bottom-[2em] opacity-40"
			style="left: {n.left}%; width: {n.size}em; height: {n.size}em; color: {n.color}; animation: float-up {n.duration}s linear {n.delay}s infinite"
		>
			<n.icon class="size-full" strokeWidth={2.2} />
		</span>
	{/each}
</div>
