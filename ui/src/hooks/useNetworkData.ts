import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import useSWR from 'swr'
import { openCacheChangesStream } from '../services/cacheChangeEventService'
import { fetchNetworkSnapshot } from '../services/networkSnapshotService'
import { getNetworkSnapshotKey, type NetworkSnapshotKey } from '../services/swrKeys'
import type { ConnectionStatus, DataResponse } from '../types'

const REFRESH_DEBOUNCE_MS = 150
const EVENT_CACHE_CHANGED = 'cache_changed'
const EVENT_RESYNC_REQUIRED = 'resync_required'

type CacheChangedEvent = {
  network_id?: number
}

function mapEventSourceReadyStateToConnectionStatus(readyState: number): ConnectionStatus {
  if (readyState === EventSource.CONNECTING) return 'connecting'
  if (readyState === EventSource.CLOSED) return 'closed'
  return 'error'
}

function parseCacheChangedNetworkId(event: Event): number | null {
  const messageEvent = event as MessageEvent<string>
  try {
    const parsed = JSON.parse(messageEvent.data) as CacheChangedEvent
    return typeof parsed.network_id === 'number' ? parsed.network_id : null
  } catch {
    return null
  }
}

function fetchNetworkSnapshotByKey([, networkId]: NetworkSnapshotKey) {
  return fetchNetworkSnapshot(networkId)
}

export function useNetworkData(networkId: number | null) {
  const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>(
    networkId === null ? 'closed' : 'connecting',
  )
  const refreshTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const snapshotKey = useMemo(() => (networkId === null ? null : getNetworkSnapshotKey(networkId)), [networkId])

  const { data, error, isLoading, mutate } = useSWR<DataResponse, Error, NetworkSnapshotKey | null>(
    snapshotKey,
    fetchNetworkSnapshotByKey,
    {
      revalidateOnFocus: false,
      revalidateOnReconnect: false,
      keepPreviousData: false,
    },
  )

  const clearScheduledRefresh = useCallback(() => {
    if (refreshTimerRef.current) {
      clearTimeout(refreshTimerRef.current)
      refreshTimerRef.current = null
    }
  }, [])

  const scheduleRefresh = useCallback(() => {
    clearScheduledRefresh()
    refreshTimerRef.current = setTimeout(() => {
      void mutate()
    }, REFRESH_DEBOUNCE_MS)
  }, [clearScheduledRefresh, mutate])

  // Ensure timers are always cleaned up on unmount.
  useEffect(
    () => () => {
      clearScheduledRefresh()
    },
    [clearScheduledRefresh],
  )

  // Update connection status baseline when selected network changes.
  useEffect(() => {
    clearScheduledRefresh()

    if (networkId === null) {
      setConnectionStatus('closed')
      return
    }

    setConnectionStatus('connecting')
  }, [networkId, clearScheduledRefresh])

  // Subscribe to SSE invalidation events and schedule debounced snapshot refreshes.
  useEffect(() => {
    if (networkId === null) return

    const eventSource = openCacheChangesStream(networkId)

    const handleCacheChanged = (event: Event) => {
      const changedNetworkId = parseCacheChangedNetworkId(event)
      if (changedNetworkId !== null && changedNetworkId !== networkId) {
        return
      }
      scheduleRefresh()
    }

    const handleResyncRequired = () => {
      scheduleRefresh()
    }

    eventSource.onopen = () => setConnectionStatus('connected')
    eventSource.onerror = () => setConnectionStatus(mapEventSourceReadyStateToConnectionStatus(eventSource.readyState))

    eventSource.addEventListener(EVENT_CACHE_CHANGED, handleCacheChanged)
    eventSource.addEventListener(EVENT_RESYNC_REQUIRED, handleResyncRequired)

    return () => {
      eventSource.removeEventListener(EVENT_CACHE_CHANGED, handleCacheChanged)
      eventSource.removeEventListener(EVENT_RESYNC_REQUIRED, handleResyncRequired)
      eventSource.close()
      clearScheduledRefresh()
    }
  }, [networkId, scheduleRefresh, clearScheduledRefresh])

  return {
    data: networkId === null ? null : (data ?? null),
    loading: networkId === null ? false : isLoading,
    error: networkId === null ? null : (error?.message ?? null),
    connectionStatus,
  }
}
