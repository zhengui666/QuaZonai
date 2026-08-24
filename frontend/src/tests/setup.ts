import '@testing-library/jest-dom/vitest';
import { afterEach, vi } from 'vitest';
import { cleanup } from '@testing-library/react';
afterEach(() => cleanup());
class ResizeObserverMock { observe() {} unobserve() {} disconnect() {} }
Object.defineProperty(globalThis, 'ResizeObserver', { value: ResizeObserverMock, writable: true });
Object.defineProperty(window, 'matchMedia', { writable: true, value: vi.fn().mockImplementation((query: string) => ({ matches: false, media: query, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() })) });
Object.defineProperty(globalThis, 'crypto', { value: { ...globalThis.crypto, randomUUID: () => '00000000-0000-4000-8000-000000000001' } });
