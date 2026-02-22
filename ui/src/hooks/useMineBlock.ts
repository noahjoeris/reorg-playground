import { useCallback, useState } from 'react'
import { mineBlock } from '../api'

export function useMineBlock() {
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const mine = useCallback(async (networkId: number, nodeId: number) => {
    setLoading(true)
    setError(null)
    try {
      const result = await mineBlock(networkId, nodeId)
      if (!result.success) {
        setError(result.error ?? 'Unknown error')
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Network error')
    } finally {
      setLoading(false)
    }
  }, [])

  const clearError = useCallback(() => setError(null), [])

  return { mine, loading, error, clearError }
}
