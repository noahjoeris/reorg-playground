export const SWR_KEY_NETWORKS = 'networks' as const

export function getNetworkSnapshotKey(networkId: number) {
  return ['network-snapshot', networkId] as const
}

export type NetworkSnapshotKey = ReturnType<typeof getNetworkSnapshotKey>
