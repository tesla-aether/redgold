use redgold_schema::{ProtoHashable, RgResult, SafeOption};
use redgold_schema::structs::{Proof, PublicKey, Request, Response};
use crate::{KeyPair, TestConstants};
use crate::proof_support::ProofSupport;

pub trait RequestSupport {
    fn with_auth(&mut self, key_pair: &KeyPair) -> &mut Request;
    fn verify_auth(&self) -> RgResult<PublicKey>;
}

impl RequestSupport for Request {

    fn with_auth(&mut self, key_pair: &KeyPair) -> &mut Request {
        let hash = self.calculate_hash();
        // println!("with_auth hash: {:?}", hash.hex());
        let proof = Proof::from_keypair_hash(&hash, &key_pair);
        proof.verify(&hash).expect("immediate verify");
        self.proof = Some(proof);
        self
    }

    fn verify_auth(&self) -> RgResult<PublicKey> {
        let hash = self.calculate_hash();
        let proof = self.proof.safe_get_msg("Missing proof on request authentication verification")?;
        proof.verify(&hash)?;
        let pk = proof.public_key.safe_get_msg("Missing public key on request authentication verification")?;
        Ok(pk.clone())
    }

}

pub trait ResponseSupport {
    fn with_auth(&mut self, key_pair: &KeyPair) -> &mut Response;
}

impl ResponseSupport for Response {

    fn with_auth(&mut self, key_pair: &KeyPair) -> &mut Response {
        let hash = self.calculate_hash();
        let proof = Proof::from_keypair_hash(&hash, &key_pair);
        self.proof = Some(proof);
        self
    }

}


#[test]
fn verify_request_auth() {
    let tc = TestConstants::new();
    let mut req = Request::empty();
    req.about();
    req.with_auth(&tc.key_pair());
    // println!("after with auth assign proof {}", req.calculate_hash().hex());
    req.verify_auth().unwrap();

}