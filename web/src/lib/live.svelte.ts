import type { StateSnapshot } from './types';
import { toast } from './toasts.svelte';

type Listener = (type: string, data: any) => void;

/** Živý stav přes WebSocket (automaticky se znovu připojuje). */
class Live {
	state = $state<StateSnapshot | null>(null);
	connected = $state(false);
	/** Čas přijetí posledního stavu – pro dopočítání pozice skladby. */
	receivedAt = $state(0);

	#ws: WebSocket | null = null;
	#retry = 0;
	#timer: ReturnType<typeof setTimeout> | null = null;
	#listeners = new Set<Listener>();
	#wanted = false;

	connect() {
		this.#wanted = true;
		if (this.#ws) return;
		const proto = location.protocol === 'https:' ? 'wss' : 'ws';
		const ws = new WebSocket(`${proto}://${location.host}/api/ws`);
		this.#ws = ws;
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
				toast(msg.message, msg.level === 'error' ? 'error' : 'info');
			}
			this.#listeners.forEach((l) => l(msg.type, msg));
		};
		ws.onclose = () => {
			this.connected = false;
			this.#ws = null;
			if (!this.#wanted) return;
			const delay = Math.min(10000, 500 * 2 ** this.#retry++);
			this.#timer = setTimeout(() => this.connect(), delay);
		};
	}

	disconnect() {
		this.#wanted = false;
		if (this.#timer) clearTimeout(this.#timer);
		this.#ws?.close();
		this.#ws = null;
	}

	on(l: Listener) {
		this.#listeners.add(l);
		return () => this.#listeners.delete(l);
	}
}

export const live = new Live();

/** Aktuální pozice skladby v sekundách (dopočítaná od posledního stavu). */
export function positionSec(now: number): number {
	const np = live.state?.now_playing;
	if (!np) return 0;
	const extra = np.paused || np.loading ? 0 : now - live.receivedAt;
	return Math.min((np.position_ms + extra) / 1000, np.entry.track.duration_sec || Infinity);
}
