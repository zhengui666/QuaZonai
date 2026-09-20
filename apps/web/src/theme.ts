import { useEffect, useLayoutEffect, useRef, useState } from 'react';

export type ColorTheme = 'light' | 'dark';
export const themeStorageKey = 'quazonai.theme';
const query = '(prefers-color-scheme: dark)';

export function resolveTheme(preference: unknown, prefersDark: boolean): ColorTheme {
  return preference === 'light' || preference === 'dark' ? preference : prefersDark ? 'dark' : 'light';
}
function storedTheme(): ColorTheme | undefined {
  try {
    const value = localStorage.getItem(themeStorageKey);
    return value === 'light' || value === 'dark' ? value : undefined;
  } catch { return undefined; }
}
export function useColorTheme(): [ColorTheme, () => void] {
  const preference = useRef(storedTheme());
  const [theme, setTheme] = useState<ColorTheme>(() => resolveTheme(preference.current, window.matchMedia(query).matches));
  useLayoutEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.colorScheme = theme;
    document.querySelector('meta[name="theme-color"]')?.setAttribute('content', theme === 'dark' ? '#141414' : '#f6f7fa');
  }, [theme]);
  useEffect(() => {
    const media = window.matchMedia(query);
    const followSystem = () => { if (preference.current === undefined) setTheme(resolveTheme(undefined, media.matches)); };
    const synchronize = (event: StorageEvent) => {
      if (event.key !== themeStorageKey && event.key !== null) return;
      preference.current = storedTheme();
      setTheme(resolveTheme(preference.current, media.matches));
    };
    followSystem();
    media.addEventListener('change', followSystem);
    window.addEventListener('storage', synchronize);
    return () => { media.removeEventListener('change', followSystem); window.removeEventListener('storage', synchronize); };
  }, []);
  function toggle() {
    const next = theme === 'dark' ? 'light' : 'dark';
    preference.current = next;
    setTheme(next);
    try { localStorage.setItem(themeStorageKey, next); } catch { /* Session-only preference when storage is unavailable. */ }
  }
  return [theme, toggle];
}
