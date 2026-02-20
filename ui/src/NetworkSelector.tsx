import type { Network } from './types'

export function NetworkSelector({
  networks,
  selectedId,
  onChange,
}: {
  networks: Network[]
  selectedId: number | null
  onChange: (id: number) => void
}) {
  return (
    <div className="flex items-center gap-2">
      <label htmlFor="network-selector" className="shrink-0 text-xs font-medium text-muted-foreground">
        Network
      </label>
      <div className="relative">
        <select
          id="network-selector"
          className="appearance-none rounded-md border border-border bg-background py-1.5 pl-3 pr-8 text-sm font-medium text-foreground transition-colors hover:border-muted-foreground/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:ring-offset-1 focus-visible:ring-offset-background"
          value={selectedId ?? ''}
          onChange={event => onChange(Number(event.target.value))}
          aria-label="Select network"
        >
          {networks.map(network => (
            <option key={network.id} value={network.id}>
              {network.name}
            </option>
          ))}
        </select>
        <span className="pointer-events-none absolute inset-y-0 right-2.5 grid place-items-center text-xs text-muted-foreground" aria-hidden="true">
          ▾
        </span>
      </div>
    </div>
  )
}
