import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

// Displej music-boxu – běží uvnitř Tauri okna.
export default defineConfig({
	plugins: [tailwindcss(), svelte()],
	clearScreen: false,
	server: { port: 1420, strictPort: true, fs: { allow: ['..'] } }
});
