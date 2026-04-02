"use client";

import React, { useEffect, useState } from 'react';
import Link from 'next/link';

export function Navbar() {
  const [isLightMode, setIsLightMode] = useState(false);

  useEffect(() => {
    // Check initial state from body class
    setIsLightMode(document.body.classList.contains('light-mode'));
  }, []);

  const toggleTheme = () => {
    document.body.classList.toggle('light-mode');
    setIsLightMode(!isLightMode);
  };

  return (
    <nav className="sticky top-0 z-[100] backdrop-blur-md bg-[var(--nav-bg)] px-5 md:px-10 py-5 flex justify-between items-center border-b border-[var(--card-border)] transition-colors duration-400">
      <Link href="/" className="trytet-logo-container">
          <div className="trytet-sigil">
              <div className="trytet-substrate"></div>
              <div className="trytet-core"></div>
          </div>
          <h1 className="trytet-wordmark">Trytet</h1>
      </Link>
      
      <div className="flex items-center gap-4 md:gap-8">
        <div 
          className="flex items-center gap-3 bg-[var(--card-border)] p-1 rounded-full cursor-pointer"
          onClick={toggleTheme}
          title="Switch Viewport Mode"
        >
          <div className="w-6 h-6 bg-[var(--btn-bg)] rounded-full flex items-center justify-center transition-transform duration-300 ease-[var(--ease)]"
               style={{ transform: isLightMode ? 'translateX(28px)' : 'translateX(0)' }}>
            {isLightMode ? (
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" className="text-[var(--btn-text)]">
                <circle cx="12" cy="12" r="5"></circle>
                <line x1="12" y1="1" x2="12" y2="3"></line>
                <line x1="12" y1="21" x2="12" y2="23"></line>
                <line x1="4.22" y1="4.22" x2="5.64" y2="5.64"></line>
                <line x1="18.36" y1="18.36" x2="19.78" y2="19.78"></line>
                <line x1="1" y1="12" x2="3" y2="12"></line>
                <line x1="21" y1="12" x2="23" y2="12"></line>
                <line x1="4.22" y1="19.78" x2="5.64" y2="18.36"></line>
                <line x1="18.36" y1="5.64" x2="19.78" y2="4.22"></line>
              </svg>
            ) : (
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3" strokeLinecap="round" strokeLinejoin="round" className="text-[var(--btn-text)]">
                <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"></path>
              </svg>
            )}
          </div>
          {/* Invisible spacer to reserve width during the slide */}
          <div className="w-6 h-6 shrink-0" />
        </div>
        <div className="flex items-center gap-1 md:gap-2">
          <Link href="/how-to" className="text-sm font-semibold tracking-wide text-[var(--text-sub)] px-3 py-1.5 rounded-md hover:bg-[var(--card-border)] hover:text-[var(--text-main)] transition-colors">
            How To
          </Link>
          <Link href="/web-demo" className="text-sm font-semibold tracking-wide text-[var(--text-sub)] px-3 py-1.5 rounded-md hover:bg-[var(--card-border)] hover:text-[var(--text-main)] transition-colors">
            Live Demo
          </Link>
          <a href="https://github.com/bneb/trytet" target="_blank" rel="noreferrer" className="text-sm font-semibold tracking-wide text-[var(--text-sub)] px-3 py-1.5 rounded-md hover:bg-[var(--card-border)] hover:text-[var(--text-main)] transition-colors">
            GitHub
          </a>
        </div>
      </div>
    </nav>
  );
}
