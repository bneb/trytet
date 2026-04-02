/**
 * theme.js
 * Trytet Native Frontend Theme Controller
 * 
 * Handles system detection, active listening, and persistence for Dark/Light mode.
 */

// Centralized theme setter to ensure DOM and generic storage are synced perfectly.
function applyTheme(theme) {
    document.documentElement.setAttribute('data-theme', theme);
    localStorage.setItem('trytet-theme', theme);
}

// 1. Initialize Toggle Button Listeners on Load
document.addEventListener('DOMContentLoaded', () => {
    const toggleContainer = document.getElementById('theme-toggle');
    if (!toggleContainer) return;

    toggleContainer.addEventListener('click', () => {
        const currentTheme = document.documentElement.getAttribute('data-theme');
        const newTheme = currentTheme === 'light' ? 'dark' : 'light';
        applyTheme(newTheme);
    });
});

// 2. Active OS System Theme Listening
const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');

mediaQuery.addEventListener('change', (e) => {
    // Only automatically apply system changes if the user hasn't forced a strict local override explicitly, 
    // or simply respect system overrides in real-time. We will prioritize system updates here:
    const newTheme = e.matches ? 'dark' : 'light';
    applyTheme(newTheme);
});
