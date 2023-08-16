use crate::structs::{ErrorInfo, Output, StandardContractType, TransactionAmount, UtxoEntry};
use crate::transaction::amount_data;
use crate::{Address, HashClear, SafeOption};

pub fn output_data(address: Vec<u8>, amount: u64) -> Output {
    Output::new(&Address::address_data(address).expect(""), amount as i64)
}

pub fn tx_output_data(address: Address, amount: u64) -> Output {
    Output::new(&address, amount as i64)
}

impl HashClear for Output {
    fn hash_clear(&mut self) {}
}

impl Output {

    pub fn new(address: &Address, amount: i64) -> Output {
        Output {
            address: Some(address.clone()),
            product_id: None,
            counter_party_proofs: vec![],
            data: amount_data(amount as u64),
            contract: None,
            output_type: None,
            utxo_id: None
        }
    }

    pub fn is_swap(&self) -> bool {
        self.contract.as_ref().and_then(|c| c.standard_contract_type)
            .filter(|&c| c == StandardContractType::Swap as i32).is_some()
    }

    pub fn to_utxo_entry(
        &self,
        transaction_hash: &Vec<u8>,
        output_index: u32,
        time: u64,
    ) -> UtxoEntry {
        return UtxoEntry::from_output(self, transaction_hash, output_index as i64, time as i64);
    }

    pub fn amount(&self) -> u64 {
        self.data.as_ref().unwrap().amount.unwrap() as u64
    }

    pub fn safe_ensure_amount(&self) -> Result<&i64, ErrorInfo> {
        self.data.safe_get_msg("Missing data field on output")?
            .amount.safe_get_msg("Missing amount field on output")
    }

    pub fn opt_amount(&self) -> Option<i64> {
        self.data.safe_get_msg("Missing data field on output").ok().and_then(|data| data.amount)
    }

    pub fn opt_amount_typed(&self) -> Option<TransactionAmount> {
        self.data.safe_get_msg("Missing data field on output").ok().and_then(|data| data.amount)
            .map(|a| TransactionAmount::from(a))
    }

    pub fn rounded_amount(&self) -> f64 {
        crate::transaction::rounded_balance(self.amount())
    }
}
