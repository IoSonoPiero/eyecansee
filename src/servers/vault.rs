//! The saved servers' file encrypted with a master password. Argon2id (64 MiB, 3 passes) turns
//! the password and a random salt into a 256-bit key; ChaCha20-Poly1305 (ring) encrypts the JSON
//! and authenticates it, header included, so a wrong password or a changed byte is refused.
//! Format: "ECSVAULT1" | memory KiB, passes, lanes (u32 LE each) | salt (16) | nonce (12) |
//! ciphertext and tag. A new random nonce at every save, a new salt with every new password.

use argon2::{Algorithm, Argon2, Params, Version};
use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};

const MAGIC: &[u8] = b"ECSVAULT1";
const HEADER: usize = MAGIC.len() + 12 + 16 + 12;
/// Argon2id as OWASP suggests for a key that guards secrets: about half a second here.
const COST: Cost = (64 * 1024, 3, 1);

/// Argon2id's memory in KiB, passes and lanes.
type Cost = (u32, u32, u32);

/// A key derived from the master password, kept in memory while the program runs.
#[derive(Clone)]
pub struct Key {
    key: [u8; 32],
    salt: [u8; 16],
    cost: (u32, u32, u32),
}

impl Drop for Key {
    fn drop(&mut self) {
        // ponytail: a plain overwrite, not a guaranteed wipe (copies the compiler made stay)
        self.key = [0; 32];
    }
}

fn derive(password: &str, salt: [u8; 16], cost: (u32, u32, u32)) -> Result<Key, String> {
    let params = Params::new(cost.0, cost.1, cost.2, Some(32)).map_err(|e| e.to_string())?;
    let mut key = [0; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password_into(password.as_bytes(), &salt, &mut key).map_err(|e| e.to_string())?;
    Ok(Key { key, salt, cost })
}

/// A new key for a new master password, with a new salt.
pub fn new_key(password: &str) -> Result<Key, String> {
    new_key_with(password, COST)
}

fn new_key_with(password: &str, cost: (u32, u32, u32)) -> Result<Key, String> {
    let mut salt = [0; 16];
    SystemRandom::new().fill(&mut salt).map_err(|_| "no random numbers".to_string())?;
    derive(password, salt, cost)
}

pub fn is_vault(data: &[u8]) -> bool {
    data.starts_with(MAGIC)
}

fn header(key: &Key, nonce: &[u8; 12]) -> Vec<u8> {
    let mut h = MAGIC.to_vec();
    for n in [key.cost.0, key.cost.1, key.cost.2] {
        h.extend_from_slice(&n.to_le_bytes());
    }
    h.extend_from_slice(&key.salt);
    h.extend_from_slice(nonce);
    h
}

/// `plain` encrypted with `key`.
pub fn seal(key: &Key, plain: &[u8]) -> Result<Vec<u8>, String> {
    let mut nonce = [0; 12];
    SystemRandom::new().fill(&mut nonce).map_err(|_| "no random numbers".to_string())?;
    let mut out = header(key, &nonce);
    let mut body = plain.to_vec();
    let sealing = LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, &key.key).map_err(|_| "bad key".to_string())?);
    sealing
        .seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(out.clone()), &mut body)
        .map_err(|_| "encryption failed".to_string())?;
    out.extend_from_slice(&body);
    Ok(out)
}

/// The salt, cost and nonce written in a vault's header.
fn read_header(data: &[u8]) -> Option<([u8; 16], Cost, [u8; 12])> {
    if !is_vault(data) || data.len() < HEADER + 16 {
        return None;
    }
    let n = |i: usize| u32::from_le_bytes(data[MAGIC.len() + 4 * i..MAGIC.len() + 4 * i + 4].try_into().unwrap());
    let at = MAGIC.len() + 12;
    Some((data[at..at + 16].try_into().ok()?, (n(0), n(1), n(2)), data[at + 16..HEADER].try_into().ok()?))
}

/// The contents of a vault, with the key it was sealed with; `None`: wrong key or a damaged file.
pub fn open_with(key: &Key, data: &[u8]) -> Option<Vec<u8>> {
    let (salt, cost, nonce) = read_header(data)?;
    if salt != key.salt || cost != key.cost {
        return None;
    }
    let opening = LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, &key.key).ok()?);
    let mut body = data[HEADER..].to_vec();
    let plain = opening.open_in_place(Nonce::assume_unique_for_key(nonce), Aad::from(&data[..HEADER]), &mut body).ok()?;
    Some(plain.to_vec())
}

/// The contents of a vault and the key, from the master password. `Err(None)`: wrong password
/// or damaged file; `Err(Some(_))`: not a vault, or its cost cannot be used.
pub fn open(password: &str, data: &[u8]) -> Result<(Key, Vec<u8>), Option<String>> {
    let (salt, cost, _) = read_header(data).ok_or(Some("not a vault".to_string()))?;
    let key = derive(password, salt, cost).map_err(Some)?;
    let plain = open_with(&key, data).ok_or(None)?;
    Ok((key, plain))
}

/// How strong a new master password is, from 0 (guessed at once) to 4 (very strong).
pub fn strength(password: &str) -> u8 {
    u8::from(zxcvbn::zxcvbn(password, &[]).score())
}

#[cfg(test)]
mod tests {
    /// Small costs, so that the tests run fast; the format is the same.
    const CHEAP: (u32, u32, u32) = (64, 1, 1);

    #[test]
    fn sealed_and_opened() {
        let key = super::new_key_with("correct horse battery staple", CHEAP).unwrap();
        let data = super::seal(&key, b"[{\"name\":\"NAS\"}]").unwrap();
        assert!(super::is_vault(&data));
        assert!(!data.windows(3).any(|w| w == b"NAS"), "the contents are not in clear");
        let (again, plain) = super::open("correct horse battery staple", &data).unwrap();
        assert_eq!(plain, b"[{\"name\":\"NAS\"}]");
        assert_eq!(super::open_with(&again, &data).unwrap(), plain);
        // Each save has its own nonce: the same contents give another file.
        assert_ne!(super::seal(&key, &plain).unwrap(), data);
    }

    #[test]
    fn wrong_password_or_changed_byte_refused() {
        let key = super::new_key_with("correct horse battery staple", CHEAP).unwrap();
        let data = super::seal(&key, b"secret").unwrap();
        assert!(matches!(super::open("Correct horse battery staple", &data), Err(None)));
        for at in [3, super::HEADER - 1, data.len() - 1] {
            let mut bad = data.clone();
            bad[at] ^= 1;
            assert!(super::open_with(&key, &bad).is_none(), "byte {at}");
        }
        assert!(matches!(super::open("x", b"[]"), Err(Some(_))));
    }

    #[test]
    fn strength_grows() {
        assert!(super::strength("password") <= 1);
        assert!(super::strength("Tramonto-viola-sul-Gran-Sasso-1976") >= 3);
    }
}
