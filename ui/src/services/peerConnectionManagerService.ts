import type { RpcActionResponse } from "../types";

export type PeerInfo = {
	id: number;
	addr: string;
	subver: string;
	inbound: boolean;
	connection_type: string;
	network: string;
	matched_node_id?: number;
	matched_node_name?: string;
};

export type NodePeerState = {
	node_id: number;
	peers: PeerInfo[];
};

export type NodeP2PAddress = {
	node_id: number;
	name: string;
	address: string;
};

export type PeerInfoResponse = {
	nodes: NodePeerState[];
	node_p2p_addresses: NodeP2PAddress[];
};

export async function fetchPeerInfo(
	networkId: number,
	signal?: AbortSignal,
): Promise<PeerInfoResponse> {
	const res = await fetch(`/api/${networkId}/peer-info.json`, { signal });
	if (!res.ok) throw new Error(`fetchPeerInfo: ${res.status}`);
	return res.json();
}

export async function addNode(
	networkId: number,
	nodeId: number,
	address: string,
): Promise<RpcActionResponse> {
	const res = await fetch(`/api/${networkId}/add-node`, {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ node_id: nodeId, address }),
	});
	return res.json();
}

export async function disconnectNode(
	networkId: number,
	nodeId: number,
	address: string,
	peerId: number,
	addnodeRemoveAddresses: string[],
	counterpartyNodeId: number | null,
	localListenAddressCandidates: string[],
): Promise<RpcActionResponse> {
	const res = await fetch(`/api/${networkId}/disconnect-node`, {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({
			node_id: nodeId,
			address,
			peer_id: peerId,
			addnode_remove_addresses: addnodeRemoveAddresses,
			counterparty_node_id: counterpartyNodeId,
			local_listen_address_candidates: localListenAddressCandidates,
		}),
	});
	return res.json();
}
