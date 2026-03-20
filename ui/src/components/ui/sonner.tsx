import { CircleCheckIcon, InfoIcon, OctagonXIcon, TriangleAlertIcon } from 'lucide-react'
import { type CSSProperties, useEffect, useState } from 'react'
import { Toaster as Sonner, type ToasterProps } from 'sonner'
import { Spinner } from '@/components/ui/spinner'

const THEME_ATTR = 'data-theme'

const TOASTER_STYLE = {
  '--normal-bg': 'var(--popover)',
  '--normal-text': 'var(--popover-foreground)',
  '--normal-border': 'var(--border)',
  '--border-radius': 'var(--radius)',
} as CSSProperties

function getDocumentTheme(): ToasterProps['theme'] {
  if (document.documentElement.getAttribute(THEME_ATTR) === 'dark') {
    return 'dark'
  }

  return 'light'
}

const TOAST_CLASS_NAMES: NonNullable<ToasterProps['toastOptions']>['classNames'] = {
  toast:
    'group toast group-[.toaster]:rounded-xl group-[.toaster]:border group-[.toaster]:px-4 group-[.toaster]:py-3 group-[.toaster]:shadow-(--elevation-lift) group-[.toaster]:backdrop-blur-md',
  title: 'text-sm font-semibold leading-5',
  description: 'text-xs leading-5 text-current/80',
  icon: 'mt-0.5',
  success: 'border-success/40 bg-success/12 text-success',
  error: 'border-destructive/40 bg-destructive/10 text-destructive',
  info: 'border-border/80 bg-popover text-popover-foreground',
  warning: 'border-warning/40 bg-warning/12 text-warning',
  loading: 'border-border/80 bg-popover text-popover-foreground',
  default: 'border-border/80 bg-popover text-popover-foreground',
  closeButton:
    'border-border/70 bg-background/70 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground',
}

function Toaster(props: ToasterProps) {
  const [theme, setTheme] = useState<ToasterProps['theme']>(getDocumentTheme)

  useEffect(() => {
    const root = document.documentElement
    const observer = new MutationObserver(() => {
      setTheme(getDocumentTheme())
    })

    observer.observe(root, { attributes: true, attributeFilter: [THEME_ATTR] })

    return () => observer.disconnect()
  }, [])

  return (
    <Sonner
      theme={theme}
      position="bottom-center"
      className="toaster group"
      icons={{
        success: <CircleCheckIcon className="size-4" />,
        info: <InfoIcon className="size-4" />,
        warning: <TriangleAlertIcon className="size-4" />,
        error: <OctagonXIcon className="size-4" />,
        loading: <Spinner className="size-4" />,
      }}
      style={TOASTER_STYLE}
      toastOptions={{
        classNames: TOAST_CLASS_NAMES,
      }}
      {...props}
    />
  )
}

export { Toaster }
