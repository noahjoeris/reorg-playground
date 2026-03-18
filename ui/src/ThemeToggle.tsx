import { Moon, Sun } from 'lucide-react'
import { Button } from '@/components/ui/button'
import type { ThemePreference } from '@/hooks/useTheme'

const LABELS: Record<ThemePreference, string> = {
  light: 'Use dark theme',
  dark: 'Use light theme',
}

type ThemeToggleProps = {
  preference: ThemePreference
  onToggle: () => void
}

export function ThemeToggle({ preference, onToggle }: ThemeToggleProps) {
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      onClick={onToggle}
      aria-label={LABELS[preference]}
      title={LABELS[preference]}
      aria-pressed={preference === 'dark'}
    >
      {preference === 'dark' ? <Moon /> : <Sun />}
    </Button>
  )
}
