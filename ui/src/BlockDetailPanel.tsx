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

function SectionLabel({ children }: { children: string }) {
  return <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">{children}</p>
}

function FieldRow({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="grid grid-cols-[7.25rem_minmax(0,1fr)] items-start gap-3 py-2.5 sm:grid-cols-[9rem_minmax(0,1fr)]">
      <dt className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">{label}</dt>
      <dd className={['wrap-break-word text-foreground', mono ? 'font-mono text-xs' : 'text-sm'].join(' ')}>{value}</dd>
    </div>
  )
}

function CopyableRow({
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
    <div className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-3 py-3">
      <div className="min-w-0">
        <p className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">{label}</p>
        <p className="mt-1 break-all font-mono text-xs text-foreground">{value}</p>
      </div>
      <Button
        variant="outline"
        size="xs"
        className="mt-[2px] bg-background/70"
        onClick={() => onCopy(id, value)}
        aria-label={`Copy ${label}`}
      >
        {copied ? 'Copied' : 'Copy'}
      </Button>
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
      <SheetContent
        side="right"
        className="w-full overflow-y-auto border-l border-accent/25 [background:var(--surface-panel-strong)] shadow-(--elevation-lift) backdrop-blur-md sm:max-w-2xl"
      >
        <SheetHeader className="border-b border-border/80 pb-3">
          <SectionLabel>Block Detail</SectionLabel>
          <SheetTitle className="text-base tracking-tight sm:text-lg">Height #{block.height}</SheetTitle>
          <SheetDescription className="truncate font-mono text-xs" title={block.hash}>
            {shortHash(block.hash, 14, 12)}
          </SheetDescription>
        </SheetHeader>

        <div className="flex-1 space-y-6 p-4">
          <section className="space-y-2">
            <SectionLabel>Summary</SectionLabel>
            <dl className="divide-y divide-border/60 border-y border-border/60">
              <FieldRow label="Timestamp" value={formatBlockTime(block.time)} />
              <FieldRow label="Miner" value={formatMinerLabel(block.miner)} />
              <FieldRow label="Difficulty" value={String(block.difficulty_int)} mono />
              <FieldRow label="Node ID" value={String(block.id)} mono />
            </dl>
          </section>

          <section className="space-y-3">
            <SectionLabel>Block Hashes</SectionLabel>
            <div className="divide-y divide-border/60 border-y border-border/60">
              <CopyableRow
                id="hash"
                label="Hash"
                value={block.hash}
                copied={copiedField === 'hash'}
                onCopy={copyToClipboard}
              />
              <CopyableRow
                id="prev_blockhash"
                label="Previous blockhash"
                value={block.prev_blockhash}
                copied={copiedField === 'prev_blockhash'}
                onCopy={copyToClipboard}
              />
              <CopyableRow
                id="merkle_root"
                label="Merkle root"
                value={block.merkle_root}
                copied={copiedField === 'merkle_root'}
                onCopy={copyToClipboard}
              />
            </div>
          </section>

          <section className="space-y-2">
            <SectionLabel>Header Fields</SectionLabel>
            <dl className="divide-y divide-border/60 border-y border-border/60">
              <FieldRow label="Version" value={toHex(block.version)} mono />
              <FieldRow label="Nonce" value={toHex(block.nonce)} mono />
              <FieldRow label="Bits" value={toHex(block.bits)} mono />
            </dl>
          </section>

          {block.tipStatuses.length > 0 && (
            <section className="space-y-2">
              <SectionLabel>Tip Status</SectionLabel>
              <ul className="divide-y divide-border/60 border-y border-border/60">
                {block.tipStatuses.map(tipStatus => (
                  <li key={tipStatus.status} className="flex items-start gap-2 py-2.5">
                    <span
                      className="mt-1 h-2 w-2 shrink-0 rounded-full ring-1 ring-background/70"
                      style={{ backgroundColor: TIP_STATUS_COLORS[tipStatus.status] }}
                    />
                    <div className="min-w-0">
                      <p className="text-sm font-semibold text-foreground">{STATUS_LABELS[tipStatus.status]}</p>
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
