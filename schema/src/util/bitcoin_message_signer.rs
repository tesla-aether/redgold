use crate::structs::Hash;

const SIGN_MESSAGE_HEADER: &str = "Redgold: \n";

pub fn prepare_message(msg: String) -> String {
    let message = format!("{}{}", SIGN_MESSAGE_HEADER, msg);
    message
}

pub fn prepare_message_sign(msg: String) -> Vec<u8> {
    let message = prepare_message(msg);
    bitcoin::util::misc::signed_msg_hash(&*message).to_vec()
}

pub fn message_from_hash(hash: &Hash) -> String {
    let msg = hash.hex();
    prepare_message(msg)
}

