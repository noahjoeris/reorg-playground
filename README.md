<a id="readme-top"></a>

# Reorg Playground

> 🚧 **In Progress**  
> This project is under active development. Features, APIs, and setup steps may change.

Reorg Playground is a Bitcoin tool for exploring forks, tip divergence, and reorg behavior across multiple node backends.

In normal network conditions, deeper reorg events are uncommon. That makes it easy for fork/reorg edge cases in wallets, explorers, and other Bitcoin-integrated systems to slip through pre-production testing.

With Reorg Playground, you can watch network state in near real time and deliberately produce blocks, isolate nodes, and create competing branches in development environments (Regtest today; custom Signet workflows next).

Inspired by [Fork Observer](https://github.com/0xb10c/fork-observer), but redesigned for interactive reorg experimentation.

![Reorg Playground UI](screenshot.webp)


## Getting Started

1. Clone the repo: `git clone https://github.com/noahjoeris/reorg-playground.git && cd reorg-playground`
2. Start everything: `docker compose up -d --build`
3. Open the app: `http://localhost`
4. Shutdown at anytime: `docker compose down`

This Docker setup boots:
- 2 connected Bitcoin Core Regtest nodes (`bitcoind-regtest-a`, `bitcoind-regtest-b`)
- 1 backend service (Rust API)
- 1 frontend service (served at `http://localhost`)

<p align="right">(<a href="#readme-top">back to top</a>)</p>


## Features

- Interactive block-header graph with forks/tips and collapse/expand behavior for dense sections.
- Multi-backend node observation (Bitcoin Core, Electrum, Esplora, btcd) via RPC/REST.
- Node controls for Bitcoin Core (mine block, enable/disable P2P), configurable per network via `disable_node_controls`.
- Config-driven network/node setup through `config.toml`.
- Header data collection and persistence in SQLite.
- Modern, responsive UI for exploring forks and node state.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Supported node backends
- Bitcoin Core
  - uses `getchaintips` RPC to fetch active and stale block tips
  - supports node interactions like mining a block or P2P isolation.
  - recommended as the primary node (use other backends mainly as comparative observers)
- Esplora
  - doesn't support stale tips (no `getchaintips`)
  - Blockstream.info and mempool.space are Esplora-based services
- Electrum
  - doesn't support stale tips (no `getchaintips`)
- btcd
  - uses `getchaintips` RPC to fetch active and stale block tips

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Known Limitations
- Node interactions are currently supported only for Bitcoin Core on Regtest; controls can be disabled per network via `disable_node_controls`.
- The app currently relies on polling-based updates (push-based improvements are planned).

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Next Steps

- [ ] `Support custom Signet for node interactions`
- [ ] `Improve frontend-backend-node communication`
- [ ] `Add docker deployment`
- [ ] `Explore and enable more RPC calls and combinations`
- [ ] `Improve UI polish and resolve known issues`

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Tech Stack

Backend:
![Rust](https://img.shields.io/badge/Rust-1F2937?style=for-the-badge&logo=rust&logoColor=white) ![Axum](https://img.shields.io/badge/Axum-1F2937?style=for-the-badge&logoColor=white) ![Tokio](https://img.shields.io/badge/Tokio-1F2937?style=for-the-badge&logo=tokio&logoColor=white) ![SQLite](https://img.shields.io/badge/SQLite-1F2937?style=for-the-badge&logo=sqlite&logoColor=white)

Frontend:
![React](https://img.shields.io/badge/React-1F2937?style=for-the-badge&logo=react&logoColor=white) ![Vite](https://img.shields.io/badge/Vite-1F2937?style=for-the-badge&logo=vite&logoColor=white) ![Tailwind CSS](https://img.shields.io/badge/Tailwind_CSS-1F2937?style=for-the-badge&logo=tailwindcss&logoColor=white) ![shadcn/ui](https://img.shields.io/badge/shadcn%2Fui-1F2937?style=for-the-badge&logo=shadcnui&logoColor=white) ![React Flow](https://img.shields.io/badge/React_Flow-1F2937?style=for-the-badge&logo=xyflow&logoColor=white)


<p align="right">(<a href="#readme-top">back to top</a>)</p>



## Top Contributors

<a href="https://github.com/noahjoeris/reorg-playground/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=noahjoeris/reorg-playground" alt="Contributors" />
</a>

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Acknowledgement

This project is inspired by [fork-observer](https://github.com/0xb10c/fork-observer) by [0xB10C](https://github.com/0xb10c), and reuses many ideas/components adapted for Reorg Playground's goals.

<p align="right">(<a href="#readme-top">back to top</a>)</p>
