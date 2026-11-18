export type Role = 'guest' | 'dj' | 'admin';
export type UserStatus = 'pending' | 'active' | 'banned';

export interface User {
	id: number;
	username: string;
	display_name: string;
	role: Role;
	status: UserStatus;
	created_at: number;
	avatar_url: string | null;
}

export interface Track {
	id: number;
	source: 'youtube' | 'local';
	source_id: string;
	title: string;
	artist: string;
	album: string | null;
	duration_sec: number;
	artwork_url: string | null;
}

export interface QueueEntry {
	id: number;
	track: Track;
	added_by: { id: number; name: string; avatar_url: string | null } | null;
	priority: boolean;
	created_at: number;
}

export interface NowPlaying {
	entry: QueueEntry;
	position_ms: number;
	paused: boolean;
	loading: boolean;
}

export type Corner = 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right';
export type Side = 'left' | 'right';
export type Size = 'small' | 'medium' | 'large';

export interface DisplaySettings {
	qr_enabled: boolean;
	qr_position: Corner;
	qr_size: Size;
	qr_caption: string;
	queue_side: Side;
	queue_length: number;
	show_added_by: boolean;
	effects: Record<string, boolean>;
	show_logo: boolean;
	logo_position: Corner;
	logo_size: Size;
	logo_rev: number;
}

export interface StateSnapshot {
	now_playing: NowPlaying | null;
	queue: QueueEntry[];
	history: QueueEntry[];
	volume: number;
	join_url: string;
	box_name: string;
	display: DisplaySettings;
	theme_css: string;
	effects: string[];
	fair_queue: boolean;
	/** Právě platná zóna hlasitosti: [název, hlasitost]. */
	volume_zone: [string, number] | null;
	radio: RadioNow | null;
	language: string;
	server_time_ms: number;
}

export interface RadioStation {
	id: string;
	name: string;
	url: string;
	logo: string | null;
}

export interface RadioNow {
	station: string;
	station_logo: string | null;
	stream_title: string | null;
	title: string | null;
	artist: string | null;
	cover_url: string | null;
	loading: boolean;
	paused: boolean;
}

export interface Settings {
	box_name: string;
	theme: string;
	language: string;
	radio_stations: RadioStation[];
	radio_station: string;
	fullscreen: boolean;
	display: DisplaySettings;
	public_url: string;
	local_folders: string[];
	idle_mode: 'silence' | 'random_local' | 'radio';
	max_queued_per_user: number;
	max_track_minutes: number;
	allow_registration: boolean;
	prevent_duplicates: boolean;
	volume: number;
	audio_device: string;
	audio_devices: string[];
	normalize_volume: boolean;
	dj_sources: DjSources;
	fair_queue: boolean;
	crossfade_sec: number;
	volume_zones: VolumeZone[];
}

export interface VolumeZone {
	name: string;
	from: string;
	to: string;
	volume: number;
	enabled: boolean;
}

export interface DjSources {
	youtube: boolean;
	links: boolean;
	local: boolean;
}

export interface AudioDevice {
	id: string;
	name: string;
	is_default: boolean;
}

export interface Album {
	key: string;
	name: string;
	artist: string;
	artwork_url: string | null;
	tracks: Track[];
}

export interface Library {
	albums: Album[];
	singles: Track[];
}

/** Návrh z MusicBrainz / Deezeru při identifikaci. */
export interface Candidate {
	source: 'musicbrainz' | 'deezer';
	id: string;
	title: string;
	artist: string;
	album: string | null;
	year: string | null;
	cover_url: string | null;
	track_count: number | null;
}

export interface ThemeInfo {
	id: string;
	name: string;
	description: string;
	description_en: string;
	author: string;
	builtin: boolean;
	version: string;
	colors: Record<'background' | 'surface' | 'surface_2' | 'surface_3' | 'text' | 'primary' | 'secondary' | 'success' | 'info' | 'extra', string>;
	fonts: { heading: string; body: string };
	effects: ThemeEffect[];
}

export interface ThemeEffect {
	id: string;
	name: string;
	name_en: string;
	animated: boolean;
	default: boolean;
}

/** URL loga music-boxu (vlastní nebo výchozí). */
export function logoUrl(rev: number | undefined, base = ''): string {
	return `${base}/api/logo?v=${rev ?? 0}`;
}

export function hasRole(user: User | null, role: Role): boolean {
	if (!user || user.status !== 'active') return false;
	const order: Role[] = ['guest', 'dj', 'admin'];
	return order.indexOf(user.role) >= order.indexOf(role);
}

export function fmtTime(sec: number): string {
	if (!sec || sec < 0) return '0:00';
	const s = Math.floor(sec);
	const h = Math.floor(s / 3600);
	const m = Math.floor((s % 3600) / 60);
	const r = String(s % 60).padStart(2, '0');
	return h ? `${h}:${String(m).padStart(2, '0')}:${r}` : `${m}:${r}`;
}

/** Hravá barva podle jména (pro avatary). */
export function nameColor(name: string): string {
	const colors = ['#ff5fa2', '#ffd23f', '#5ce1b9', '#6ec8ff', '#b69cff', '#ff9f5a'];
	let h = 0;
	for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
	return colors[h % colors.length];
}
