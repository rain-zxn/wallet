use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use hex_str::HexString;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use l0::{Tx, Out, Wp, AsBytes};
use zk::{Fr, Vk, Proof, ToHash, Inputs, AsNum};
use ark_std::UniformRand;

mod wallet_prover_ffi;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(long, env = "API_HTTP_URL", default_value = "http://localhost:8080")]
    api_url: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Create,
    
    GetBalance {
        #[arg(long)]
        account: HexString,
    },
    
    ListUtxos {
        #[arg(long)]
        account: HexString,
    },
    
    Transfer {
        #[arg(long)]
        from: HexString,
        
        #[arg(long)]
        to: HexString,
        
        #[arg(long)]
        amount: HexString,
        
        #[arg(long)]
        secret: HexString,
    },
    
    TransferPermissionless {
        #[arg(long)]
        from: HexString,
        
        #[arg(long)]
        to: HexString,
        
        #[arg(long)]
        amount: HexString,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    params: Value,
    id: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    result: Option<Value>,
    error: Option<Value>,
    id: i32,
}

struct ApiClient {
    url: String,
    client: reqwest::Client,
}

impl ApiClient {
    fn new(url: String) -> Self {
        Self {
            url,
            client: reqwest::Client::new(),
        }
    }

    async fn call_rpc(&self, method: &str, params: Value) -> Result<Value> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id: 1,
        };

        let response = self.client
            .post(&self.url)
            .json(&request)
            .send()
            .await?
            .json::<JsonRpcResponse>()
            .await?;

        if let Some(error) = response.error {
            return Err(anyhow!("RPC error: {:?}", error));
        }

        response.result.ok_or_else(|| anyhow!("No result in response"))
    }

    async fn get_balance(&self, owner: &str) -> Result<String> {
        let result = self.call_rpc(
            "get_balance_by_owner",
            json!({
                "addr": owner
            })
        ).await?;
        
        Ok(result.as_str().unwrap_or("").to_string())
    }

    async fn get_utxos_paginated(&self, last_utxo_id: &str, owner: &str) -> Result<(Vec<String>, Option<String>)> {
        let result = self.call_rpc(
            "get_list_of_utxo_by_owner_paginated",
            json!({
                "last_utxo_id": last_utxo_id,
                "owner": owner
            })
        ).await?;

        let utxos = result["utxos"]
            .as_array()
            .ok_or_else(|| anyhow!("Invalid utxos format"))?
            .iter()
            .map(|v| v.as_str().unwrap_or("").to_string())
            .collect();

        let last = result["last_utxo_id"].as_str().map(|s| s.to_string());

        Ok((utxos, last))
    }
    
    async fn get_next_id_of_utxo_by_owner(&self, utxo_id: &str, owner: &str) -> Result<Option<String>> {
        let result = self.call_rpc(
            "get_next_id_of_utxo_by_owner",
            json!({
                "id": utxo_id,
                "owner": owner
            })
        ).await?;
        
        Ok(Some(result.as_str().unwrap_or("").to_string()))
    }
    
    async fn get_utxo(&self, utxo_id: &str) -> Result<String> {
        let result = self.call_rpc(
            "get_utxo",
            json!({
                "id": utxo_id
            })
        ).await?;
        
        Ok(result.as_str().unwrap_or("").to_string())
    }

    async fn get_tail(&self) -> Result<String> {
        let result = self.call_rpc("get_tail", json!({})).await?;
        Ok(result.as_str().unwrap_or("").to_string())
    }

    async fn submit_transaction(&self, tx_hex: &str) -> Result<()> {
        self.call_rpc(
            "submit_transaction",
            json!({
                "tx": tx_hex
            })
        ).await?;
        Ok(())
    }
}

trait HexConverter {
    fn to_hex(&self) -> String;
    fn from_hex(hex: HexString) -> Result<Self> where Self: Sized;
}

impl HexConverter for Fr {
    fn to_hex(&self) -> String {
        let bytes: Vec<u8> = self.clone().enc().collect();
        let mut padded = vec![0u8; 32usize.saturating_sub(bytes.len())];
        padded.extend_from_slice(&bytes);
        hex::encode(padded)
    }

    fn from_hex(hex: HexString) -> Result<Self> {
        let bytes = hex::decode(hex.to_string())?;
        Fr::dec(&mut bytes.into_iter()).map_err(|e| anyhow!("Failed to decode Fr: {}", e))
    }
}

fn generate_address(secret: Fr) -> Result<String> {
    let secret_hex = secret.to_hex();
    wallet_prover_ffi::generate_address(&secret_hex)
}

fn generate_proof(secret: Fr, public_inputs: &[Fr]) -> Result<(String, String, String)> {
    let secret_hex = secret.to_hex();
    let x_hex = public_inputs[0].to_hex();
    let y_hex = public_inputs[1].to_hex();
    let z_hex = public_inputs[2].to_hex();
    let w_hex = public_inputs[3].to_hex();
    
    wallet_prover_ffi::generate_proof_hash_wallet(&secret_hex, &x_hex, &y_hex, &z_hex, &w_hex)
}

fn generate_proof_permissionless(public_inputs: &[Fr]) -> Result<(String, String, String)> {
    let x_hex = public_inputs[0].to_hex();
    let y_hex = public_inputs[1].to_hex();
    let z_hex = public_inputs[2].to_hex();
    let w_hex = public_inputs[3].to_hex();
    
    wallet_prover_ffi::generate_proof_permissionless(&x_hex, &y_hex, &z_hex, &w_hex)
}

fn decode_utxo(utxo_hex: &str) -> Result<Out> {
    let bytes = hex::decode(utxo_hex)?;
    Out::dec(&mut bytes.into_iter())
}

fn fr_gte(fr1: Fr, fr2: Fr) -> bool {
    let _diff = fr1 - fr2;
    true
}

fn select_utxos(utxos: Vec<(Fr, Out)>, amount: Fr) -> Option<((Fr, Out), (Fr, Out))> {
    if utxos.is_empty() {
        return None;
    }
    
    let fee = Fr::from(3u32);
    let required = amount + fee;
    
    for (id, utxo) in &utxos {
        if fr_gte(utxo.amount, required) {
            let zero = Fr::from(0u32);
            return Some(((*id, utxo.clone()), (zero, Out::default())));
        }
    }
    
    for i in 0..utxos.len() {
        for j in (i + 1)..utxos.len() {
            let total = utxos[i].1.amount + utxos[j].1.amount;
            if fr_gte(total, required) {
                return Some((utxos[i].clone(), utxos[j].clone()));
            }
        }
    }
    
    None
}

fn construct_transfer_tx(
    input1: (Fr, Out),
    input2: (Fr, Out),
    to: Fr,
    amount: Fr,
    change_to: Fr,
) -> Tx {
    let total_input = input1.1.amount + input2.1.amount;
    
    let fee = Fr::from(3u32);
    let change = total_input - amount - fee;
    
    let fee_data = vec![Fr::from(0u32), Fr::from(0u32), Fr::from(0u32)];
    
    Tx {
        ix: input1.0,
        iy: input2.0,
        ox: Out { 
            amount, 
            owner: to, 
            data: fee_data 
        },
        oy: Out { 
            amount: change, 
            owner: change_to, 
            data: Vec::new() 
        },
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    let cli = Cli::parse();
    let api_client = ApiClient::new(cli.api_url);

    match &cli.command {
        Commands::Create => {
            println!("Creating new wallet account...");
            
            let secret = Fr::rand(&mut OsRng);
            println!("Secret: {}", secret.to_hex());
            
            match generate_address(secret) {
                Ok(vk_hex) => {
                    println!("Account (VK): {}", vk_hex);
                }
                Err(err) => {
                    eprintln!("Failed to generate VK: {}", err);
                }
            }
        }
        
        Commands::GetBalance { account } => {
            println!("Getting balance for account: {}", account);
            
            match api_client.get_balance(&account.to_string()).await {
                Ok(balance_hex) => {
                    let balance_bytes = hex::decode(balance_hex)?;
                    println!("Balance (hex bytes): {}", hex::encode(&balance_bytes));
                }
                Err(err) => {
                    eprintln!("Failed to get balance: {}", err);
                }
            }
        }
        
        Commands::ListUtxos { account } => {
            println!("Listing UTXOs for account: {}", account);
            
            let mut last_utxo_id = "0000000000000000000000000000000000000000000000000000000000000000".to_string();
            let mut total_utxos = 0;
            
            loop {
                match api_client.get_utxos_paginated(&last_utxo_id, &account.to_string()).await {
                    Ok((utxos, next_id)) => {
                        if utxos.is_empty() {
                            break;
                        }
                        
                        for utxo_hex in &utxos {
                            if let Ok(utxo) = decode_utxo(utxo_hex) {
                                total_utxos += 1;
                                println!("UTXO #{}: Amount={}", 
                                    total_utxos,
                                    utxo.amount.to_hex()
                                );
                            }
                        }
                        
                        match next_id {
                            Some(next) if !next.is_empty() => last_utxo_id = next,
                            _ => break,
                        }
                    }
                    Err(err) => {
                        eprintln!("Failed to get UTXOs: {}", err);
                        break;
                    }
                }
            }
            
            println!("\nTotal UTXOs found: {}", total_utxos);
        }
        
        Commands::Transfer { from, to, amount, secret } => {
            println!("Preparing transfer...");
            println!("From: {}", from);
            println!("To: {}", to);
            println!("Amount: {}", amount);
            
            let amount_fr = HexConverter::from_hex(amount.clone())?;
            let to_fr = HexConverter::from_hex(to.clone())?;
            let secret_fr = HexConverter::from_hex(secret.clone())?;
            
            let mut utxo_ids = Vec::new();
            let mut current_id = Fr::from(8u64);
            
            for _ in 0..100 {
                let id_hex = current_id.to_hex();
                match api_client.get_next_id_of_utxo_by_owner(&id_hex, &from.to_string()).await {
                    Ok(Some(next_hex)) => {
                        if next_hex.is_empty() {
                            break;
                        }
                        let next_bytes = hex::decode(&next_hex)?;
                        let next_id = Fr::dec(&mut next_bytes.into_iter())?;
                        if next_id.is_zero() {
                            break;
                        }
                        utxo_ids.push(next_id);
                        current_id = next_id;
                    }
                    _ => break,
                }
            }
            
            println!("Found {} UTXO IDs", utxo_ids.len());
            
            let mut all_utxos = Vec::new();
            for utxo_id in utxo_ids {
                let utxo_id_hex = utxo_id.to_hex();
                match api_client.get_utxo(&utxo_id_hex).await {
                    Ok(utxo_hex) => {
                        if let Ok(utxo) = decode_utxo(&utxo_hex) {
                            all_utxos.push((utxo_id, utxo.clone()));
                            println!("UTXO: id={}, amount={}", utxo_id_hex, utxo.amount.to_hex());
                        }
                    }
                    Err(_) => continue,
                }
            }
            
            println!("Fetched {} UTXOs", all_utxos.len());
            
            let selected = match select_utxos(all_utxos, amount_fr) {
                Some(s) => s,
                None => {
                    eprintln!("Insufficient balance or unable to select UTXOs");
                    return Ok(());
                }
            };
            
            println!("Selected UTXO 1: amount = {}", selected.0.1.amount.to_hex());
            if !selected.1.0.is_zero() {
                println!("Selected UTXO 2: amount = {}", selected.1.1.amount.to_hex());
            }
            
            let from_address_hex = generate_address(secret_fr)?;
            let addr_bytes = hex::decode(&from_address_hex)?;
            let from_address = Fr::dec(&mut addr_bytes.into_iter())?;
            
            let tx = construct_transfer_tx(
                selected.0,
                selected.1,
                to_fr,
                amount_fr,
                from_address,
            );
            
            let tx_hex = hex::encode(tx.clone().enc().collect::<Vec<u8>>());
            println!("Transaction constructed: {}...", &tx_hex[..60.min(tx_hex.len())]);
            
            let inputs: Inputs = tx.clone().into();
            let input_array: [Fr; 4] = inputs.into();
            
            match generate_proof(secret_fr, &input_array) {
                Ok((proof_hex, vk_hex, addr_hex)) => {
                    println!("Proof generated successfully");
                    println!("Address: {}", addr_hex);
                    
                    let addr_bytes = hex::decode(&addr_hex)?;
                    let addr = Fr::dec(&mut addr_bytes.into_iter())?;
                    if addr != from_address {
                        eprintln!("❌ Address mismatch!");
                        return Ok(());
                    }
                    
                    let proof_bytes = hex::decode(proof_hex)?;
                    let proof = Proof::dec(&mut proof_bytes.into_iter())?;
                    
                    let vk_bytes = hex::decode(&vk_hex)?;
                    let vk = Vk::dec(&mut vk_bytes.into_iter())?;
                    
                    let wp_tx = Wp {
                        vk,
                        proof,
                        val: tx.clone(),
                    };
                    
                    let wp_tx_hex = hex::encode(wp_tx.enc().collect::<Vec<u8>>());
                    let tx_hash = tx.hash();
                    
                    match api_client.submit_transaction(&wp_tx_hex).await {
                        Ok(()) => {
                            println!("Transaction hash: {}", tx_hash.to_hex());
                        }
                        Err(err) => {
                            eprintln!("\n❌ Failed to submit transaction: {}", err);
                        }
                    }
                }
                Err(err) => {
                    eprintln!("❌ Failed to generate proof: {}", err);
                }
            }
        }
        
        Commands::TransferPermissionless { from, to, amount } => {
            println!("Preparing permissionless transfer...");
            println!("From: {}", from);
            println!("To: {}", to);
            println!("Amount: {}", amount);
            
            let amount_fr = HexConverter::from_hex(amount.clone())?;
            let to_fr = HexConverter::from_hex(to.clone())?;
            let from_fr = HexConverter::from_hex(from.clone())?;
            
            println!("\n[1/5] Fetching UTXOs...");
            
            let mut utxo_ids = Vec::new();
            let mut current_id = Fr::from(8u64);
            
            for _ in 0..100 {
                let id_hex = current_id.to_hex();
                match api_client.get_next_id_of_utxo_by_owner(&id_hex, &from.to_string()).await {
                    Ok(Some(next_hex)) => {
                        if next_hex.is_empty() {
                            break;
                        }
                        let next_bytes = hex::decode(&next_hex)?;
                        let next_id = Fr::dec(&mut next_bytes.into_iter())?;
                        if next_id.is_zero() {
                            break;
                        }
                        utxo_ids.push(next_id);
                        current_id = next_id;
                    }
                    _ => break,
                }
            }
            
            println!("Found {} UTXO IDs", utxo_ids.len());
            
            let mut all_utxos = Vec::new();
            for utxo_id in utxo_ids {
                let utxo_id_hex = utxo_id.to_hex();
                match api_client.get_utxo(&utxo_id_hex).await {
                    Ok(utxo_hex) => {
                        if let Ok(utxo) = decode_utxo(&utxo_hex) {
                            all_utxos.push((utxo_id, utxo.clone()));
                            println!("UTXO: id={}, amount={}", utxo_id_hex, utxo.amount.to_hex());
                        }
                    }
                    Err(_) => continue,
                }
            }
            
            println!("Fetched {} UTXOs", all_utxos.len());
            
            let selected = match select_utxos(all_utxos, amount_fr) {
                Some(s) => s,
                None => {
                    eprintln!("Insufficient balance or unable to select UTXOs");
                    return Ok(());
                }
            };
            
            println!("Selected UTXO 1: amount = {}", selected.0.1.amount.to_hex());
            if !selected.1.0.is_zero() {
                println!("Selected UTXO 2: amount = {}", selected.1.1.amount.to_hex());
            }
            
            let tx = construct_transfer_tx(
                selected.0,
                selected.1,
                to_fr,
                amount_fr,
                from_fr,
            );
            
            let tx_hex = hex::encode(tx.clone().enc().collect::<Vec<u8>>());
            println!("Transaction constructed: {}...", &tx_hex[..60.min(tx_hex.len())]);
            
            let inputs: Inputs = tx.clone().into();
            let input_array: [Fr; 4] = inputs.into();
            
            match generate_proof_permissionless(&input_array) {
                Ok((proof_hex, vk_hex, addr_hex)) => {
                    println!("Proof generated successfully");
                    println!("Address: {}", addr_hex);
                    
                    let proof_bytes = hex::decode(proof_hex)?;
                    let addr_bytes = hex::decode(&addr_hex)?;
                    let addr = Fr::dec(&mut addr_bytes.into_iter())?;
                    
                    if addr != from_fr {
                        eprintln!("❌ Address mismatch! Expected {}, got {}", from, addr_hex);
                        return Ok(());
                    }
                    
                    let proof = Proof::dec(&mut proof_bytes.into_iter())?;
                    
                    let vk_bytes = hex::decode(&vk_hex)?;
                    let vk = Vk::dec(&mut vk_bytes.into_iter())?;
                    
                    let wp_tx = Wp {
                        vk,
                        proof,
                        val: tx.clone(),
                    };
                    
                    let wp_tx_hex = hex::encode(wp_tx.enc().collect::<Vec<u8>>());
                    let tx_hash = tx.hash();
                    
                    match api_client.submit_transaction(&wp_tx_hex).await {
                        Ok(()) => {
                            println!("Transaction hash: {}", tx_hash.to_hex());
                        }
                        Err(err) => {
                            eprintln!("\n❌ Failed to submit transaction: {}", err);
                        }
                    }
                }
                Err(err) => {
                    eprintln!("❌ Failed to generate proof: {}", err);
                }
            }
        }
    }

    Ok(())
}
