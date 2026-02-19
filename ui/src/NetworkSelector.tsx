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
    <select
      className="rounded border border-border bg-background px-2 py-1 text-sm text-foreground"
      value={selectedId ?? ''}
      onChange={e => onChange(Number(e.target.value))}
    >
      {networks.map(n => (
        <option key={n.id} value={n.id}>
          {n.name}
        </option>
      ))}
    </select>
  )
}
