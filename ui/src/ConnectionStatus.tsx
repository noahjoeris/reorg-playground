import type { ConnectionStatus as Status } from './types'

const CONFIG: Record<Status, { color: string; label: string }> = {
  connecting: { color: 'bg-amber-500', label: 'Connecting' },
  connected: { color: 'bg-emerald-500', label: 'Live' },
  error: { color: 'bg-red-500', label: 'Error' },
  closed: { color: 'bg-muted-foreground', label: 'Disconnected' },
}

export function ConnectionStatus({ status }: { status: Status }) {
  const { color, label } = CONFIG[status]

  return (
    <span className="inline-flex items-center gap-1.5" role="status" aria-label={`Connection status: ${label}`}>
      <span className={`h-1.5 w-1.5 rounded-full ${color}`} />
      <span className="text-xs text-muted-foreground">{label}</span>
    </span>
  )
}
