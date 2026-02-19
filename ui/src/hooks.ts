import { useCallback, useEffect, useRef, useState } from 'react'
import { createChangesEventSource, fetchData, fetchNetworks } from './api'
import type { ConnectionStatus, DataResponse, Network } from './types'

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

export function useNetworkData(networkId: number | null) {
  const [data, setData] = useState<DataResponse | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>('connecting')
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const loadData = useCallback(
    (showLoading: boolean) => {
      if (networkId === null) return
      if (showLoading) setLoading(true)
      fetchData(networkId)
        .then(d => {
          setData(d)
          setError(null)
          setLoading(false)
        })
        .catch(err => {
          setError(err instanceof Error ? err.message : String(err))
          setLoading(false)
        })
    },
    [networkId],
  )

  // Initial fetch on network change
  useEffect(() => {
    if (networkId === null) return
    setData(null)
    loadData(true)
  }, [networkId, loadData])

  // SSE subscription with 500ms debounced refetch
  useEffect(() => {
    if (networkId === null) return

    setConnectionStatus('connecting')
    const es = createChangesEventSource()

    es.onopen = () => setConnectionStatus('connected')
    es.onerror = () => {
      setConnectionStatus(es.readyState === EventSource.CLOSED ? 'closed' : 'error')
    }

    es.addEventListener('cache_changed', (event: Event) => {
      const messageEvent = event as MessageEvent
      try {
        const parsed = JSON.parse(messageEvent.data)
        if (parsed.network_id !== networkId) return
      } catch {
        // Non-JSON event or no network filter — refetch anyway
      }

      if (debounceRef.current) clearTimeout(debounceRef.current)
      debounceRef.current = setTimeout(() => loadData(false), 500)
    })

    return () => {
      es.close()
      if (debounceRef.current) clearTimeout(debounceRef.current)
    }
  }, [networkId, loadData])

  return { data, loading, error, connectionStatus }
}
