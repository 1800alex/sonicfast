//! HAP cryptographic primitives. No protocol bytes, PINs or keys are logged.
use num_bigint::BigUint;
use ring::{aead, agreement, hkdf, rand::SystemRandom, signature};
use sha2::{Digest, Sha512};
use std::collections::BTreeMap;
use subtle::ConstantTimeEq;

pub(super) type Result<T> = std::result::Result<T, super::pairing::Error>;
use super::pairing::Error;
pub(super) type Tlv = BTreeMap<u8, Vec<u8>>;

pub(super) fn encode(fields: &[(u8, &[u8])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for &(tag, value) in fields {
        for chunk in value.chunks(255) {
            bytes.extend([tag, chunk.len() as u8]);
            bytes.extend(chunk);
        }
    }
    bytes
}
pub(super) fn decode(mut bytes: &[u8]) -> Result<Tlv> {
    let mut fields = Tlv::new();
    let mut previous = None;
    while !bytes.is_empty() {
        if bytes.len() < 2 {
            return Err(Error::Protocol);
        }
        let (tag, len) = (bytes[0], usize::from(bytes[1]));
        if bytes.len() < len + 2 {
            return Err(Error::Protocol);
        }
        if fields.contains_key(&tag) && previous != Some((tag, 255)) {
            return Err(Error::Protocol);
        }
        fields.entry(tag).or_default().extend(&bytes[2..2 + len]);
        previous = Some((tag, len));
        bytes = &bytes[2 + len..];
    }
    Ok(fields)
}
pub(super) fn field(fields: &Tlv, tag: u8, len: std::ops::RangeInclusive<usize>) -> Result<&[u8]> {
    fields
        .get(&tag)
        .filter(|v| len.contains(&v.len()))
        .map(Vec::as_slice)
        .ok_or(Error::Protocol)
}
pub(super) fn step(bytes: &[u8], sequence: u8) -> Result<Tlv> {
    let fields = decode(bytes)?;
    if let Some(error) = fields.get(&7) {
        return Err(match error.as_slice() {
            [2] => Error::Authentication,
            [3..=5] => Error::Backoff,
            _ => Error::Protocol,
        });
    }
    if field(&fields, 6, 1..=1)? != [sequence] {
        return Err(Error::Protocol);
    }
    Ok(fields)
}
pub(super) fn derive(secret: &[u8], salt: &str, info: &str) -> Result<[u8; 32]> {
    let prk = hkdf::Salt::new(hkdf::HKDF_SHA512, salt.as_bytes()).extract(secret);
    let infos = [info.as_bytes()];
    let mut key = [0; 32];
    prk.expand(&infos, hkdf::HKDF_SHA256)
        .map_err(|_| Error::Protocol)?
        .fill(&mut key)
        .map_err(|_| Error::Protocol)?;
    Ok(key)
}
pub(super) fn seal(key: &[u8; 32], nonce: &[u8], aad: &[u8], bytes: &mut Vec<u8>) -> Result<()> {
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::CHACHA20_POLY1305, key).map_err(|_| Error::Protocol)?,
    );
    let mut padded = [0; 12];
    if nonce.len() > 12 {
        return Err(Error::Protocol);
    }
    padded[12 - nonce.len()..].copy_from_slice(nonce);
    key.seal_in_place_append_tag(
        aead::Nonce::assume_unique_for_key(padded),
        aead::Aad::from(aad),
        bytes,
    )
    .map_err(|_| Error::Authentication)
}
pub(super) fn open(key: &[u8; 32], nonce: &[u8], aad: &[u8], bytes: &[u8]) -> Result<Vec<u8>> {
    let key = aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::CHACHA20_POLY1305, key).map_err(|_| Error::Protocol)?,
    );
    let mut padded = [0; 12];
    if nonce.len() > 12 {
        return Err(Error::Protocol);
    }
    padded[12 - nonce.len()..].copy_from_slice(nonce);
    let mut bytes = bytes.to_vec();
    let len = key
        .open_in_place(
            aead::Nonce::assume_unique_for_key(padded),
            aead::Aad::from(aad),
            &mut bytes,
        )
        .map_err(|_| Error::Authentication)?
        .len();
    bytes.truncate(len);
    Ok(bytes)
}
pub(super) fn signing(seed: &[u8]) -> Result<signature::Ed25519KeyPair> {
    signature::Ed25519KeyPair::from_seed_unchecked(seed).map_err(|_| Error::Protocol)
}
pub(super) fn verify(key: &[u8], message: &[u8], sig: &[u8]) -> Result<()> {
    signature::UnparsedPublicKey::new(&signature::ED25519, key)
        .verify(message, sig)
        .map_err(|_| Error::Authentication)
}
pub(super) fn exchange() -> Result<(agreement::EphemeralPrivateKey, Vec<u8>)> {
    let key = agreement::EphemeralPrivateKey::generate(&agreement::X25519, &SystemRandom::new())
        .map_err(|_| Error::Protocol)?;
    let public = key
        .compute_public_key()
        .map_err(|_| Error::Protocol)?
        .as_ref()
        .to_vec();
    Ok((key, public))
}
pub(super) fn agree(key: agreement::EphemeralPrivateKey, public: &[u8]) -> Result<Vec<u8>> {
    agreement::agree_ephemeral(
        key,
        &agreement::UnparsedPublicKey::new(&agreement::X25519, public),
        |v| v.to_vec(),
    )
    .map_err(|_| Error::Authentication)
}
fn hash(parts: &[&[u8]]) -> Vec<u8> {
    let mut h = Sha512::new();
    for part in parts {
        h.update(part);
    }
    h.finalize().to_vec()
}
fn number(parts: &[&[u8]]) -> BigUint {
    BigUint::from_bytes_be(&hash(parts))
}
// RFC 5054 appendix A, 3072-bit group (generator 5), as required by HAP.
pub(super) fn prime() -> BigUint {
    BigUint::parse_bytes(
        concat!(
            "FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E08",
            "8A67CC74020BBEA63B139B22514A08798E3404DDEF9519B3CD",
            "3A431B302B0A6DF25F14374FE1356D6D51C245E485B576625",
            "E7EC6F44C42E9A637ED6B0BFF5CB6F406B7EDEE386BFB5A8",
            "99FA5AE9F24117C4B1FE649286651ECE45B3DC2007CB8A163",
            "BF0598DA48361C55D39A69163FA8FD24CF5F83655D23DCA3A",
            "D961C62F356208552BB9ED529077096966D670C354E4ABC9804",
            "F1746C08CA18217C32905E462E36CE3BE39E772C180E86039B",
            "2783A2EC07A28FB5C55DF06F4C52C9DE2BCBF695581718399",
            "5497CEA956AE515D2261898FA051015728E5A8AAAC42DAD331",
            "70D04507A33A85521ABDF1CBA64ECFB850458DBEF0A8AEA715",
            "75D060C7DB3970F85A6E1E4C7ABF5AE8CDB0933D71E8C94E",
            "04A25619DCEE3D2261AD2EE6BF12FFA06D98A0864D8760273",
            "3EC86A64521F2B18177B200CBBE117577A615D6C770988C0BA",
            "D946E208E24FA074E5AB3143DB5BFCE0FD108E4B82D120A93",
            "AD2CAFFFFFFFFFFFFFFFF"
        )
        .as_bytes(),
        16,
    )
    .unwrap()
}
fn pad(n: &BigUint) -> Vec<u8> {
    let bytes = n.to_bytes_be();
    let mut padded = vec![0; 384 - bytes.len()];
    padded.extend(bytes);
    padded
}
pub(super) struct Srp {
    pub public: Vec<u8>,
    pub proof: Vec<u8>,
    pub key: Vec<u8>,
    expected: Vec<u8>,
}
impl Srp {
    pub fn new(pin: &str, salt: &[u8], server: &[u8], private: &[u8]) -> Result<Self> {
        if salt.len() != 16 || server.len() > 384 || server.is_empty() {
            return Err(Error::Protocol);
        }
        let n = prime();
        let g = BigUint::from(5u8);
        let b = BigUint::from_bytes_be(server);
        if b == BigUint::default() || b >= n {
            return Err(Error::Authentication);
        }
        let a = BigUint::from_bytes_be(private);
        let public = g.modpow(&a, &n);
        let k = number(&[&n.to_bytes_be(), &pad(&g)]);
        let u = number(&[&pad(&public), &pad(&b)]);
        if u == BigUint::default() {
            return Err(Error::Authentication);
        }
        let x = number(&[salt, &hash(&[b"Pair-Setup:", pin.as_bytes()])]);
        let base = (&b + &n - (k * g.modpow(&x, &n)) % &n) % &n;
        let secret = base.modpow(&(a + u * x), &n);
        let key = hash(&[&secret.to_bytes_be()]);
        let xor: Vec<u8> = hash(&[&n.to_bytes_be()])
            .iter()
            .zip(hash(&[&g.to_bytes_be()]))
            .map(|(a, b)| a ^ b)
            .collect();
        let public = public.to_bytes_be();
        let proof = hash(&[&xor, &hash(&[b"Pair-Setup"]), salt, &public, server, &key]);
        let expected = hash(&[&public, &proof, &key]);
        Ok(Self {
            public,
            proof,
            key,
            expected,
        })
    }
    pub fn verify(&self, proof: &[u8]) -> Result<()> {
        if bool::from(self.expected.ct_eq(proof)) {
            Ok(())
        } else {
            Err(Error::Authentication)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srp_matches_independent_srptools_vector_and_rejects_bad_server_proofs() {
        // Generated with srptools 1.0.1 (SRPClientSession/SRPServerSession),
        // RFC 5054 group 3072 / SHA-512. Synthetic credentials only.
        let v: std::collections::HashMap<String, String> =
            serde_json::from_str(include_str!("../../testdata/airplay/srp-3072.json")).unwrap();
        let bytes = |name: &str| -> Vec<u8> {
            v[name]
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect()
        };
        let srp = Srp::new(
            &v["pin"],
            &bytes("salt"),
            &bytes("server_public"),
            &bytes("private"),
        )
        .unwrap();
        assert_eq!(srp.public, bytes("client_public"));
        assert_eq!(srp.key, bytes("key"));
        assert_eq!(srp.proof, bytes("proof"));
        srp.verify(&bytes("server_proof")).unwrap();
        let mut invalid = bytes("server_proof");
        invalid[0] ^= 1;
        assert!(srp.verify(&invalid).is_err());
        assert!(Srp::new("0427", &bytes("salt"), &[0], &bytes("private")).is_err());
        assert!(
            Srp::new(
                "0427",
                &bytes("salt"),
                &prime().to_bytes_be(),
                &bytes("private")
            )
            .is_err()
        );
    }
    #[test]
    fn tlv_fragments_are_bounded_and_errors_are_typed() {
        let long = vec![5; 600];
        assert_eq!(
            field(&decode(&encode(&[(3, &long)])).unwrap(), 3, 600..=600).unwrap(),
            long
        );
        for invalid in [&[1][..], &[1, 4, 0], &[1, 1, 0, 1, 1, 0]] {
            assert!(decode(invalid).is_err());
        }
        assert!(matches!(
            step(&[6, 1, 4, 7, 1, 2], 4),
            Err(Error::Authentication)
        ));
        assert!(matches!(step(&[6, 1, 4, 7, 1, 3], 4), Err(Error::Backoff)));
        assert!(step(&[6, 1, 2], 4).is_err());
        assert_eq!(prime().bits(), 3072);
    }
    #[test]
    fn authenticated_cipher_rejects_wrong_key_nonce_aad_and_modified_payload() {
        let mut encrypted = b"private RTSP".to_vec();
        seal(&[7; 32], b"PV-Msg02", b"aad", &mut encrypted).unwrap();
        assert_eq!(
            open(&[7; 32], b"PV-Msg02", b"aad", &encrypted).unwrap(),
            b"private RTSP"
        );
        assert!(open(&[8; 32], b"PV-Msg02", b"aad", &encrypted).is_err());
        assert!(open(&[7; 32], b"PV-Msg03", b"aad", &encrypted).is_err());
        assert!(open(&[7; 32], b"PV-Msg02", b"bad", &encrypted).is_err());
        encrypted[0] ^= 1;
        assert!(open(&[7; 32], b"PV-Msg02", b"aad", &encrypted).is_err());
    }
}
