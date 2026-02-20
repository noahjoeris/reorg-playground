import { type ThemePreference, useTheme } from './useTheme'

const ICON_PROPS = {
  width: 16,
  height: 16,
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 2,
  strokeLinecap: 'round' as const,
  strokeLinejoin: 'round' as const,
}

function SunIcon() {
  return (
    <svg {...ICON_PROPS} aria-hidden="true">
      <circle cx="12" cy="12" r="5" />
      <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
    </svg>
  )
}

function MoonIcon() {
  return (
    <svg {...ICON_PROPS} aria-hidden="true">
      <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
    </svg>
  )
}

const LABELS: Record<ThemePreference, string> = {
  light: 'Use dark theme',
  dark: 'Use light theme',
}

export function ThemeToggle() {
  const { preference, cycle } = useTheme()
  const Icon = preference === 'dark' ? MoonIcon : SunIcon

  return (
    <button
      type="button"
      onClick={cycle}
      className="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:ring-offset-1 focus-visible:ring-offset-background"
      aria-label={LABELS[preference]}
      title={LABELS[preference]}
      aria-pressed={preference === 'dark'}
    >
      <Icon />
    </button>
  )
}
