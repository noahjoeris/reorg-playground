import type { ConnectionStatus as Status } from './types'

const STATUS_COLORS: Record<Status, string> = {
  connecting: 'bg-yellow-500',
  connected: 'bg-green-500',
  error: 'bg-red-500',
  closed: 'bg-gray-500',
}

const STATUS_LABELS: Record<Status, string> = {
  connecting: 'Connecting...',
  connected: 'Connected',
  error: 'Connection error',
  closed: 'Disconnected',
}

export function ConnectionStatus({ status }: { status: Status }) {
  return (
    <div className="flex items-center gap-1.5" title={STATUS_LABELS[status]}>
      <span
        className={`h-2.5 w-2.5 rounded-full ${STATUS_COLORS[status]} ${status === 'connected' ? 'animate-pulse' : ''}`}
      />
      <span className="text-xs text-muted-foreground">{STATUS_LABELS[status]}</span>
    </div>
  )
}
