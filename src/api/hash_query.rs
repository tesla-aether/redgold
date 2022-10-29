use redgold_schema::from_hex;
use redgold_schema::structs::{Address, AddressInfo, ErrorInfo, HashSearchResponse, TransactionInfo};
use crate::core::relay::Relay;
use crate::data::data_store::DataStore;

pub async fn hash_query(relay: Relay, hash_input: String) -> Result<HashSearchResponse, ErrorInfo> {
    let mut response = HashSearchResponse {
        transaction_info: None,
        address_info: None,
        observation: None,
        peer_data: None,
    };
    if let Ok(a) = Address::parse(hash_input.clone()) {
        let res = DataStore::map_err_sqlx(relay.ds.query_utxo_address(vec![a.clone()]).await)?;
        let mut bal = 0;
        for r in &res {
            if let Some(o) = &r.output {
                if let Some(d) = &o.data {
                    if let Some(a) = d.amount {
                        bal += a;
                    }
                }
            }
        }
        response.address_info = Some(AddressInfo {
            address: Some(a.clone()),
            utxo_entries: res,
            balance: bal
        });

        return Ok(response);
    } else {
        let h = from_hex(hash_input)?;
        let transaction = DataStore::map_err(relay.ds.query_transaction(&h))?;
        let mut observation_proofs = vec![];
        if let Some(t) = transaction.clone() {
            observation_proofs = DataStore::map_err(relay.ds.query_observation_edge(h.clone()))?;
        }
        response.transaction_info = Some(TransactionInfo{
            transaction,
            observation_proofs
        })
    }
    Ok(response)
}