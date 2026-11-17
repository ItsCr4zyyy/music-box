// Texty displeje. Jazyk určuje nastavení music-boxu (Admin → Displej → Jazyk).
const cs = {
	preparing: 'Připravuji písničku…',
	paused: 'Pozastaveno',
	playing: 'Teď hraje',
	wishFrom: 'přání od',
	pickedByBox: 'vybral music-box',
	silence: 'Ticho…',
	scanQr: 'Naskenuj QR kód a pusť první písničku!',
	upNext: 'Další na řadě',
	andMore: '…a dalších {n}',
	queueEmpty: 'Fronta je prázdná',
	connecting: 'připojuji…',
	radioConnecting: 'Ladím rádio…',
	radioOnAir: 'Teď v rádiu',
	radioHint: 'Ve frontě nic není – přidej písničku přes QR kód',
	next: 'Další'
};

const en: typeof cs = {
	preparing: 'Preparing the song…',
	paused: 'Paused',
	playing: 'Now playing',
	wishFrom: 'requested by',
	pickedByBox: 'picked by music-box',
	silence: 'Silence…',
	scanQr: 'Scan the QR code and play the first song!',
	upNext: 'Up next',
	andMore: '…and {n} more',
	queueEmpty: 'The queue is empty',
	connecting: 'connecting…',
	radioConnecting: 'Tuning in…',
	radioOnAir: 'On the radio',
	radioHint: 'The queue is empty – add a song via the QR code',
	next: 'Next'
};

export type Key = keyof typeof cs;

export function translate(lang: string | undefined, key: Key, params?: Record<string, string | number>): string {
	let s = (lang === 'en' ? en : cs)[key];
	if (params) for (const [k, v] of Object.entries(params)) s = s.replaceAll(`{${k}}`, String(v));
	return s;
}
