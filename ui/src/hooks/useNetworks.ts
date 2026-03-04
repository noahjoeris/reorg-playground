import useSWR from 'swr'
import { fetchNetworks } from '../services/networksService'
import { SWR_KEY_NETWORKS } from '../services/swrKeys'
import type { Network } from '../types'

export function useNetworks() {
  const { data, error, isLoading } = useSWR<Network[], Error>(SWR_KEY_NETWORKS, () => fetchNetworks(), {
    revalidateOnFocus: false,
    revalidateOnReconnect: false,
  })

  return {
    networks: data ?? [],
    loading: isLoading,
    error: error?.message ?? null,
  }
}
