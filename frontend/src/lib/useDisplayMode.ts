import { useMediaQuery } from './useMediaQuery';

export function useDisplayMode(): { isStandalone: boolean } {
  const mediaStandalone = useMediaQuery('(display-mode: standalone)');
  const iosStandalone = typeof navigator !== 'undefined'
    && 'standalone' in navigator
    && Boolean((navigator as Navigator & { standalone?: boolean }).standalone);
  return { isStandalone: mediaStandalone || iosStandalone };
}
