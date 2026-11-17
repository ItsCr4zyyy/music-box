<script lang="ts">
	let {
		checked = $bindable(false),
		label,
		hint,
		onchange
	}: { checked: boolean; label: string; hint?: string; onchange?: (value: boolean) => void } = $props();
</script>

<label class="flex cursor-pointer items-center gap-3">
	<span class="min-w-0 flex-1">
		<span class="block font-bold">{label}</span>
		{#if hint}<span class="block text-sm text-white/50">{hint}</span>{/if}
	</span>
	<input type="checkbox" class="peer sr-only" bind:checked onchange={() => onchange?.(checked)} />
	<span class="relative h-8 w-14 shrink-0 rounded-full bg-ink-4 transition peer-checked:bg-mint after:absolute after:top-1 after:left-1 after:size-6 after:rounded-full after:bg-white after:transition peer-checked:after:translate-x-6"></span>
</label>
