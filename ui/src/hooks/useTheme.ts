import { useCallback, useEffect, useState } from 'react'

export type ThemePreference = 'light' | 'dark'

const STORAGE_KEY = 'theme'
const ATTR = 'data-theme'

function getInitialPreference(): ThemePreference {
  const stored = localStorage.getItem(STORAGE_KEY)
  if (stored === 'dark' || stored === 'light') return stored
  return document.documentElement.getAttribute(ATTR) === 'dark' ? 'dark' : 'light'
}

/** Manages light/dark via `data-theme` on `<html>`. Persists to localStorage. */
export function useTheme() {
  const [preference, setPreference] = useState<ThemePreference>(getInitialPreference)

  useEffect(() => {
    document.documentElement.setAttribute(ATTR, preference)
    localStorage.setItem(STORAGE_KEY, preference)
  }, [preference])

  const cycle = useCallback(() => {
    setPreference(prev => (prev === 'light' ? 'dark' : 'light'))
  }, [])

  return { preference, setPreference, cycle }
}
