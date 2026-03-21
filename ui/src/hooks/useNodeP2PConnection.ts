import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import useSWR from "swr";
import useSWRMutation from "swr/mutation";
import {
	fetchNodeP2PState,
	type NodeP2PStateResponse,
	setNodeP2PConnectionActive,
} from "../services/nodeP2PConnectionService";
import type { Network, NodeInfo, SetNodeP2PConnectionResponse } from "../types";
import { useNotification } from "./useNotification";

type LoadingByNodeId = Record<number, boolean>;
type IsEnabledByNodeId = Record<number, boolean>;
type WaitingForReconnectByNodeId = Record<number, boolean>;
type NodeP2PStateKey = readonly ["node-p2p-state", number];
type SetNodeP2PConnectionArgs = {
	networkId: number;
	nodeId: number;
	active: boolean;
};

const SET_NODE_P2P_CONNECTION_MUTATION_KEY = "set-node-p2p-connection";
const P2P_STATE_REFRESH_INTERVAL_MS = 30_000;
const P2P_RECONNECT_WAIT_MS = 30_000;

function getP2PStateErrorToastId(networkId: number | null) {
	return networkId === null ? undefined : `p2p-state-error:${networkId}`;
}

export function useNodeP2PConnection(
	network: Network | null,
	nodes: NodeInfo[] = [],
) {
	const { notifyError, notifySuccess, dismissNotification } = useNotification();
	const [loadingByNodeId, setLoadingByNodeId] = useState<LoadingByNodeId>({});
	const [waitingForReconnectByNodeId, setWaitingForReconnectByNodeId] =
		useState<WaitingForReconnectByNodeId>({});
	const reconnectWaitTimeoutsRef = useRef<
		Record<number, ReturnType<typeof window.setTimeout>>
	>({});
	const { trigger: triggerSetNodeP2PConnection } = useSWRMutation<
		SetNodeP2PConnectionResponse,
		Error,
		string,
		SetNodeP2PConnectionArgs
	>(SET_NODE_P2P_CONNECTION_MUTATION_KEY, async (_key, { arg }) => {
		const result = await setNodeP2PConnectionActive(
			arg.networkId,
			arg.nodeId,
			arg.active,
		);
		if (!result.success) {
			throw new Error(result.error ?? "Unknown error");
		}
		return result;
	});
	const isEnabledByNodeId = useMemo(() => {
		const map: IsEnabledByNodeId = {};
		for (const node of nodes) {
			map[node.id] = node.supports_controls;
		}
		return map;
	}, [nodes]);
	const isFeatureEnabled = Object.values(isEnabledByNodeId).some(Boolean);
	const { data: p2pStateData, mutate: revalidateP2PState } = useSWR<
		NodeP2PStateResponse,
		Error,
		NodeP2PStateKey | null
	>(
		network && isFeatureEnabled ? ["node-p2p-state", network.id] : null,
		([, networkId]) => fetchNodeP2PState(networkId),
		{
			revalidateOnFocus: true,
			revalidateOnReconnect: true,
			refreshInterval: P2P_STATE_REFRESH_INTERVAL_MS,
			keepPreviousData: false,
			onError: (currentError) => {
				notifyError({
					id: getP2PStateErrorToastId(network?.id ?? null),
					title: "Could not refresh P2P state",
					description: currentError.message,
				});
			},
			onSuccess: () => {
				dismissNotification(getP2PStateErrorToastId(network?.id ?? null));
			},
		},
	);

	useEffect(() => {
		for (const timeoutId of Object.values(reconnectWaitTimeoutsRef.current)) {
			window.clearTimeout(timeoutId);
		}
		reconnectWaitTimeoutsRef.current = {};
		setLoadingByNodeId({});
		setWaitingForReconnectByNodeId({});
		dismissNotification(getP2PStateErrorToastId(network?.id ?? null));
		return () => {
			for (const timeoutId of Object.values(reconnectWaitTimeoutsRef.current)) {
				window.clearTimeout(timeoutId);
			}
			reconnectWaitTimeoutsRef.current = {};
		};
	}, [network?.id, dismissNotification]);

	const getNodeP2PConnectionActive = useCallback(
		(nodeId: number) => {
			return p2pStateData?.nodes.find(
				(nodeState) => nodeState.node_id === nodeId,
			)?.active;
		},
		[p2pStateData],
	);

	const clearReconnectWait = useCallback((nodeId: number) => {
		const timeoutId = reconnectWaitTimeoutsRef.current[nodeId];
		if (timeoutId != null) {
			window.clearTimeout(timeoutId);
			delete reconnectWaitTimeoutsRef.current[nodeId];
		}

		setWaitingForReconnectByNodeId((current) => {
			if (!current[nodeId]) return current;
			const next = { ...current };
			delete next[nodeId];
			return next;
		});
	}, []);

	const startReconnectWait = useCallback(
		(nodeId: number) => {
			clearReconnectWait(nodeId);
			setWaitingForReconnectByNodeId((current) => ({
				...current,
				[nodeId]: true,
			}));
			reconnectWaitTimeoutsRef.current[nodeId] = window.setTimeout(() => {
				delete reconnectWaitTimeoutsRef.current[nodeId];
				setWaitingForReconnectByNodeId((current) => {
					if (!current[nodeId]) return current;
					const next = { ...current };
					delete next[nodeId];
					return next;
				});
			}, P2P_RECONNECT_WAIT_MS);
		},
		[clearReconnectWait],
	);

	const setP2PConnectionActive = useCallback(
		async (node: NodeInfo, active: boolean) => {
			const nodeId = node.id;
			if (!network) {
				notifyError({
					title: "No network selected",
					description: `Cannot update P2P for ${node.name} without an active network.`,
				});
				return false;
			}
			if (!(isEnabledByNodeId[nodeId] ?? false)) {
				notifyError({
					title: "P2P control is unavailable",
					description: `P2P control is disabled for ${node.name}.`,
				});
				return false;
			}

			setLoadingByNodeId((current) => ({ ...current, [nodeId]: true }));

			try {
				await triggerSetNodeP2PConnection({
					networkId: network.id,
					nodeId,
					active,
				});
				if (active) {
					startReconnectWait(nodeId);
				} else {
					clearReconnectWait(nodeId);
				}
				void revalidateP2PState();
				notifySuccess({
					title: active ? "P2P enabled" : "P2P disabled",
					description: `${node.name} was updated successfully.`,
				});
				return true;
			} catch (error) {
				notifyError({
					title: active ? "Could not enable P2P" : "Could not disable P2P",
					description: error instanceof Error ? error.message : "Network error",
				});
				return false;
			} finally {
				setLoadingByNodeId((current) => ({ ...current, [nodeId]: false }));
			}
		},
		[
			clearReconnectWait,
			isEnabledByNodeId,
			network,
			notifyError,
			notifySuccess,
			revalidateP2PState,
			startReconnectWait,
			triggerSetNodeP2PConnection,
		],
	);

	const toggleNodeP2PConnection = useCallback(
		async (node: NodeInfo) => {
			const currentActive = getNodeP2PConnectionActive(node.id);
			if (currentActive == null) {
				notifyError({
					title: "P2P status is still loading",
					description: `Try again in a moment for ${node.name}.`,
				});
				return false;
			}

			return setP2PConnectionActive(node, !currentActive);
		},
		[getNodeP2PConnectionActive, notifyError, setP2PConnectionActive],
	);

	return {
		setP2PConnectionActive,
		toggleNodeP2PConnection,
		getNodeP2PConnectionActive,
		isEnabledByNodeId,
		loadingByNodeId,
		waitingForReconnectByNodeId,
		isFeatureEnabled,
	};
}
