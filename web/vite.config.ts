import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

// Backend (Rust) při vývoji – `cargo run -p musicbox-core --bin musicbox-headless` nebo Tauri aplikace.
const backend = process.env.MUSICBOX_BACKEND ?? 'http://127.0.0.1:8787';

export default defineConfig({
	plugins: [
		tailwindcss(),
		sveltekit({
			compilerOptions: {
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			// Web je SPA, kterou servíruje Rust server (fallback 200.html).
			adapter: adapter({ fallback: '200.html' })
		})
	],
	server: {
		host: true,
		proxy: {
			'/api': { target: backend, ws: true }
		}
	}
});
