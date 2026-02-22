import { useEffect, useState } from 'react'
import { fetchNetworks } from '../api'
import type { Network } from '../types'

export function useNetworks() {
  const [networks, setNetworks] = useState<Network[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    fetchNetworks()
      .then(data => {
        if (!cancelled) {
          setNetworks(data)
          setLoading(false)
        }
      })
      .catch(err => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err))
          setLoading(false)
        }
      })
    return () => {
      cancelled = true
    }
  }, [])

  return { networks, loading, error }
}
