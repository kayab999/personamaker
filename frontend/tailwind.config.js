// Tailwind CSS runtime config (externalized for strict CSP — no inline <script>).
// Loaded after vendor/tailwind.js via <script src="tailwind.config.js">.
tailwind.config = {
    darkMode: 'class',
    theme: {
        extend: {
            fontFamily: { sans: ['Inter', 'system-ui', 'sans-serif'] },
            colors: {
                surface: { DEFAULT: '#1a1a2e', light: '#222240', lighter: '#2a2a45', hover: '#333358' },
                accent: { DEFAULT: '#7c3aed', light: '#a78bfa', glow: '#c4b5fd' },
                chat: { user: '#3b3b5c', ai: '#1e1e35' },
            }
        }
    }
};
