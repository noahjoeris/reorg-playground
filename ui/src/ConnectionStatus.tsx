import { Badge } from '@/components/ui/badge'
import { Spinner } from '@/components/ui/spinner'
import type { ConnectionStatus as Status } from './types'

const CONFIG: Record<Status, { dotClassName: string; label: string }> = {
  connecting: { dotClassName: 'bg-warning', label: 'Connecting' },
  connected: { dotClassName: 'bg-success', label: 'Live' },
  error: { dotClassName: 'bg-destructive', label: 'Error' },
  closed: { dotClassName: 'bg-muted-foreground', label: 'Disconnected' },
}

export function ConnectionStatus({ status }: { status: Status }) {
  const { dotClassName, label } = CONFIG[status]

  return (
    <Badge variant="outline" className="gap-1.5 font-normal" role="status" aria-label={`Connection status: ${label}`}>
      {status === 'connecting' ? (
        <Spinner className="size-3 text-warning" />
      ) : (
        <span className={`h-1.5 w-1.5 rounded-full ${dotClassName}`} />
      )}
      {label}
    </Badge>
  )
}
