import { api, ApiError } from './api.ts';
import type { DjSources, User } from './types.ts';

interface Info {
	needs_setup: boolean;
	allow_registration: boolean;
	version: string;
	box_name: string;
	language: string;
	dj_sources: DjSources;
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

/** Odkud smí přihlášený uživatel přidávat (admin vždy všechno). */
export function allowedSources(): DjSources {
	if (session.user?.role === 'admin') return { youtube: true, links: true, local: true };
	return session.info?.dj_sources ?? { youtube: true, links: true, local: true };
}

export async function logout() {
	await api('/auth/logout', { method: 'POST' }).catch(() => {});
	session.user = null;
}
