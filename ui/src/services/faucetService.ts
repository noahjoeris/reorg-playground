import type { FaucetResponse } from '../types'

type FaucetRequest = {
  node_id: number
  address: string
  amount_btc: string
}

export async function sendFaucetTransaction(networkId: number, request: FaucetRequest): Promise<FaucetResponse> {
  const res = await fetch(`/api/${networkId}/faucet`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request),
  })
  return res.json()
}
