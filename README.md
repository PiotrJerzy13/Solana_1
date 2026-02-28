# Solana ICO — On-Chain Token Sale Program

A production-grade Initial Coin Offering (ICO) smart contract built with [Anchor](https://www.anchor-lang.com/) on Solana, featuring a React + Phantom wallet frontend.

---

## Overview

This project implements a fully on-chain ICO where users can purchase SPL tokens using SOL. All logic — pricing, lifecycle management, per-wallet caps, and fund custody — is handled by the Anchor program. There is no traditional backend server.

```
Phantom Wallet (browser)
       ↓
  React Frontend
       ↓
Solana Validator (localnet / devnet / mainnet)
       ↓
  Anchor Program (on-chain logic)
```

---

## Features

- **Dynamic decimal handling** — works with any SPL token regardless of decimal configuration
- **ICO lifecycle** — configurable start/end times with a 60-second clock drift tolerance
- **Emergency pause** — admin can pause and resume the ICO at any time
- **Per-wallet purchase cap** — prevents whale domination (default: 10,000 tokens)
- **Dust attack protection** — rejects purchases that round down to 0 lamports
- **Vault security** — program vault is a PDA-owned ATA, preventing account substitution attacks
- **Canonical bump signing** — uses `ctx.bumps` for all PDA signatures
- **Clean exit** — `withdraw_unsold` closes both the vault ATA and data PDA, reclaiming all rent
- **On-chain events** — emits `TokensPurchased` and `IcoStatusChanged` for off-chain indexing
- **u128 overflow protection** — all price calculations use 128-bit intermediate math

---

## Program Instructions

| Instruction | Description | Access |
|---|---|---|
| `create_ico_ata` | Initialize ICO, fund vault, set start/end times | Admin only |
| `buy_tokens` | Purchase tokens with SOL | Any wallet |
| `toggle_pause` | Pause or resume the ICO | Admin only |
| `withdraw_unsold` | Reclaim unsold tokens and close accounts after ICO ends | Admin only |

---

## Security Architecture

- `ADMIN_PUBKEY` is hardcoded at compile time — no runtime admin spoofing
- All ATAs are derived with `associated_token` constraints — no fake vault injection
- `has_one = admin` double-binds admin on every privileged instruction
- `SystemAccount<'info>` type on admin enforces system program ownership
- Bump stored from `ctx.bumps` — immune to bump manipulation attacks

---

## Project Structure

```
solana_ico/
├── programs/
│   └── solana_ico/
│       └── src/
│           └── lib.rs          # Anchor program
├── scripts/
│   └── init_ico.ts             # ICO initialization script
├── tests/
│   └── solana_ico.ts           # Full test suite (13 tests)
├── frontend/
│   └── src/
│       └── App.tsx             # React + Phantom frontend
├── Anchor.toml
└── Cargo.toml
```

---

## Getting Started

### Prerequisites

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Solana CLI
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"

# Anchor (via avm)
cargo install --git https://github.com/coral-xyz/anchor avm --locked
avm install 0.32.0
avm use 0.32.0

# Node.js 18+
nvm install 18 && nvm use 18
```

### Installation

```bash
git clone https://github.com/PiotrJerzy13/Solana_1.git
cd Solana_1/solana_ico
yarn install
```

### Local Development

**Terminal 1 — Start validator:**
```bash
solana-test-validator --reset
```

**Terminal 2 — Deploy and initialize:**
```bash
# Generate wallet if needed
solana-keygen new --outfile ~/.config/solana/id.json
solana config set --url localhost
solana airdrop 10

# Build and deploy
anchor build
anchor deploy

# Initialize ICO (prints addresses for frontend)
anchor run init_ico
```

**Terminal 3 — Start frontend:**
```bash
cd frontend
yarn dev
```

Open [http://localhost:5173](http://localhost:5173) and connect Phantom set to **Localnet**.

---

## Configuration

Update `programs/solana_ico/src/lib.rs` before deployment:

```rust
// Your admin wallet pubkey
pub const ADMIN_PUBKEY: Pubkey = pubkey!("YOUR_PUBKEY_HERE");

// Price: 1_000_000 lamports = 0.001 SOL per whole token
pub const PRICE_PER_WHOLE_TOKEN_LAMPORTS: u64 = 1_000_000;

// Per-wallet purchase cap in whole tokens
pub const MAX_WHOLE_TOKENS_PER_WALLET: u64 = 10_000;
```

After `anchor run init_ico`, update `frontend/src/App.tsx`:

```typescript
const PROGRAM_ID = new PublicKey('YOUR_PROGRAM_ID');
const ICO_MINT   = new PublicKey('YOUR_MINT_ADDRESS');
const ADMIN      = new PublicKey('YOUR_ADMIN_PUBKEY');
```

---

## Running Tests

```bash
# With validator already running
anchor test --skip-local-validator

# Or let Anchor manage the validator
anchor test
```

**Test coverage:**

- `create_ico_ata` — invalid time range, start time in past, successful initialization
- `buy_tokens` — zero amount, dust truncation, successful purchase, wallet cap exceeded
- `toggle_pause` — admin pause/resume, purchase blocked while paused, non-admin rejected
- `withdraw_unsold` — blocked while ICO running, post-expiry withdrawal

---

## Deployment

### Devnet

```bash
solana config set --url devnet
solana airdrop 2
anchor deploy
anchor run init_ico
```

### Mainnet

> ⚠️ **Do not deploy to mainnet without a professional security audit.**
> Recommended auditors: OtterSec, Neodyme, Zellic.

```bash
solana config set --url mainnet-beta
anchor deploy
```

---

## Tech Stack

| Layer | Technology |
|---|---|
| Smart Contract | Rust, Anchor 0.32 |
| Token Standard | SPL Token |
| Frontend | React, TypeScript, Vite |
| Wallet | Phantom via `@solana/wallet-adapter` |
| Testing | Mocha, Chai, `@coral-xyz/anchor` |

---
