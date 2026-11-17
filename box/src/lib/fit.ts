/**
 * Svelte akce: zmenšuje písmo, dokud se text nevejde (max. na `min` em).
 * Měří skutečnou šířku/výšku – funguje s jakýmkoli fontem motivu.
 */
export function fitText(node: HTMLElement, _key: unknown, max = 4.2, min = 2) {
	const run = () => {
		let size = max;
		node.style.fontSize = `${size}em`;
		// Výška má kvůli diakritice/dotahům písma vždy pár px navíc – přetečení řádku = víc než ~půl řádku.
		const overflows = () => {
			const px = parseFloat(getComputedStyle(node).fontSize) || 16;
			return node.scrollWidth > node.clientWidth + 1 || node.scrollHeight - node.clientHeight > px * 0.5;
		};
		while (size > min && overflows()) {
			size -= 0.2;
			node.style.fontSize = `${size}em`;
		}
	};
	run();
	document.fonts?.ready.then(run);
	const ro = new ResizeObserver(run);
	ro.observe(node.parentElement ?? node);
	return {
		update: () => run(),
		destroy: () => ro.disconnect()
	};
}
