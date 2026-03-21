import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import useSWR from "swr";
import { openPeerChangesStream } from "../services/peerChangeEventService";
import {
	addNode,
	disconnectNode,
	fetchPeerInfo,
	type PeerInfo,
	type PeerInfoResponse,
} from "../services/peerConnectionManagerService";
import type { ConnectionStatus, Network } from "../types";
import { useNotification } from "./useNotification";

type PeerInfoKey = readonly ["peer-info", number];

const PEER_INFO_REFRESH_INTERVAL_MS = 60_000;
const REFRESH_DEBOUNCE_MS = 150;
const PENDING_POLL_INTERVAL_MS = 3_000;
const PENDING_TIMEOUT_MS = 30_000;
const EVENT_PEER_CHANGED = "peer_changed";
const EVENT_RESYNC_REQUIRED = "resync_required";

export type PendingConnect = {
	id: number;
	nodeId: number;
	nodeName: string;
	targetNodeId: number;
	targetName: string;
	createdAt: number;
};

type PendingDisconnect = {
	id: number;
	nodeId: number;
	nodeName: string;
	peerId: number;
	peerLabel: string;
	address: string;
	matchedNodeId: number | null;
	createdAt: number;
};

type PendingResolution<T> = {
	remaining: T[];
	completed: T[];
	timedOut: T[];
};

type PendingMutationIndexes = {
	pendingConnectionPairKeys: Set<string>;
	disconnectingPeerKeys: Set<string>;
	disconnectingConnectionPairKeys: Set<string>;
};

/** Stable key for a node-to-node relationship regardless of connect direction. */
function getNodePairKey(nodeId: number, relatedNodeId: number): string {
	return [nodeId, relatedNodeId].sort((a, b) => a - b).join(":");
}

function connectionStatusFromReadyState(readyState: number): ConnectionStatus {
	if (readyState === EventSource.CONNECTING) return "connecting";
	if (readyState === EventSource.CLOSED) return "closed";
	return "error";
}

function peerConnectionsForNode(
	peerData: PeerInfoResponse,
	nodeId: number,
): PeerInfo[] {
	return peerData.nodes.find((node) => node.node_id === nodeId)?.peers ?? [];
}

/**
 * Disconnect completion can show up under a different peer id or address when the backend tears
 * down both sides of an in-app node-to-node link. We therefore match by the related node id first,
 * then fall back to the original peer id/address.
 */
function peerMatchesPendingDisconnect(
	peer: PeerInfo,
	pendingDisconnect: PendingDisconnect,
): boolean {
	if (
		pendingDisconnect.matchedNodeId != null &&
		peer.matched_node_id === pendingDisconnect.matchedNodeId
	) {
		return true;
	}

	return (
		peer.id === pendingDisconnect.peerId ||
		peer.addr === pendingDisconnect.address
	);
}

function resolvePendingConnects(
	peerData: PeerInfoResponse,
	pendingConnects: PendingConnect[],
	now: number,
): PendingResolution<PendingConnect> {
	const completed: PendingConnect[] = [];
	const timedOut: PendingConnect[] = [];

	const remaining = pendingConnects.filter((pendingConnect) => {
		const nodePeers = peerConnectionsForNode(peerData, pendingConnect.nodeId);
		if (
			nodePeers.some(
				(peer) => peer.matched_node_id === pendingConnect.targetNodeId,
			)
		) {
			completed.push(pendingConnect);
			return false;
		}

		if (now - pendingConnect.createdAt >= PENDING_TIMEOUT_MS) {
			timedOut.push(pendingConnect);
			return false;
		}

		return true;
	});

	return { remaining, completed, timedOut };
}

function resolvePendingDisconnects(
	peerData: PeerInfoResponse,
	pendingDisconnects: PendingDisconnect[],
	now: number,
): PendingResolution<PendingDisconnect> {
	const completed: PendingDisconnect[] = [];
	const timedOut: PendingDisconnect[] = [];

	const remaining = pendingDisconnects.filter((pendingDisconnect) => {
		const nodePeers = peerConnectionsForNode(
			peerData,
			pendingDisconnect.nodeId,
		);
		const stillConnected = nodePeers.some((peer) =>
			peerMatchesPendingDisconnect(peer, pendingDisconnect),
		);

		if (!stillConnected) {
			completed.push(pendingDisconnect);
			return false;
		}

		if (now - pendingDisconnect.createdAt >= PENDING_TIMEOUT_MS) {
			timedOut.push(pendingDisconnect);
			return false;
		}

		return true;
	});

	return { remaining, completed, timedOut };
}

function buildPendingMutationIndexes(
	pendingConnects: PendingConnect[],
	pendingDisconnects: PendingDisconnect[],
): PendingMutationIndexes {
	const pendingConnectionPairKeys = new Set(
		pendingConnects.map((pendingConnect) =>
			getNodePairKey(pendingConnect.nodeId, pendingConnect.targetNodeId),
		),
	);
	const disconnectingPeerKeys = new Set(
		pendingDisconnects.map(
			(pendingDisconnect) =>
				`${pendingDisconnect.nodeId}:${pendingDisconnect.peerId}`,
		),
	);
	const disconnectingConnectionPairKeys = new Set(
		pendingDisconnects.flatMap((pendingDisconnect) =>
			pendingDisconnect.matchedNodeId == null
				? []
				: [
						getNodePairKey(
							pendingDisconnect.nodeId,
							pendingDisconnect.matchedNodeId,
						),
					],
		),
	);

	return {
		pendingConnectionPairKeys,
		disconnectingPeerKeys,
		disconnectingConnectionPairKeys,
	};
}

function hasPendingPeerMutations(
	pendingConnects: PendingConnect[],
	pendingDisconnects: PendingDisconnect[],
): boolean {
	return pendingConnects.length > 0 || pendingDisconnects.length > 0;
}

export function usePeerConnectionManager(network: Network | null) {
	const networkId = network?.id ?? null;
	const { notifyError, notifySuccess } = useNotification();
	const [pendingConnects, setPendingConnects] = useState<PendingConnect[]>([]);
	const [pendingDisconnects, setPendingDisconnects] = useState<
		PendingDisconnect[]
	>([]);
	const nextPendingIdRef = useRef(1);
	const scheduledRefreshRef = useRef<ReturnType<typeof setTimeout> | null>(
		null,
	);
	const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>(
		networkId == null ? "closed" : "connecting",
	);

	const {
		data: peerData,
		mutate: refreshPeerInfo,
		isLoading,
		isValidating,
		error,
	} = useSWR<PeerInfoResponse, Error, PeerInfoKey | null>(
		networkId == null ? null : ["peer-info", networkId],
		([, currentNetworkId]) => fetchPeerInfo(currentNetworkId),
		{
			revalidateOnFocus: true,
			revalidateOnReconnect: true,
			refreshInterval: PEER_INFO_REFRESH_INTERVAL_MS,
			keepPreviousData: true,
		},
	);

	const clearScheduledPeerInfoRefresh = useCallback(() => {
		if (scheduledRefreshRef.current == null) {
			return;
		}

		clearTimeout(scheduledRefreshRef.current);
		scheduledRefreshRef.current = null;
	}, []);

	const schedulePeerInfoRefresh = useCallback(() => {
		clearScheduledPeerInfoRefresh();
		scheduledRefreshRef.current = setTimeout(() => {
			scheduledRefreshRef.current = null;
			void refreshPeerInfo();
		}, REFRESH_DEBOUNCE_MS);
	}, [clearScheduledPeerInfoRefresh, refreshPeerInfo]);

	useEffect(() => {
		if (
			!peerData ||
			!hasPendingPeerMutations(pendingConnects, pendingDisconnects)
		) {
			return;
		}

		const now = Date.now();
		const pendingConnectResolution = resolvePendingConnects(
			peerData,
			pendingConnects,
			now,
		);
		const pendingDisconnectResolution = resolvePendingDisconnects(
			peerData,
			pendingDisconnects,
			now,
		);

		if (
			pendingConnectResolution.completed.length > 0 ||
			pendingConnectResolution.timedOut.length > 0
		) {
			setPendingConnects(pendingConnectResolution.remaining);
		}

		if (
			pendingDisconnectResolution.completed.length > 0 ||
			pendingDisconnectResolution.timedOut.length > 0
		) {
			setPendingDisconnects(pendingDisconnectResolution.remaining);
		}

		for (const pendingConnect of pendingConnectResolution.completed) {
			notifySuccess({
				title: "Peer connected",
				description: `${pendingConnect.nodeName} connected to ${pendingConnect.targetName}.`,
			});
		}

		for (const pendingConnect of pendingConnectResolution.timedOut) {
			notifyError({
				title: "Peer connection timed out",
				description: `Could not confirm that ${pendingConnect.nodeName} connected to ${pendingConnect.targetName}.`,
			});
		}

		for (const pendingDisconnect of pendingDisconnectResolution.completed) {
			notifySuccess({
				title: "Peer disconnected",
				description: `${pendingDisconnect.nodeName} disconnected from ${pendingDisconnect.peerLabel}.`,
			});
		}

		for (const pendingDisconnect of pendingDisconnectResolution.timedOut) {
			notifyError({
				title: "Peer disconnect timed out",
				description: `Could not confirm that ${pendingDisconnect.nodeName} disconnected from ${pendingDisconnect.peerLabel}.`,
			});
		}
	}, [
		peerData,
		pendingConnects,
		pendingDisconnects,
		notifyError,
		notifySuccess,
	]);

	useEffect(() => {
		setPendingConnects([]);
		setPendingDisconnects([]);
		clearScheduledPeerInfoRefresh();

		if (networkId == null) {
			setConnectionStatus("closed");
			return;
		}

		setConnectionStatus("connecting");

		const eventSource = openPeerChangesStream(networkId);
		const handlePeerChange = () => {
			schedulePeerInfoRefresh();
		};

		eventSource.onopen = () => {
			setConnectionStatus("connected");
		};

		eventSource.onerror = () => {
			setConnectionStatus(
				connectionStatusFromReadyState(eventSource.readyState),
			);
		};

		eventSource.addEventListener(EVENT_PEER_CHANGED, handlePeerChange);
		eventSource.addEventListener(EVENT_RESYNC_REQUIRED, handlePeerChange);

		return () => {
			eventSource.removeEventListener(EVENT_PEER_CHANGED, handlePeerChange);
			eventSource.removeEventListener(EVENT_RESYNC_REQUIRED, handlePeerChange);
			eventSource.close();
			clearScheduledPeerInfoRefresh();
		};
	}, [networkId, schedulePeerInfoRefresh, clearScheduledPeerInfoRefresh]);

	const hasPendingMutations = hasPendingPeerMutations(
		pendingConnects,
		pendingDisconnects,
	);

	useEffect(() => {
		if (!hasPendingMutations) {
			return;
		}

		const intervalId = setInterval(() => {
			void refreshPeerInfo();
		}, PENDING_POLL_INTERVAL_MS);

		return () => {
			clearInterval(intervalId);
		};
	}, [hasPendingMutations, refreshPeerInfo]);

	const {
		pendingConnectionPairKeys,
		disconnectingPeerKeys,
		disconnectingConnectionPairKeys,
	} = useMemo(
		() => buildPendingMutationIndexes(pendingConnects, pendingDisconnects),
		[pendingConnects, pendingDisconnects],
	);

	const createPendingId = useCallback(() => {
		const pendingId = nextPendingIdRef.current;
		nextPendingIdRef.current += 1;
		return pendingId;
	}, []);

	const connectNodePair = useCallback(
		async (
			nodeId: number,
			nodeName: string,
			targetNodeId: number,
			targetLabel: string,
			address: string,
		) => {
			if (!network) {
				return false;
			}

			const nodePairKey = getNodePairKey(nodeId, targetNodeId);
			if (
				pendingConnectionPairKeys.has(nodePairKey) ||
				disconnectingConnectionPairKeys.has(nodePairKey)
			) {
				return false;
			}

			const pendingConnect: PendingConnect = {
				id: createPendingId(),
				nodeId,
				nodeName,
				targetNodeId,
				targetName: targetLabel,
				createdAt: Date.now(),
			};
			setPendingConnects((current) => [...current, pendingConnect]);

			try {
				const result = await addNode(network.id, nodeId, address);
				if (!result.success) {
					throw new Error(result.error ?? "Unknown error");
				}

				await refreshPeerInfo();
				return true;
			} catch (error) {
				setPendingConnects((current) =>
					current.filter((item) => item.id !== pendingConnect.id),
				);
				notifyError({
					title: "Could not add peer",
					description: error instanceof Error ? error.message : "Network error",
				});
				return false;
			}
		},
		[
			createPendingId,
			disconnectingConnectionPairKeys,
			network,
			notifyError,
			pendingConnectionPairKeys,
			refreshPeerInfo,
		],
	);

	const disconnectPeer = useCallback(
		async (
			nodeId: number,
			nodeName: string,
			peerLabel: string,
			address: string,
			peerId: number,
			addnodeRemoveAddresses: string[],
			counterpartyNodeId: number | null,
			localListenAddressCandidates: string[],
			matchedNodeId: number | null,
		) => {
			if (!network) {
				return false;
			}

			const disconnectKey = `${nodeId}:${peerId}`;
			const disconnectPairKey =
				matchedNodeId == null ? null : getNodePairKey(nodeId, matchedNodeId);
			if (
				disconnectingPeerKeys.has(disconnectKey) ||
				(disconnectPairKey != null &&
					disconnectingConnectionPairKeys.has(disconnectPairKey))
			) {
				return false;
			}

			const pendingDisconnect: PendingDisconnect = {
				id: createPendingId(),
				nodeId,
				nodeName,
				peerId,
				peerLabel,
				address,
				matchedNodeId,
				createdAt: Date.now(),
			};
			setPendingDisconnects((current) => [...current, pendingDisconnect]);

			try {
				const result = await disconnectNode(
					network.id,
					nodeId,
					address,
					peerId,
					addnodeRemoveAddresses,
					counterpartyNodeId,
					localListenAddressCandidates,
				);
				if (!result.success) {
					throw new Error(result.error ?? "Unknown error");
				}

				await refreshPeerInfo();
				return true;
			} catch (error) {
				setPendingDisconnects((current) =>
					current.filter((item) => item.id !== pendingDisconnect.id),
				);
				notifyError({
					title: "Could not disconnect peer",
					description: error instanceof Error ? error.message : "Network error",
				});
				return false;
			}
		},
		[
			createPendingId,
			disconnectingConnectionPairKeys,
			disconnectingPeerKeys,
			network,
			notifyError,
			refreshPeerInfo,
		],
	);

	return {
		peerData,
		isLoading,
		isRefreshing: isValidating && !!peerData,
		error: error?.message ?? null,
		connectionStatus,
		pendingConnects,
		disconnectingPeers: disconnectingPeerKeys,
		disconnectingConnectionPairs: disconnectingConnectionPairKeys,
		addNode: connectNodePair,
		disconnectNode: disconnectPeer,
	};
}
