import { useEffect, useMemo, useState } from 'react';
import { ThemeContext } from './theme';

const applyTheme = (theme) => {
  const root = document.documentElement;
  if (theme === 'dark') {
    root.classList.add('dark');
    root.setAttribute('data-theme', 'dark');
  } else {
    root.classList.remove('dark');
    root.setAttribute('data-theme', 'light');
  }
};

export const ThemeProvider = ({ children }) => {
  const [theme, setThemeState] = useState('light');

  useEffect(() => {
    (() => {
      try {
        const stored = localStorage.getItem('theme');
        const prefersDark = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches;
        const initial = stored || (prefersDark ? 'dark' : 'light');
        setThemeState(initial);
        applyTheme(initial);
      } catch { /* ignore */ }
    })();
  }, []);

  const setTheme = (next) => {
    setThemeState(next);
    try {
      localStorage.setItem('theme', next);
    } catch { /* ignore */ }
    applyTheme(next);
  };

  const value = useMemo(() => ({ theme, setTheme }), [theme]);

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
};


