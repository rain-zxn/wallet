# L0 Wallet

A command-line wallet for the L0 blockchain with ZK proof support.

## Prerequisites

### 1. Clone Dependencies

```bash
# Clone l0 library (eon-protocol branch)
git clone -b eon-protocol https://github.com/theparadigmshifters/l0.git

# Clone zk library (eon-protocol branch)
git clone -b eon-protocol https://github.com/theparadigmshifters/zk.git
```

Make sure these repositories are cloned to the correct paths referenced in `Cargo.toml`:
- `l0 = { path = "../l0" }`
- `zk = { path = "../zk" }`

### 2. Build and Install wallet_prover

```bash
# Clone wallet_prover
git clone https://github.com/theparadigmshifters/wallet_prover.git
cd wallet_prover

# Build
go mod tidy
go build

# Install to system path
sudo cp wallet_prover /usr/local/bin/

# Verify installation
which wallet_prover
```

## Building the Wallet

```bash
cd wallet
cargo build --release
```

## Usage

### 1. Create a New Wallet

```bash
./target/release/wallet create
```

**Example output:**
```
Creating new wallet account...
Secret: 29b4f95059e36e0d40b1e1cee1c2ebe43d1a87fe6149f53fb571030211151d89
Account (VK): 12727ce7ddecd07aa535cad6bae1264bc0ee5b024a4c16916c3961a9bd2ccbb0
```

**⚠️ IMPORTANT**: Save both the **secret** and **account address** securely!
- **Secret**: Required for authorized transfers (private key)
- **Account**: Your wallet address

### 2. Get Initial Funds (Faucet)

Use **permissionless transfer** to claim funds from a faucet or public account:

```bash
./target/release/wallet \
  --api-url https://eon.zk524.com/ \
  transfer-permissionless \
  --from <FAUCET_ADDRESS> \
  --to <YOUR_ACCOUNT> \
  --amount <AMOUNT_HEX_64_CHARS>
```

**Example:**
```bash
./target/release/wallet \
  --api-url https://eon.zk524.com/ \
  transfer-permissionless \
  --from 57dc9fec5313adc71288012d20bbc8f59637a700e301f516bd358157733fc108 \
  --to 12727ce7ddecd07aa535cad6bae1264bc0ee5b024a4c16916c3961a9bd2ccbb0 \
  --amount 0000000000000000000000000000000000000000000000000000000000000064
```

**Note**: Permissionless transfers don't require a secret - anyone can transfer from the source address.

### 3. Check Balance

#### Get Total Balance

```bash
./target/release/wallet \
  --api-url https://eon.zk524.com/ \
  get-balance \
  --account <YOUR_ACCOUNT>
```

**Example:**
```bash
./target/release/wallet \
  --api-url https://eon.zk524.com/ \
  get-balance \
  --account 12727ce7ddecd07aa535cad6bae1264bc0ee5b024a4c16916c3961a9bd2ccbb0
```

#### List UTXOs (Detailed)

```bash
./target/release/wallet \
  --api-url https://eon.zk524.com/ \
  list-utxos \
  --account <YOUR_ACCOUNT>
```

**Example:**
```bash
./target/release/wallet \
  --api-url https://eon.zk524.com/ \
  list-utxos \
  --account 12727ce7ddecd07aa535cad6bae1264bc0ee5b024a4c16916c3961a9bd2ccbb0
```

### 4. Transfer Funds (With Secret)

Send funds to another account using your secret:

```bash
./target/release/wallet \
  --api-url https://eon.zk524.com/ \
  transfer \
  --from <YOUR_ACCOUNT> \
  --to <RECIPIENT_ACCOUNT> \
  --amount <AMOUNT_HEX_64_CHARS> \
  --secret <YOUR_SECRET>
```

**Example:**
```bash
./target/release/wallet \
  --api-url https://eon.zk524.com/ \
  transfer \
  --from 12727ce7ddecd07aa535cad6bae1264bc0ee5b024a4c16916c3961a9bd2ccbb0 \
  --to 576ba01604a96e3969453b87a4dce6da9373ddbacda08b113e2bbdd686eeddb4 \
  --amount 0000000000000000000000000000000000000000000000000000000000000010 \
  --secret 29b4f95059e36e0d40b1e1cee1c2ebe43d1a87fe6149f53fb571030211151d89
```

**This will:**
1. Fetch UTXOs from your account
2. Select appropriate UTXOs to cover the amount
3. Construct a transaction
4. Generate a ZK proof using your secret (proves ownership)
5. Submit the transaction to the network

**Success output:**
```
Transaction hash: 6df28f8b19a16c82b099549a841b5b1e9706c9fc15fc76b8cd835116d0aaabfb
```

## Amount Format

Amounts must be **64-character hex strings** (32 bytes):
- Decimal 100 = `0000000000000000000000000000000000000000000000000000000000000064`
- Decimal 1000 = `00000000000000000000000000000000000000000000000000000000000003e8`

Use an online hex converter or:
```bash
printf "%064x\n" 100
```

## Commands Reference

| Command | Description | Requires Secret |
|---------|-------------|-----------------|
| `create` | Generate new wallet | No |
| `get-balance` | Get total account balance | No |
| `list-utxos` | View detailed UTXOs | No |
| `transfer-permissionless` | Transfer from public account | No |
| `transfer` | Transfer from your account | Yes |

## Architecture

```
wallet (Rust CLI)
    ↓ Calls wallet_prover (Go binary)
    ↓ HTTP JSON-RPC to api_http
    ↓ L0 blockchain
```

**Key Components:**
- **wallet_prover**: Generates ZK proofs for transactions
  - `hash_wallet` circuit: Proves secret ownership
  - `permissionless` circuit: No secret required
- **l0 crate**: Core transaction types
- **zk crate**: Cryptographic primitives