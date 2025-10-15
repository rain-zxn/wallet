use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use anyhow::{anyhow, Result};

#[link(name = "wallet_prover", kind = "static")]
extern "C" {
    fn GenerateAddress(secret_hex: *const c_char) -> *mut c_char;
    fn GenerateProofPermissionless(
        x_hex: *const c_char,
        y_hex: *const c_char,
        z_hex: *const c_char,
        w_hex: *const c_char,
    ) -> *mut c_char;
    fn GenerateProofHashWallet(
        secret_hex: *const c_char,
        x_hex: *const c_char,
        y_hex: *const c_char,
        z_hex: *const c_char,
        w_hex: *const c_char,
    ) -> *mut c_char;
    fn FreeString(s: *mut c_char);
}

pub fn generate_address(secret_hex: &str) -> Result<String> {
    let c_secret = CString::new(secret_hex)?;
    
    unsafe {
        let result_ptr = GenerateAddress(c_secret.as_ptr());
        if result_ptr.is_null() {
            return Err(anyhow!("GenerateAddress returned null"));
        }
        
        let c_str = CStr::from_ptr(result_ptr);
        let result = c_str.to_str()?.to_string();
        FreeString(result_ptr);
        
        if result.is_empty() {
            return Err(anyhow!("GenerateAddress failed"));
        }
        
        Ok(result)
    }
}

pub fn generate_proof_permissionless(
    x_hex: &str,
    y_hex: &str,
    z_hex: &str,
    w_hex: &str,
) -> Result<(String, String, String)> {
    let c_x = CString::new(x_hex)?;
    let c_y = CString::new(y_hex)?;
    let c_z = CString::new(z_hex)?;
    let c_w = CString::new(w_hex)?;
    
    unsafe {
        let result_ptr = GenerateProofPermissionless(
            c_x.as_ptr(),
            c_y.as_ptr(),
            c_z.as_ptr(),
            c_w.as_ptr(),
        );
        
        if result_ptr.is_null() {
            return Err(anyhow!("GenerateProofPermissionless returned null"));
        }
        
        let c_str = CStr::from_ptr(result_ptr);
        let result = c_str.to_str()?.to_string();
        FreeString(result_ptr);
        
        if result.is_empty() {
            return Err(anyhow!("GenerateProofPermissionless failed"));
        }
        
        parse_proof_result(&result)
    }
}

pub fn generate_proof_hash_wallet(
    secret_hex: &str,
    x_hex: &str,
    y_hex: &str,
    z_hex: &str,
    w_hex: &str,
) -> Result<(String, String, String)> {
    let c_secret = CString::new(secret_hex)?;
    let c_x = CString::new(x_hex)?;
    let c_y = CString::new(y_hex)?;
    let c_z = CString::new(z_hex)?;
    let c_w = CString::new(w_hex)?;
    
    unsafe {
        let result_ptr = GenerateProofHashWallet(
            c_secret.as_ptr(),
            c_x.as_ptr(),
            c_y.as_ptr(),
            c_z.as_ptr(),
            c_w.as_ptr(),
        );
        
        if result_ptr.is_null() {
            return Err(anyhow!("GenerateProofHashWallet returned null"));
        }
        
        let c_str = CStr::from_ptr(result_ptr);
        let result = c_str.to_str()?.to_string();
        FreeString(result_ptr);
        
        if result.is_empty() {
            return Err(anyhow!("GenerateProofHashWallet failed"));
        }
        
        parse_proof_result(&result)
    }
}

fn parse_proof_result(result: &str) -> Result<(String, String, String)> {
    let parts: Vec<&str> = result.split(',').collect();
    if parts.len() == 3 {
        Ok((
            parts[0].to_string(),
            parts[1].to_string(),
            parts[2].to_string(),
        ))
    } else {
        Err(anyhow!(
            "Invalid proof output format, expected: proof,vk,address"
        ))
    }
}
