<script lang="ts">
	import { loadSession } from '#lib/session.svelte.ts';
	// Neschválený účet nemá živé spojení → stav účtu kontrolujeme každých pár sekund.
	$effect(() => {
		const t = setInterval(loadSession, 5000);
		return () => clearInterval(t);
	});
</script>
