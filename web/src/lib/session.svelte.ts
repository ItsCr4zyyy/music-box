import { api, ApiError } from './api.ts';
import type { User } from './types.ts';

interface Info {
	needs_setup: boolean;
	allow_registration: boolean;
	version: string;
	box_name: string;
	language: string;
}

export const session = $state<{ user: User | null; info: Info | null; loaded: boolean }>({
	user: null,
	info: null,
	loaded: false
});

export async function loadSession() {
	try {
		session.info = await api<Info>('/info');
	} catch {
		/* backend neběží – stránky ukážou chybu samy */
	}
	try {
		session.user = (await api<{ user: User }>('/me')).user;
	} catch (e) {
		if (e instanceof ApiError && (e.status === 401 || e.status === 403)) session.user = null;
	}
	session.loaded = true;
}

export async function logout() {
	await api('/auth/logout', { method: 'POST' }).catch(() => {});
	session.user = null;
}
