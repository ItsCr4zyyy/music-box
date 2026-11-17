<script lang="ts">
	import { Key, KeyRound } from '@lucide/svelte';
	// Efekt „Cinkání klíčů“ – svazky klíčů visí shora a cinkají jako na náměstích v listopadu 1989.
	const bunches = [
		{ left: 3, len: 15, delay: 0, dur: 1.1 },
		{ left: 34, len: 6, delay: 0.35, dur: 0.9 },
		{ left: 45, len: 11, delay: 0.15, dur: 1.25 },
		{ left: 57, len: 4, delay: 0.5, dur: 1 }
	];
</script>

<div class="mb-fx mb-keys pointer-events-none absolute inset-x-0 top-0 h-[22vh] overflow-visible">
	{#each bunches as b, i (i)}
		<div
			class="absolute top-0 flex flex-col items-center"
			style="left: {b.left}%; transform-origin: top center; animation: mb-jingle {b.dur}s ease-in-out {b.delay}s infinite"
		>
			<div class="w-[2px] bg-white/30" style="height: {b.len}vh"></div>
			<div class="size-[1.4em] rounded-full border-[0.18em] border-white/50"></div>
			<div class="-mt-[0.4em] flex">
				<span class="origin-top -rotate-[18deg] text-white/60" style="animation: mb-jingle {b.dur * 0.7}s ease-in-out {b.delay}s infinite"><KeyRound class="size-[2.2em]" strokeWidth={2} /></span>
				<span class="origin-top rotate-[14deg] text-pink/80" style="animation: mb-jingle {b.dur * 0.8}s ease-in-out {b.delay + 0.1}s infinite reverse"><Key class="size-[2em]" strokeWidth={2} /></span>
			</div>
		</div>
	{/each}
</div>

<style>
	@keyframes -global-mb-jingle {
		0%,
		100% {
			transform: rotate(-9deg);
		}
		50% {
			transform: rotate(9deg);
		}
	}
</style>
