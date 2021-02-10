#[link(name = "sodium", kind = "static")]
extern "C" {
    // int crypto_vrf_prove(unsigned char *proof, const unsigned char *sk, const unsigned char *m, unsigned long long mlen);
    fn crypto_vrf_prove(proof: *mut u8, sk: *const u8, m: *const u8, mlen: u64) -> i32;

    // int crypto_vrf_proof_to_hash(unsigned char *hash, const unsigned char *proof);
    fn crypto_vrf_proof_to_hash(hash: *mut u8, proof: *const u8) -> i32;
}

pub(crate) fn sodium_crypto_vrf_prove(secret_key: &[u8], seed: &[u8]) -> Result<Vec<u8>, String> {
    let mut proof: Vec<u8> = Vec::with_capacity(80);
    unsafe {
        let rc = crypto_vrf_prove(
            proof.as_mut_ptr(),
            secret_key.as_ptr(),
            seed.as_ptr(),
            seed.len() as u64,
        );
        if rc != 0 {
            Err(format!(
                "libsodium crypto_vrf_prove() failed, returned {}, expected 0",
                rc
            ))
        } else {
            proof.set_len(80);
            Ok(proof)
        }
    }
}

pub(crate) fn sodium_crypto_vrf_proof_to_hash(proof: &[u8]) -> Result<Vec<u8>, String> {
    let mut hash: Vec<u8> = Vec::with_capacity(64);
    unsafe {
        let rc = crypto_vrf_proof_to_hash(hash.as_mut_ptr(), proof.as_ptr());
        if rc != 0 {
            Err(format!(
                "libsodium crypto_vrf_proof_to_hash() failed, returned {}, expected 0",
                rc
            ))
        } else {
            hash.set_len(64);
            Ok(hash)
        }
    }
}
