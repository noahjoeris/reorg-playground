import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { type ProcessedBlock, TIP_STATUS_COLORS, type TipStatus } from './types'
import { formatMinerLabel, shortHash } from './utils'

const STATUS_LABELS: Record<TipStatus, string> = {
  active: 'Active',
  invalid: 'Invalid',
  'valid-fork': 'Valid Fork',
  'valid-headers': 'Valid Headers',
  'headers-only': 'Headers Only',
  unknown: 'Unknown',
}

function toHex(n: number): string {
  return `0x${n.toString(16)}`
}

function formatBlockTime(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleString()
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-border/80 bg-muted/40 p-3">
      <p className="text-[11px] uppercase tracking-wide text-muted-foreground">{label}</p>
      <p className="mt-1 font-mono text-xs text-foreground">{value}</p>
    </div>
  )
}

function CopyableField({
  id,
  label,
  value,
  copied,
  onCopy,
}: {
  id: string
  label: string
  value: string
  copied: boolean
  onCopy: (id: string, value: string) => void
}) {
  return (
    <div className="rounded-lg border border-border/80 bg-muted/30 p-3">
      <div className="mb-1.5 flex items-center justify-between gap-3">
        <p className="text-[11px] uppercase tracking-wide text-muted-foreground">{label}</p>
        <Button variant="outline" size="xs" onClick={() => onCopy(id, value)} aria-label={`Copy ${label}`}>
          {copied ? 'Copied' : 'Copy'}
        </Button>
      </div>
      <p className="break-all font-mono text-xs text-foreground">{value}</p>
    </div>
  )
}

export function BlockDetailPanel({ block, onClose }: { block: ProcessedBlock; onClose: () => void }) {
  const [copiedField, setCopiedField] = useState<string | null>(null)

  const copyToClipboard = async (fieldId: string, value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      setCopiedField(fieldId)
      window.setTimeout(() => {
        setCopiedField(currentField => (currentField === fieldId ? null : currentField))
      }, 1200)
    } catch {
      setCopiedField(null)
    }
  }

  return (
    <Sheet
      open
      onOpenChange={open => {
        if (!open) onClose()
      }}
    >
      <SheetContent side="right" className="w-full overflow-y-auto sm:max-w-2xl">
        <SheetHeader className="border-b border-border/80 pb-3">
          <SheetTitle className="text-base sm:text-lg">Height #{block.height}</SheetTitle>
          <SheetDescription className="truncate font-mono" title={block.hash}>
            {shortHash(block.hash, 14, 12)}
          </SheetDescription>
        </SheetHeader>

        <div className="flex-1 space-y-4 p-4">
          <section className="rounded-xl border border-border/80 bg-muted/25 p-4">
            <div className="grid gap-3 sm:grid-cols-3">
              <Metric label="Timestamp" value={formatBlockTime(block.time)} />
              <Metric label="Difficulty" value={String(block.difficulty_int)} />
              <Metric label="Miner" value={formatMinerLabel(block.miner)} />
            </div>
          </section>

          <section className="space-y-3">
            <CopyableField
              id="hash"
              label="Hash"
              value={block.hash}
              copied={copiedField === 'hash'}
              onCopy={copyToClipboard}
            />
            <CopyableField
              id="prev_blockhash"
              label="Previous blockhash"
              value={block.prev_blockhash}
              copied={copiedField === 'prev_blockhash'}
              onCopy={copyToClipboard}
            />
            <CopyableField
              id="merkle_root"
              label="Merkle root"
              value={block.merkle_root}
              copied={copiedField === 'merkle_root'}
              onCopy={copyToClipboard}
            />
          </section>

          <section className="grid gap-3 sm:grid-cols-2">
            <Metric label="Version" value={toHex(block.version)} />
            <Metric label="Nonce" value={toHex(block.nonce)} />
            <Metric label="Bits" value={toHex(block.bits)} />
            <Metric label="Node ID" value={String(block.id)} />
          </section>

          {block.tipStatuses.length > 0 && (
            <section className="space-y-2 rounded-xl border border-border/80 bg-muted/20 p-4">
              <p className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">Tip status</p>
              <ul className="space-y-2">
                {block.tipStatuses.map(tipStatus => (
                  <li
                    key={tipStatus.status}
                    className="flex items-start gap-2 rounded-lg border border-border/80 bg-background/80 px-3 py-2"
                  >
                    <span
                      className="mt-1 h-2 w-2 shrink-0 rounded-full"
                      style={{ backgroundColor: TIP_STATUS_COLORS[tipStatus.status] }}
                    />
                    <div className="min-w-0">
                      <p className="text-sm font-medium text-foreground">{STATUS_LABELS[tipStatus.status]}</p>
                      <p className="wrap-break-word text-xs text-muted-foreground">{tipStatus.nodeNames.join(', ')}</p>
                    </div>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </div>
      </SheetContent>
    </Sheet>
  )
}
