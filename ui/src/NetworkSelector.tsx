import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
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
      <label className="shrink-0 text-xs font-medium text-muted-foreground">Network</label>
      <Select value={selectedId !== null ? String(selectedId) : undefined} onValueChange={v => onChange(Number(v))}>
        <SelectTrigger size="sm" aria-label="Select network">
          <SelectValue placeholder="Select network" />
        </SelectTrigger>
        <SelectContent>
          {networks.map(network => (
            <SelectItem key={network.id} value={String(network.id)}>
              {network.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}
