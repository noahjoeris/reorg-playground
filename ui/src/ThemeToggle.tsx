import { Moon, Sun } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { type ThemePreference, useTheme } from './useTheme'

const LABELS: Record<ThemePreference, string> = {
  light: 'Use dark theme',
  dark: 'Use light theme',
}

export function ThemeToggle() {
  const { preference, cycle } = useTheme()

  return (
    <Button
      variant="ghost"
      size="icon-sm"
      onClick={cycle}
      aria-label={LABELS[preference]}
      title={LABELS[preference]}
      aria-pressed={preference === 'dark'}
    >
      {preference === 'dark' ? <Moon /> : <Sun />}
    </Button>
  )
}
