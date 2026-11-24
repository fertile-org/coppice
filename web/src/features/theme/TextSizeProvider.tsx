import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import {
  applyTextSize,
  readStoredTextSize,
  storeTextSize,
  type TextSize,
} from './textSize';

type TextSizeContextValue = {
  size: TextSize;
  setSize: (size: TextSize) => void;
};

const TextSizeContext = createContext<TextSizeContextValue | null>(null);

export function TextSizeProvider({ children }: { children: ReactNode }) {
  const [size, setSizeState] = useState<TextSize>(readStoredTextSize);

  useEffect(() => {
    applyTextSize(size);
    storeTextSize(size);
  }, [size]);

  const setSize = useCallback((next: TextSize) => {
    setSizeState(next);
  }, []);

  const value = useMemo(() => ({ size, setSize }), [size, setSize]);

  return (
    <TextSizeContext.Provider value={value}>{children}</TextSizeContext.Provider>
  );
}

export function useTextSize(): TextSizeContextValue {
  const ctx = useContext(TextSizeContext);
  if (!ctx) {
    throw new Error('useTextSize must be used within TextSizeProvider');
  }
  return ctx;
}
