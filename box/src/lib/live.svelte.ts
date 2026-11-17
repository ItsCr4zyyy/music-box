import type { StateSnapshot } from './types';

/** Adresa backendu – v Tauri ji vrátí Rust, v prohlížeči výchozí port. */
async function backendUrl(): Promise<string> {
	if ('__TAURI_INTERNALS__' in window) {
		const { invoke } = await import('@tauri-apps/api/core');
		return invoke<string>('backend_url');
	}
	const fromQuery = new URLSearchParams(location.search).get('backend');
	return fromQuery ?? 'http://127.0.0.1:8787';
}

export interface Toast {
	id: number;
	level: string;
	message: string;
}

class Live {
	state = $state<StateSnapshot | null>(null);
	connected = $state(false);
	receivedAt = $state(0);
	base = $state('');
	toasts = $state<Toast[]>([]);
	#nextToast = 1;
	#retry = 0;

	async start() {
		this.base = await backendUrl();
		this.#connect();
	}

	#connect() {
		const ws = new WebSocket(this.base.replace(/^http/, 'ws') + '/api/ws');
		ws.onopen = () => {
			this.connected = true;
			this.#retry = 0;
		};
		ws.onmessage = (ev) => {
			const msg = JSON.parse(ev.data);
			if (msg.type === 'state') {
				this.state = msg.state;
				this.receivedAt = Date.now();
			} else if (msg.type === 'toast') {
				const id = this.#nextToast++;
				this.toasts.push({ id, level: msg.level, message: msg.message });
				setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), 8000);
			}
		};
		ws.onclose = () => {
			this.connected = false;
			setTimeout(() => this.#connect(), Math.min(5000, 300 * 2 ** this.#retry++));
		};
	}

	/** Relativní URL z backendu (např. obal lokální skladby) → absolutní. */
	url(u: string | null | undefined): string | null {
		if (!u) return null;
		return u.startsWith('/') ? this.base + u : u;
	}
}

export const live = new Live();

export function positionSec(now: number): number {
	const np = live.state?.now_playing;
	if (!np) return 0;
	const extra = np.paused || np.loading ? 0 : now - live.receivedAt;
	return Math.min((np.position_ms + extra) / 1000, np.entry.track.duration_sec || Infinity);
}
