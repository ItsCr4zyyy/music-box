export interface Toast {
	id: number;
	level: 'info' | 'error' | 'success';
	message: string;
}

let nextId = 1;
export const toasts = $state<Toast[]>([]);

export function toast(message: string, level: Toast['level'] = 'info') {
	const id = nextId++;
	toasts.push({ id, level, message });
	setTimeout(() => {
		const i = toasts.findIndex((t) => t.id === id);
		if (i >= 0) toasts.splice(i, 1);
	}, level === 'error' ? 6000 : 3500);
}

export function toastError(e: unknown) {
	toast(e instanceof Error ? e.message : String(e), 'error');
}
