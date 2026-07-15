import { Droplets } from 'lucide-react'
import { useMemo, useState } from 'react'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Spinner } from '@/components/ui/spinner'
import { useFaucet } from '@/hooks/useFaucet'
import { cn } from '@/utils'
import type { FaucetResponse, Network, NodeInfo } from './types'

type FaucetButtonProps = {
  network: Network
  nodes: NodeInfo[]
  buttonClassName?: string
}

const DEFAULT_FAUCET_AMOUNT_BTC = '0.1'

function defaultNodeId(nodes: NodeInfo[]) {
  return nodes.length > 0 ? String(nodes[0].id) : ''
}

export function FaucetButton({ network, nodes, buttonClassName }: FaucetButtonProps) {
  const eligibleNodes = useMemo(() => nodes.filter(node => node.supports_controls && node.supports_mining), [nodes])
  const { send, loading } = useFaucet(network)
  const [dialogOpen, setDialogOpen] = useState(false)
  const [nodeId, setNodeId] = useState('')
  const [address, setAddress] = useState('')
  const [amountBtc, setAmountBtc] = useState(DEFAULT_FAUCET_AMOUNT_BTC)
  const [result, setResult] = useState<FaucetResponse | null>(null)

  if (network.view_only_mode || network.network_type !== 'Regtest' || eligibleNodes.length === 0) {
    return null
  }

  const selectedNode = eligibleNodes.find(node => String(node.id) === nodeId) ?? null
  const canSubmit = selectedNode !== null && address.trim().length > 0 && amountBtc.trim().length > 0 && !loading

  const openDialog = () => {
    if (!nodeId) setNodeId(defaultNodeId(eligibleNodes))
    setDialogOpen(true)
  }

  const resetDialogState = () => {
    setNodeId(defaultNodeId(eligibleNodes))
    setAddress('')
    setAmountBtc(DEFAULT_FAUCET_AMOUNT_BTC)
    setResult(null)
  }

  const handleOpenChange = (open: boolean) => {
    if (!open && !loading) {
      resetDialogState()
    }
    setDialogOpen(open)
  }

  const handleSubmit = async () => {
    if (!selectedNode) return
    try {
      const faucetResult = await send(selectedNode.id, selectedNode.name, address.trim(), amountBtc.trim())
      setResult(faucetResult)
    } catch {
      setResult(null)
    }
  }

  return (
    <>
      <Button variant="outline" size="xs" className={cn('gap-1.5', buttonClassName)} onClick={openDialog}>
        <Droplets className="size-3.5" />
        Faucet
      </Button>

      <Dialog open={dialogOpen} onOpenChange={handleOpenChange}>
        <DialogContent
          showCloseButton={!loading}
          onClick={(e: React.MouseEvent) => e.stopPropagation()}
          onPointerDownOutside={loading ? e => e.preventDefault() : undefined}
          onEscapeKeyDown={loading ? e => e.preventDefault() : undefined}
        >
          <DialogHeader>
            <DialogTitle>Regtest Faucet</DialogTitle>
            <DialogDescription>
              Send an unconfirmed wallet transaction from a selected regtest node without mining a confirmation block.
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-4 pt-1">
            <div className="space-y-2">
              <label className="text-sm" htmlFor="faucet-node">
                Source Node
              </label>
              <Select value={nodeId} onValueChange={setNodeId} disabled={loading || result?.success}>
                <SelectTrigger className="w-full" id="faucet-node">
                  <SelectValue placeholder="Select a node" />
                </SelectTrigger>
                <SelectContent>
                  {eligibleNodes.map(node => (
                    <SelectItem key={node.id} value={String(node.id)}>
                      {node.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className="space-y-2">
              <label className="text-sm" htmlFor="faucet-address">
                Destination Address
              </label>
              <Input
                id="faucet-address"
                value={address}
                onChange={e => setAddress(e.target.value)}
                placeholder="bcrt1..."
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
                disabled={loading || result?.success}
              />
            </div>

            <div className="space-y-2">
              <label className="text-sm" htmlFor="faucet-amount">
                Amount (BTC)
              </label>
              <Input
                id="faucet-amount"
                value={amountBtc}
                onChange={e => setAmountBtc(e.target.value)}
                placeholder={DEFAULT_FAUCET_AMOUNT_BTC}
                inputMode="decimal"
                disabled={loading || result?.success}
              />
            </div>

            {result?.success && (
              <div className="space-y-2 rounded-md border border-success/40 bg-success/10 p-3 text-sm text-success">
                <p>Broadcast an unconfirmed transaction successfully.</p>
                <p>
                  {result.mined_blocks && result.mined_blocks > 0
                    ? `The faucet mined ${result.mined_blocks} refill block${result.mined_blocks === 1 ? '' : 's'} first.`
                    : 'No refill mining was needed.'}
                </p>
                {result.txid && <p className="break-all font-mono text-xs text-current/90">Txid: {result.txid}</p>}
              </div>
            )}
          </div>

          <DialogFooter>
            {result?.success ? (
              <>
                <Button variant="outline" onClick={resetDialogState}>
                  Send Another
                </Button>
                <Button variant="outline" onClick={() => handleOpenChange(false)}>
                  Close
                </Button>
              </>
            ) : (
              <Button onClick={() => void handleSubmit()} disabled={!canSubmit}>
                {loading ? <Spinner className="size-3" /> : null}
                Send Transaction
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
