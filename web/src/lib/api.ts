export class ApiError extends Error {
	constructor(
		public status: number,
		message: string
	) {
		super(message);
	}
}

import { i18n, t } from './i18n.svelte.ts';

export async function api<T = unknown>(
	path: string,
	opts: { method?: string; body?: unknown } = {}
): Promise<T> {
	const res = await fetch(`/api${path}`, {
		method: opts.method ?? (opts.body !== undefined ? 'POST' : 'GET'),
		headers: {
			'X-Lang': i18n.lang,
			...(opts.body !== undefined ? { 'Content-Type': 'application/json' } : {})
		},
		body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
		credentials: 'same-origin'
	});
	const text = await res.text();
	let data: any = null;
	try {
		data = text ? JSON.parse(text) : null;
	} catch {
		data = { error: text };
	}
	if (!res.ok) throw new ApiError(res.status, data?.error ?? t('common.error', { code: res.status }));
	return data as T;
}
