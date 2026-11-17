import cs, { type Key } from './locales/cs.ts';
import en from './locales/en.ts';

export type Lang = 'cs' | 'en';
export const languages: { id: Lang; name: string }[] = [
	{ id: 'cs', name: 'Čeština' },
	{ id: 'en', name: 'English' }
];

const dicts: Record<Lang, Record<Key, string>> = { cs, en };
const STORAGE = 'mb-lang';

/** Jazyk webu: volba uživatele (localStorage), jinak výchozí jazyk music-boxu. */
export const i18n = $state<{ lang: Lang; userChoice: Lang | null; boxDefault: Lang }>({
	lang: 'cs',
	userChoice: null,
	boxDefault: 'cs'
});

function apply() {
	i18n.lang = i18n.userChoice ?? i18n.boxDefault;
	if (typeof document !== 'undefined') document.documentElement.lang = i18n.lang;
}

export function initLang() {
	const saved = typeof localStorage !== 'undefined' ? localStorage.getItem(STORAGE) : null;
	i18n.userChoice = saved === 'cs' || saved === 'en' ? saved : null;
	apply();
}

export function setBoxDefault(lang: string | undefined) {
	if (lang === 'cs' || lang === 'en') {
		i18n.boxDefault = lang;
		apply();
	}
}

export function setUserLang(lang: Lang | null) {
	i18n.userChoice = lang;
	if (lang) localStorage.setItem(STORAGE, lang);
	else localStorage.removeItem(STORAGE);
	apply();
}

/** Přeložený text; `{name}` v textu nahradí parametry. */
export function t(key: Key, params?: Record<string, string | number>): string {
	let s = dicts[i18n.lang][key] ?? cs[key] ?? key;
	if (params) for (const [k, v] of Object.entries(params)) s = s.replaceAll(`{${k}}`, String(v));
	return s;
}

/** Text z motivu: anglická varianta, když existuje a je zvolená angličtina. */
export function local(cs: string, en?: string | null): string {
	return i18n.lang === 'en' && en ? en : cs;
}

export function dateLocale() {
	return i18n.lang === 'en' ? 'en-GB' : 'cs-CZ';
}
