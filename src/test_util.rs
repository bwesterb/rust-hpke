use crate::{
    aead::{Aead, AeadCtx, AeadKey, AeadNonce, AssociatedData},
    dh::DiffieHellman,
    kdf::Kdf,
    op_mode::{OpModeR, OpModeS, Psk, PskBundle},
    setup::ExporterSecret,
};

use rand::{Rng, RngCore};

/// Makes an random PSK bundle
pub(crate) fn gen_psk_bundle<K: Kdf>() -> PskBundle<K> {
    let mut csprng = rand::thread_rng();

    let psk = {
        let mut buf = vec![0u8; 32];
        csprng.fill_bytes(buf.as_mut_slice());
        Psk::<K>::from_bytes(buf)
    };
    let psk_id = {
        let mut buf = [0u8; 32];
        csprng.fill_bytes(&mut buf);
        buf.to_vec()
    };

    PskBundle::<K> { psk, psk_id }
}

/// Creates a pair of `AeadCtx`s without doing a key exchange
pub(crate) fn gen_ctx_simple_pair<A: Aead, K: Kdf>() -> (AeadCtx<A, K>, AeadCtx<A, K>) {
    let mut csprng = rand::thread_rng();

    // Initialize the key and nonce
    let key = {
        let mut buf = AeadKey::<A>::default();
        csprng.fill_bytes(buf.as_mut_slice());
        buf
    };
    let nonce = {
        let mut buf = AeadNonce::<A>::default();
        csprng.fill_bytes(buf.as_mut_slice());
        buf
    };
    let exporter_secret = {
        let mut buf = ExporterSecret::<K>::default();
        csprng.fill_bytes(buf.as_mut_slice());
        buf
    };

    let ctx1 = AeadCtx::new(key.clone(), nonce.clone(), exporter_secret.clone());
    let ctx2 = AeadCtx::new(key.clone(), nonce.clone(), exporter_secret.clone());

    (ctx1, ctx2)
}

#[derive(Clone, Copy)]
pub(crate) enum OpModeKind {
    Base,
    Auth,
    Psk,
    AuthPsk,
}

/// Makes an agreeing pair of `OpMode`s of the specified variant
pub(crate) fn gen_op_mode_pair<Dh: DiffieHellman, K: Kdf>(
    kind: OpModeKind,
) -> (OpModeS<Dh, K>, OpModeR<Dh, K>) {
    let mut csprng = rand::thread_rng();
    let (sk_sender_id, pk_sender_id) = Dh::gen_keypair(&mut csprng);
    let psk_bundle = gen_psk_bundle::<K>();

    match kind {
        OpModeKind::Base => {
            let sender_mode = OpModeS::Base;
            let receiver_mode = OpModeR::Base;
            (sender_mode, receiver_mode)
        }
        OpModeKind::Psk => {
            let sender_mode = OpModeS::Psk(psk_bundle.clone());
            let receiver_mode = OpModeR::Psk(psk_bundle);
            (sender_mode, receiver_mode)
        }
        OpModeKind::Auth => {
            let sender_mode = OpModeS::Auth(sk_sender_id);
            let receiver_mode = OpModeR::Auth(pk_sender_id);
            (sender_mode, receiver_mode)
        }
        OpModeKind::AuthPsk => {
            let sender_mode = OpModeS::AuthPsk(sk_sender_id, psk_bundle.clone());
            let receiver_mode = OpModeR::AuthPsk(pk_sender_id, psk_bundle);
            (sender_mode, receiver_mode)
        }
    }
}

/// Evaluates the equivalence of two encryption contexts by doing some encryption-decryption
/// round trips. Returns `true` iff the contexts are equal after 1000 iterations
pub(crate) fn aead_ctx_eq<A: Aead, K: Kdf>(
    ctx1: &mut AeadCtx<A, K>,
    ctx2: &mut AeadCtx<A, K>,
) -> bool {
    let mut csprng = rand::thread_rng();

    // Some random input data
    let msg = {
        let len = csprng.gen::<u8>();
        let mut buf = vec![0u8; len as usize];
        csprng.fill_bytes(&mut buf);
        buf
    };
    let aad_bytes = {
        let len = csprng.gen::<u8>();
        let mut buf = vec![0u8; len as usize];
        csprng.fill_bytes(&mut buf);
        buf
    };
    let aad = AssociatedData(&aad_bytes);

    // Do 1000 iterations of encryption-decryption. The underlying sequence number increments
    // each time.
    for i in 0..1000 {
        let mut plaintext = msg.clone();
        // Encrypt the plaintext
        let tag = ctx1
            .seal(&mut plaintext[..], aad)
            .expect(&format!("seal() #{} failed", i));
        // Rename for clarity
        let mut ciphertext = plaintext;

        // Now to decrypt on the other side
        if let Err(_) = ctx2.open(&mut ciphertext[..], aad, &tag) {
            // An error occurred in decryption. These encryption contexts are not identical.
            return false;
        }
        // Rename for clarity
        let roundtrip_plaintext = ciphertext;

        // Make sure the output message was the same as the input message. If it doesn't match,
        // early return
        if msg != roundtrip_plaintext {
            return false;
        }
    }

    true
}
