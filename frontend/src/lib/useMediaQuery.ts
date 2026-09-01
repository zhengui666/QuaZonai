import { useEffect, useState } from 'react';

/**
 * A single responsive seam for DOM changes that CSS cannot express.  The
 * server/test fallback is deliberately desktop so the hook is safe to render
 * without a browser and then corrects itself after hydration.
 */
export function useMediaQuery(query: string, defaultValue = false): boolean {
  const [matches, setMatches] = useState(() => (
    typeof window !== 'undefined' && typeof window.matchMedia === 'function'
      ? window.matchMedia(query).matches
      : defaultValue
  ));

  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return undefined;
    const media = window.matchMedia(query);
    const update = () => setMatches(media.matches);
    update();
    media.addEventListener?.('change', update);
    if (!media.addEventListener) media.addListener(update);
    return () => {
      media.removeEventListener?.('change', update);
      if (!media.removeEventListener) media.removeListener(update);
    };
  }, [query]);

  return matches;
}

export function useResponsiveViewport() {
  return {
    isPhone: useMediaQuery('(max-width: 780px)'),
    isCompact: useMediaQuery('(max-width: 1100px)'),
    isNarrowPhone: useMediaQuery('(max-width: 480px)'),
  };
}
