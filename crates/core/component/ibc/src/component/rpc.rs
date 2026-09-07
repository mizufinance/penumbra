use super::HostInterface;

mod client_query;
mod connection_query;
mod consensus_query;
mod utils;

use std::marker::PhantomData;

#[derive(Clone)]
pub struct IbcQuery<HI: HostInterface> {
    storage: cnidarium::Storage,
    _marker: PhantomData<HI>,
}

impl<HI: HostInterface> IbcQuery<HI> {
    pub fn new(storage: cnidarium::Storage) -> Self {
        Self {
            storage,
            _marker: PhantomData,
        }
    }
}
