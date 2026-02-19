import type { ProcessedBlock } from './types'
import { TIP_STATUS_COLORS } from './types'

function toHex(n: number): string {
  return `0x${n.toString(16)}`
}

function CopyableField({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span
        className="cursor-pointer break-all font-mono text-xs text-foreground hover:text-accent"
        title="Click to copy"
        onClick={() => navigator.clipboard.writeText(value)}
      >
        {value}
      </span>
    </div>
  )
}

export function BlockDetailPanel({ block, onClose }: { block: ProcessedBlock; onClose: () => void }) {
  return (
    <div className="fixed right-0 top-0 z-50 flex h-full w-96 flex-col border-l border-border bg-background shadow-lg">
      <div className="flex items-center justify-between border-b border-border px-4 py-3">
        <h2 className="text-sm font-semibold text-foreground">Block #{block.height}</h2>
        <button className="text-muted-foreground hover:text-foreground" onClick={onClose}>
          &times;
        </button>
      </div>

      <div className="flex flex-1 flex-col gap-3 overflow-y-auto p-4">
        <CopyableField label="Hash" value={block.hash} />
        <CopyableField label="Previous Blockhash" value={block.prev_blockhash} />
        <CopyableField label="Merkle Root" value={block.merkle_root} />

        <div className="grid grid-cols-2 gap-3">
          <div className="flex flex-col gap-0.5">
            <span className="text-xs text-muted-foreground">Height</span>
            <span className="font-mono text-xs text-foreground">{block.height}</span>
          </div>
          <div className="flex flex-col gap-0.5">
            <span className="text-xs text-muted-foreground">Time</span>
            <span className="font-mono text-xs text-foreground">{new Date(block.time * 1000).toLocaleString()}</span>
          </div>
          <div className="flex flex-col gap-0.5">
            <span className="text-xs text-muted-foreground">Version</span>
            <span className="font-mono text-xs text-foreground">{toHex(block.version)}</span>
          </div>
          <div className="flex flex-col gap-0.5">
            <span className="text-xs text-muted-foreground">Nonce</span>
            <span className="font-mono text-xs text-foreground">{toHex(block.nonce)}</span>
          </div>
          <div className="flex flex-col gap-0.5">
            <span className="text-xs text-muted-foreground">Bits</span>
            <span className="font-mono text-xs text-foreground">{toHex(block.bits)}</span>
          </div>
          <div className="flex flex-col gap-0.5">
            <span className="text-xs text-muted-foreground">Difficulty</span>
            <span className="font-mono text-xs text-foreground">{block.difficulty_int}</span>
          </div>
        </div>

        {block.miner && (
          <div className="flex flex-col gap-0.5">
            <span className="text-xs text-muted-foreground">Miner</span>
            <span className="text-xs text-foreground">{block.miner}</span>
          </div>
        )}

        {block.tipStatuses.length > 0 && (
          <div className="flex flex-col gap-2">
            <span className="text-xs font-semibold text-muted-foreground">Tip Status</span>
            {block.tipStatuses.map(ts => (
              <div key={ts.status} className="flex items-center gap-2">
                <span className="h-2.5 w-2.5 rounded-full" style={{ backgroundColor: TIP_STATUS_COLORS[ts.status] }} />
                <span className="text-xs text-foreground">
                  {ts.status}: {ts.nodeNames.join(', ')}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
